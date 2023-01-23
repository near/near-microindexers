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
struct FtTransfer {
    pub receiver_id: AccountId,
    pub amount: numeric_types::U128,
    pub memo: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct FtRefund {
    pub receiver_id: AccountId,
    pub sender_id: AccountId,
    pub amount: numeric_types::U128,
    pub memo: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct NearWithdraw {
    pub amount: numeric_types::U128,
}

pub(crate) async fn collect_wrap_near(
    shard_id: &near_indexer_primitives::types::ShardId,
    receipt_execution_outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
) -> anyhow::Result<Vec<FungibleTokenEvent>> {
    let mut events: Vec<FungibleTokenEvent> = vec![];

    for outcome in receipt_execution_outcomes {
        if outcome.receipt.receiver_id != AccountId::from_str("wrap.near")?
            || !db_adapters::events::extract_events(outcome).is_empty()
        {
            continue;
        }
        if let ReceiptEnumView::Action { actions, .. } = &outcome.receipt.receipt {
            for action in actions {
                events.extend(process_wrap_near_functions(block_header, action, outcome).await?);
            }
        }
    }
    coin::filter_zeros_and_enumerate_events(
        &mut events,
        shard_id,
        block_header.timestamp,
        &Event::WrapNear,
    )?;

    Ok(events)
}

// We can't take the info from function call parameters, see https://explorer.near.org/transactions/AAcncdoxDGaoM8TMMRSVuMLfrRvvmAMtU3mDbtB9L6JJ#EahNmkevAXEjXeQfP6sxxi6c53KE1pZpwzNWoXnDWDeS
// We also can't just parse logs. near_deposit and ft_transfer_call are usually have logs duplicated
async fn process_wrap_near_functions(
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    action: &ActionView,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
) -> anyhow::Result<Vec<FungibleTokenEvent>> {
    let (method_name, args) = match action {
        ActionView::FunctionCall {
            method_name, args, ..
        } => (method_name, args),
        _ => return Ok(vec![]),
    };

    if vec![
        "storage_deposit",
        "ft_balance_of",
        "ft_metadata",
        "ft_total_supply",
        "new",
    ]
    .contains(&method_name.as_str())
    {
        return Ok(vec![]);
    }

    // MINT produces 1 event, where involved_account_id is NULL
    if method_name == "near_deposit" {
        // We can't take deposit value because of
        // https://explorer.near.org/transactions/AAcncdoxDGaoM8TMMRSVuMLfrRvvmAMtU3mDbtB9L6JJ#EahNmkevAXEjXeQfP6sxxi6c53KE1pZpwzNWoXnDWDeS
        let mut events = vec![];
        for log in &outcome.execution_outcome.outcome.logs {
            if let Some(mint) = process_mint_log(block_header, outcome, log).await? {
                events.push(mint);
            }
            // there are also transfer logs, but they are duplicated, we will catch them in transfer section
        }
        return Ok(events);
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

        let base_from = db_adapters::get_base(Event::WrapNear, outcome, block_header)?;
        let custom_from = coin::FtEvent {
            affected_id: outcome.receipt.predecessor_id.clone(),
            involved_id: Some(ft_transfer_args.receiver_id.clone()),
            delta: negative_delta,
            cause: "TRANSFER".to_string(),
            memo: memo.clone(),
        };

        let base_to = db_adapters::get_base(Event::WrapNear, outcome, block_header)?;
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
                let base = db_adapters::get_base(Event::WrapNear, outcome, block_header)?;
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
                let base_from = db_adapters::get_base(Event::WrapNear, outcome, block_header)?;
                let custom_from = coin::FtEvent {
                    affected_id: ft_refund_args.receiver_id.clone(),
                    involved_id: Some(ft_refund_args.sender_id.clone()),
                    delta: negative_delta,
                    cause: "TRANSFER".to_string(),
                    memo: memo.clone(),
                };

                let base_to = db_adapters::get_base(Event::WrapNear, outcome, block_header)?;
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
    if method_name == "near_withdraw" {
        let ft_burn_args = match serde_json::from_slice::<NearWithdraw>(args) {
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

        let base = db_adapters::get_base(Event::WrapNear, outcome, block_header)?;
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
        "WRAP NEAR {} new method found: {}, receipt {}",
        block_header.height,
        method_name,
        outcome.receipt.receipt_id
    );
    Ok(vec![])
}

async fn process_mint_log(
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    log: &str,
) -> anyhow::Result<Option<FungibleTokenEvent>> {
    lazy_static::lazy_static! {
        static ref RE: regex::Regex = regex::Regex::new(r"^Deposit (?P<amount>(0|[1-9][0-9]*)) NEAR to (?P<account_id>[a-z0-9_\.\-]+)$").unwrap();
    }

    if let Some(cap) = RE.captures(log) {
        let amount = match cap.name("amount") {
                Some(x) => x.as_str(),
                None => anyhow::bail!("Unexpected deposit log format in wrap.near: {}\n Expected format: Deposit <amount> NEAR to <account_id>", log)
            };
        if amount == "0" {
            return Ok(None);
        }
        let account_id = match cap.name("account_id") {
                Some(x) => x.as_str(),
                None => anyhow::bail!("Unexpected deposit log format in wrap.near: {}\n Expected format: Deposit <amount> NEAR to <account_id>", log)
            };

        let delta = BigDecimal::from_str(amount)?;
        let base = db_adapters::get_base(Event::WrapNear, outcome, block_header)?;
        let custom = coin::FtEvent {
            affected_id: AccountId::from_str(account_id)?,
            involved_id: None,
            delta,
            cause: "MINT".to_string(),
            memo: None,
        };
        return Ok(Some(coin::build_event(base, custom).await?));
    }

    Ok(None)
}
