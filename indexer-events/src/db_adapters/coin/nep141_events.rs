use crate::db_adapters::event_types;
use crate::db_adapters::event_types::Nep141Event;
use crate::db_adapters::Event;
use crate::db_adapters::{coin, events, get_base};
use crate::models::coin_events::CoinEvent;
use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;
use near_primitives::types::AccountId;
use std::ops::Mul;
use std::str::FromStr;

pub(crate) async fn collect_nep141_events(
    shard_id: &near_indexer_primitives::types::ShardId,
    receipt_execution_outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
) -> anyhow::Result<Vec<CoinEvent>> {
    let mut res = Vec::new();
    for outcome in receipt_execution_outcomes {
        let events = events::extract_events(outcome);
        for event in events {
            if let event_types::NearEvent::Nep141(ft_events) = event {
                res.extend(compose_db_events(&ft_events, outcome, block_header).await?);
            }
        }
    }
    coin::filter_zeros_and_enumerate_events(
        &mut res,
        shard_id,
        block_header.timestamp,
        &Event::Nep141,
    )?;

    Ok(res)
}

async fn compose_db_events(
    events: &Nep141Event,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
) -> anyhow::Result<Vec<CoinEvent>> {
    let mut ft_events = Vec::new();
    match &events.event_kind {
        event_types::Nep141EventKind::FtMint(mint_events) => {
            for mint_event in mint_events {
                // We filter such things later; I add this check here
                // only because sweatcoin produces too many such events and we want to ignore them in the early beginning
                if mint_event.amount == "0" {
                    continue;
                }
                let base = get_base(Event::Nep141, outcome, block_header)?;
                let custom = coin::FtEvent {
                    affected_id: AccountId::from_str(&mint_event.owner_id)?,
                    involved_id: None,
                    delta: BigDecimal::from_str(&mint_event.amount)?,
                    cause: "MINT".to_string(),
                    memo: mint_event
                        .memo
                        .as_ref()
                        .map(|s| s.escape_default().to_string()),
                };
                ft_events.push(coin::build_event(base, custom).await?);
            }
        }
        event_types::Nep141EventKind::FtTransfer(transfer_events) => {
            for transfer_event in transfer_events {
                let base = get_base(Event::Nep141, outcome, block_header)?;
                let custom = coin::FtEvent {
                    affected_id: AccountId::from_str(&transfer_event.old_owner_id)?,
                    involved_id: Some(AccountId::from_str(&transfer_event.new_owner_id)?),
                    delta: BigDecimal::from_str(&transfer_event.amount)?.mul(BigDecimal::from(-1)),
                    cause: "TRANSFER".to_string(),
                    memo: transfer_event
                        .memo
                        .as_ref()
                        .map(|s| s.escape_default().to_string()),
                };
                ft_events.push(coin::build_event(base, custom).await?);

                let base = get_base(Event::Nep141, outcome, block_header)?;
                let custom = coin::FtEvent {
                    affected_id: AccountId::from_str(&transfer_event.new_owner_id)?,
                    involved_id: Some(AccountId::from_str(&transfer_event.old_owner_id)?),
                    delta: BigDecimal::from_str(&transfer_event.amount)?,
                    cause: "TRANSFER".to_string(),
                    memo: transfer_event
                        .memo
                        .as_ref()
                        .map(|s| s.escape_default().to_string()),
                };
                ft_events.push(coin::build_event(base, custom).await?);
            }
        }
        event_types::Nep141EventKind::FtBurn(burn_events) => {
            for burn_event in burn_events {
                let base = get_base(Event::Nep141, outcome, block_header)?;
                let custom = coin::FtEvent {
                    affected_id: AccountId::from_str(&burn_event.owner_id)?,
                    involved_id: None,
                    delta: BigDecimal::from_str(&burn_event.amount)?.mul(BigDecimal::from(-1)),
                    cause: "BURN".to_string(),
                    memo: burn_event
                        .memo
                        .as_ref()
                        .map(|s| s.escape_default().to_string()),
                };
                ft_events.push(coin::build_event(base, custom).await?);
            }
        }
    }

    Ok(ft_events)
}
