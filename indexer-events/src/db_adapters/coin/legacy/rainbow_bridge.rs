use crate::db_adapters;
use crate::db_adapters::{coin, numeric_types, Event};
use crate::models::fungible_token_events::FungibleTokenEvent;
use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;
use near_primitives::types::AccountId;
use near_primitives::views::{ActionView, ExecutionStatusView, ReceiptEnumView};
use serde::Deserialize;
use std::ops::{Mul, Sub};
use std::str::FromStr;

#[derive(Deserialize, Debug, Clone)]
struct Mint {
    pub account_id: AccountId,
    pub amount: String,
}

#[derive(Deserialize, Debug, Clone)]
struct FtTransfer {
    pub receiver_id: AccountId,
    pub amount: numeric_types::U128,
    pub memo: Option<String>,
    // https://explorer.near.org/transactions/6PamJFeTSkcncpaNQwGr3F7VaH64pNwVnqMFDFdF4Txd#4HzoQHKjf1N41g5ppZhqtv3anaQZod6En9U25QGnWGmC
    // Usually people pass args as the dict, but in tx above it's a list
    // "Classic" args as the reference:
    // https://explorer.near.org/transactions/8mkXd67wxzdgtP1v1GE4a6PWcSPtQ1W6Sjh3CNkjc6bK#EQjU8UqeEx8E67HPhYGvUUrCSg6kb9rYREuz72GfwWCm
    // We can ignore this field, it's internal info of the contracts, we have it here only to help serde
    #[allow(dead_code)]
    pub msg: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct FtRefund {
    pub receiver_id: AccountId,
    pub sender_id: AccountId,
    pub amount: numeric_types::U128,
    pub memo: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct Withdraw {
    pub amount: numeric_types::U128,
    // Contains internal info of bridged contracts. It's not NEAR account_id
    // pub recipient: AccountId,
}

pub(crate) async fn collect_rainbow_bridge(
    shard_id: &near_indexer_primitives::types::ShardId,
    receipt_execution_outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
) -> anyhow::Result<Vec<FungibleTokenEvent>> {
    let mut events: Vec<FungibleTokenEvent> = vec![];

    for outcome in receipt_execution_outcomes {
        if !is_rainbow_bridge_contract(outcome.receipt.receiver_id.as_str())
            || !db_adapters::events::extract_events(outcome).is_empty()
        {
            continue;
        }
        if let ReceiptEnumView::Action { actions, .. } = &outcome.receipt.receipt {
            for action in actions {
                match action {
                    ActionView::Delegate {
                        delegate_action, ..
                    } => {
                        for non_delegate_action in &delegate_action.actions {
                            events.extend(
                                process_rainbow_bridge_functions(
                                    block_header,
                                    &coin::legacy::to_action_view(non_delegate_action.clone()),
                                    outcome,
                                )
                                .await?,
                            )
                        }
                    }
                    _ => events.extend(
                        process_rainbow_bridge_functions(block_header, action, outcome).await?,
                    ),
                }
            }
        }
    }

    coin::filter_zeros_and_enumerate_events(
        &mut events,
        shard_id,
        block_header.timestamp,
        &Event::RainbowBridge,
    )?;

    Ok(events)
}

fn is_rainbow_bridge_contract(contract_id: &str) -> bool {
    if let Some(contract_prefix) = contract_id.strip_suffix(".factory.bridge.near") {
        lazy_static::lazy_static! {
            static ref RE: regex::Regex = regex::Regex::new("^[a-f0-9]+$").unwrap();
        }
        contract_prefix.len() == 40 && RE.is_match(contract_prefix)
    } else {
        false
    }
}

async fn process_rainbow_bridge_functions(
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    action: &ActionView,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
) -> anyhow::Result<Vec<FungibleTokenEvent>> {
    let (method_name, args, ..) = match action {
        ActionView::FunctionCall {
            method_name,
            args,
            deposit,
            ..
        } => (method_name, args, deposit),
        _ => return Ok(vec![]),
    };

    if vec![
        "storage_deposit",
        "finish_deposit",
        "verify_log_entry",
        "ft_balance_of",
        "ft_metadata",
        "set_metadata",
        "ft_total_supply",
    ]
    .contains(&method_name.as_str())
    {
        return Ok(vec![]);
    }

    // MINT produces 1 event, where involved_account_id is NULL
    if method_name == "mint" {
        let mint_args = match serde_json::from_slice::<Mint>(args) {
            Ok(x) => x,
            Err(err) => {
                match outcome.execution_outcome.outcome.status {
                    // We couldn't parse args for failed receipt. Let's just ignore it, we can't save it properly
                    ExecutionStatusView::Unknown | ExecutionStatusView::Failure(_) => {
                        return Ok(vec![])
                    }
                    ExecutionStatusView::SuccessValue(_)
                    | ExecutionStatusView::SuccessReceiptId(_) => {
                        anyhow::bail!(err)
                    }
                }
            }
        };
        let delta = BigDecimal::from_str(&mint_args.amount)?;
        let base = db_adapters::get_base(Event::RainbowBridge, outcome, block_header)?;
        let custom = coin::FtEvent {
            // We can't use here outcome.receipt.predecessor_id, it's usually factory.bridge.near
            affected_id: mint_args.account_id.clone(),
            involved_id: None,
            delta,
            cause: "MINT".to_string(),
            memo: None,
        };
        return Ok(vec![coin::build_event(base, custom).await?]);
    }

    // TRANSFER produces 2 events
    // 1. affected_account_id is sender, delta is negative, absolute_amount decreased
    // 2. affected_account_id is receiver, delta is positive, absolute_amount increased
    if method_name == "ft_transfer" || method_name == "ft_transfer_call" {
        let ft_transfer_args = match serde_json::from_slice::<FtTransfer>(args) {
            Ok(x) => x,
            Err(err) => {
                match outcome.execution_outcome.outcome.status {
                    // We couldn't parse args for failed receipt. Let's just ignore it, we can't save it properly
                    ExecutionStatusView::Unknown | ExecutionStatusView::Failure(_) => {
                        return Ok(vec![])
                    }
                    ExecutionStatusView::SuccessValue(_)
                    | ExecutionStatusView::SuccessReceiptId(_) => {
                        anyhow::bail!(err)
                    }
                }
            }
        };

        let delta = BigDecimal::from_str(&ft_transfer_args.amount.0.to_string())?;
        let negative_delta = delta.clone().mul(BigDecimal::from(-1));
        let memo = ft_transfer_args
            .memo
            .as_ref()
            .map(|s| s.escape_default().to_string());

        let base_from = db_adapters::get_base(Event::RainbowBridge, outcome, block_header)?;
        let custom_from = coin::FtEvent {
            affected_id: outcome.receipt.predecessor_id.clone(),
            involved_id: Some(ft_transfer_args.receiver_id.clone()),
            delta: negative_delta,
            cause: "TRANSFER".to_string(),
            memo: memo.clone(),
        };

        let base_to = db_adapters::get_base(Event::RainbowBridge, outcome, block_header)?;
        let custom_to = coin::FtEvent {
            affected_id: ft_transfer_args.receiver_id,
            involved_id: Some(outcome.receipt.predecessor_id.clone()),
            delta,
            cause: "TRANSFER".to_string(),
            memo,
        };
        return Ok(vec![
            coin::build_event(base_from, custom_from).await?,
            coin::build_event(base_to, custom_to).await?,
        ]);
    }

    // If TRANSFER failed, it could be revoked. The procedure is the same as for TRANSFER
    if method_name == "ft_resolve_transfer" {
        if outcome.execution_outcome.outcome.logs.is_empty() {
            // ft_transfer_call was successful, there's nothing to return back
            return Ok(vec![]);
        }
        let ft_refund_args = match serde_json::from_slice::<FtRefund>(args) {
            Ok(x) => x,
            Err(err) => {
                match outcome.execution_outcome.outcome.status {
                    // We couldn't parse args for failed receipt. Let's just ignore it, we can't save it properly
                    ExecutionStatusView::Unknown | ExecutionStatusView::Failure(_) => {
                        return Ok(vec![])
                    }
                    ExecutionStatusView::SuccessValue(_)
                    | ExecutionStatusView::SuccessReceiptId(_) => {
                        anyhow::bail!(err)
                    }
                }
            }
        };
        let mut delta = BigDecimal::from_str(&ft_refund_args.amount.0.to_string())?;
        // The contract may return only the part of the coins.
        // We should parse it from the output and subtract from the value from args
        if let ExecutionStatusView::SuccessValue(transferred_amount_bytes) =
            &outcome.execution_outcome.outcome.status
        {
            let transferred_amount = serde_json::from_slice::<String>(transferred_amount_bytes)?;
            delta = delta.sub(BigDecimal::from_str(&transferred_amount)?);
        }
        let negative_delta = delta.clone().mul(BigDecimal::from(-1));
        let memo = ft_refund_args
            .memo
            .as_ref()
            .map(|s| s.escape_default().to_string());

        for log in &outcome.execution_outcome.outcome.logs {
            if log == "The account of the sender was deleted" {
                // I never met this case so it's better to re-check it manually when we find it
                tracing::error!(
                    target: crate::LOGGING_PREFIX,
                    "The account of the sender was deleted {}",
                    block_header.height
                );

                // we should revert ft_transfer_call, but there's no receiver_id. We should burn tokens
                let base = db_adapters::get_base(Event::RainbowBridge, outcome, block_header)?;
                let custom = coin::FtEvent {
                    affected_id: ft_refund_args.receiver_id,
                    involved_id: None,
                    delta: negative_delta,
                    cause: "BURN".to_string(),
                    memo,
                };
                return Ok(vec![coin::build_event(base, custom).await?]);
            }
            if log.starts_with("Refund ") {
                // we should revert ft_transfer_call
                let base_from = db_adapters::get_base(Event::RainbowBridge, outcome, block_header)?;
                let custom_from = coin::FtEvent {
                    affected_id: ft_refund_args.receiver_id.clone(),
                    involved_id: Some(ft_refund_args.sender_id.clone()),
                    delta: negative_delta,
                    cause: "TRANSFER".to_string(),
                    memo: memo.clone(),
                };

                let base_to = db_adapters::get_base(Event::RainbowBridge, outcome, block_header)?;
                let custom_to = coin::FtEvent {
                    affected_id: ft_refund_args.sender_id,
                    involved_id: Some(ft_refund_args.receiver_id),
                    delta,
                    cause: "TRANSFER".to_string(),
                    memo,
                };

                return Ok(vec![
                    coin::build_event(base_from, custom_from).await?,
                    coin::build_event(base_to, custom_to).await?,
                ]);
            }
        }
        return Ok(vec![]);
    }

    // BURN produces 1 event, where involved_account_id is NULL
    if method_name == "withdraw" {
        let ft_burn_args = match serde_json::from_slice::<Withdraw>(args) {
            Ok(x) => x,
            Err(err) => {
                match outcome.execution_outcome.outcome.status {
                    // We couldn't parse args for failed receipt. Let's just ignore it, we can't save it properly
                    ExecutionStatusView::Unknown | ExecutionStatusView::Failure(_) => {
                        return Ok(vec![])
                    }
                    ExecutionStatusView::SuccessValue(_)
                    | ExecutionStatusView::SuccessReceiptId(_) => {
                        anyhow::bail!(err)
                    }
                }
            }
        };
        let negative_delta =
            BigDecimal::from_str(&ft_burn_args.amount.0.to_string())?.mul(BigDecimal::from(-1));

        let base = db_adapters::get_base(Event::RainbowBridge, outcome, block_header)?;
        let custom = coin::FtEvent {
            affected_id: outcome.receipt.predecessor_id.clone(),
            involved_id: None,
            delta: negative_delta,
            cause: "BURN".to_string(),
            memo: None,
        };
        return Ok(vec![coin::build_event(base, custom).await?]);
    }

    tracing::error!(
        target: crate::LOGGING_PREFIX,
        "RAINBOW {} new method found: {}, receipt {}",
        block_header.height,
        method_name,
        outcome.receipt.receipt_id
    );
    Ok(vec![])
}
