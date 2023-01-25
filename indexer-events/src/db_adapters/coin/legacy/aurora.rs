use crate::db_adapters;
use crate::db_adapters::coin;
use crate::db_adapters::{numeric_types, Event};
use crate::models::fungible_token_events::FungibleTokenEvent;
use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;
use near_primitives::borsh;
use near_primitives::borsh::{BorshDeserialize, BorshSerialize};
use near_primitives::types::AccountId;
use near_primitives::views::{ActionView, ExecutionStatusView, ReceiptEnumView};
use serde::Deserialize;
use std::io;
use std::ops::Mul;
use std::str::FromStr;

#[derive(Deserialize, Debug, Clone)]
struct FtTransfer {
    pub receiver_id: AccountId,
    pub amount: numeric_types::U128,
    pub memo: Option<String>,
}

// Took from the link below + some places around
// https://github.com/aurora-is-near/aurora-engine/blob/master/engine-types/src/parameters.rs
/// withdraw NEAR eth-connector call args
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct WithdrawCallArgs {
    pub recipient_address: Address,
    pub amount: numeric_types::U128,
}

/// Base Eth Address type
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Address(primitive_types::H160);

impl BorshSerialize for Address {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(self.0.as_bytes())
    }
}

impl BorshDeserialize for Address {
    fn deserialize(buf: &mut &[u8]) -> io::Result<Self> {
        if buf.len() < 20 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "ETH_WRONG_ADDRESS_LENGTH",
            ));
        }
        // Guaranty no panics. The length checked early
        let address = Self(primitive_types::H160::from_slice(&buf[..20]));
        *buf = &buf[20..];
        Ok(address)
    }
}

pub(crate) async fn collect_aurora(
    shard_id: &near_indexer_primitives::types::ShardId,
    receipt_execution_outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
) -> anyhow::Result<Vec<FungibleTokenEvent>> {
    let mut events: Vec<FungibleTokenEvent> = vec![];
    for outcome in receipt_execution_outcomes {
        if outcome.receipt.receiver_id != AccountId::from_str("aurora")?
            || !db_adapters::events::extract_events(outcome).is_empty()
        {
            continue;
        }
        if let ReceiptEnumView::Action { actions, .. } = &outcome.receipt.receipt {
            for action in actions {
                events.extend(process_aurora_functions(block_header, action, outcome).await?);
            }
        }
    }
    coin::filter_zeros_and_enumerate_events(
        &mut events,
        shard_id,
        block_header.timestamp,
        &Event::Aurora,
    )?;
    Ok(events)
}

async fn process_aurora_functions(
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
        "new",
        "call",
        "new_eth_connector",
        "set_eth_connector_contract_data",
        "deposit",
        "storage_deposit",
        "submit",
        "deploy_erc20_token",
        "get_nep141_from_erc20",
        "ft_on_transfer",
        "ft_balance_of",
        "ft_metadata",
        "ft_total_supply",
    ]
    .contains(&method_name.as_str())
    {
        return Ok(vec![]);
    }

    // MINT may produce several events, where involved_account_id is always NULL
    // deposit do not mint anything; mint goes in finish_deposit
    if method_name == "finish_deposit" {
        let mut events = vec![];
        for log in &outcome.execution_outcome.outcome.logs {
            lazy_static::lazy_static! {
                static ref RE: regex::Regex = regex::Regex::new(r"^Mint (?P<amount>(0|[1-9][0-9]*)) nETH tokens for: (?P<account_id>[a-z0-9_\.\-]+)$").unwrap();
            }

            if let Some(cap) = RE.captures(log) {
                let amount = match cap.name("amount") {
                    Some(x) => x.as_str(),
                    None => anyhow::bail!("Unexpected mint log format in aurora: {}\n Expected format: Mint <amount> nETH tokens for: <account_id>", log)
                };
                if amount == "0" {
                    continue;
                }
                let account_id = match cap.name("account_id") {
                    Some(x) => x.as_str(),
                    None => anyhow::bail!("Unexpected mint log format in aurora: {}\n Expected format: Mint <amount> nETH tokens for: <account_id>", log)
                };

                let delta = BigDecimal::from_str(amount)?;
                let base = db_adapters::get_base(Event::Aurora, outcome, block_header)?;
                let custom = coin::FtEvent {
                    affected_id: AccountId::from_str(account_id)?,
                    involved_id: None,
                    delta,
                    cause: "MINT".to_string(),
                    memo: None,
                };
                events.push(coin::build_event(base, custom).await?);
            };
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

        let base_from = db_adapters::get_base(Event::Aurora, outcome, block_header)?;
        let custom_from = coin::FtEvent {
            affected_id: outcome.receipt.predecessor_id.clone(),
            involved_id: Some(ft_transfer_args.receiver_id.clone()),
            delta: negative_delta,
            cause: "TRANSFER".to_string(),
            memo: memo.clone(),
        };

        let base_to = db_adapters::get_base(Event::Aurora, outcome, block_header)?;
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
        let mut events = vec![];
        for log in &outcome.execution_outcome.outcome.logs {
            lazy_static::lazy_static! {
                static ref RE: regex::Regex = regex::Regex::new(r"^Refund amount (?P<amount>(0|[1-9][0-9]*)) from (?P<from_account_id>[a-z0-9_\.\-]+) to (?P<to_account_id>[a-z0-9_\.\-]+)$").unwrap();
            }

            if let Some(cap) = RE.captures(log) {
                let amount = match cap.name("amount") {
                    Some(x) => x.as_str(),
                    None => anyhow::bail!("Unexpected ft_resolve_transfer log format in aurora: {}\n Expected format: Refund amount <amount> from <account_id> to <account_id>", log)
                };
                if amount == "0" {
                    continue;
                }
                let from_account_id = match cap.name("from_account_id") {
                    Some(x) => x.as_str(),
                    None => anyhow::bail!("Unexpected ft_resolve_transfer log format in aurora: {}\n Expected format: Refund amount <amount> from <account_id> to <account_id>", log)
                };
                let to_account_id = match cap.name("to_account_id") {
                    Some(x) => x.as_str(),
                    None => anyhow::bail!("Unexpected ft_resolve_transfer log format in aurora: {}\n Expected format: Refund amount <amount> from <account_id> to <account_id>", log)
                };

                let delta = BigDecimal::from_str(amount)?;
                let negative_delta = delta.clone().mul(BigDecimal::from(-1));

                let base_from = db_adapters::get_base(Event::Aurora, outcome, block_header)?;
                let custom_from = coin::FtEvent {
                    affected_id: AccountId::from_str(from_account_id)?,
                    involved_id: Some(AccountId::from_str(to_account_id)?),
                    delta: negative_delta,
                    cause: "TRANSFER".to_string(),
                    memo: None,
                };
                events.push(coin::build_event(base_from, custom_from).await?);

                let base_to = db_adapters::get_base(Event::Aurora, outcome, block_header)?;
                let custom_to = coin::FtEvent {
                    affected_id: AccountId::from_str(to_account_id)?,
                    involved_id: Some(AccountId::from_str(from_account_id)?),
                    delta,
                    cause: "TRANSFER".to_string(),
                    memo: None,
                };
                events.push(coin::build_event(base_to, custom_to).await?);
            };
        }
        return Ok(events);
    }

    if method_name == "withdraw" {
        let args = match WithdrawCallArgs::try_from_slice(args) {
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
            BigDecimal::from_str(&args.amount.0.to_string())?.mul(BigDecimal::from(-1));
        let base = db_adapters::get_base(Event::Aurora, outcome, block_header)?;
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
        "AURORA {} new method found: {}, receipt {}",
        block_header.height,
        method_name,
        outcome.receipt.receipt_id
    );
    Ok(vec![])
}
