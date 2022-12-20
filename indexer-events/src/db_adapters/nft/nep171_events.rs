use bigdecimal::BigDecimal;

use crate::db_adapters::event_types::Nep171Event;
use crate::db_adapters::Event;
use crate::db_adapters::{events, get_status, nft};
use crate::models::nft_events::NftEvent;
use near_lake_framework::near_indexer_primitives;
use num_traits::Zero;

use crate::db_adapters::event_types;
use crate::db_adapters::nft::NFT;

pub(crate) async fn collect_nep171_events(
    shard_id: &near_indexer_primitives::types::ShardId,
    receipt_execution_outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
) -> anyhow::Result<Vec<NftEvent>> {
    let mut res = Vec::new();
    for outcome in receipt_execution_outcomes {
        for event in events::extract_events(outcome) {
            if let event_types::NearEvent::Nep171(nft_events) = event {
                compose_nft_db_events(&nft_events, outcome, block_header)?;
            }
        }
    }

    nft::enumerate_events(&mut res, shard_id, block_header.timestamp, &Event::Nep171)?;
    Ok(res)
}

fn compose_nft_db_events(
    events: &Nep171Event,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
) -> anyhow::Result<Vec<NftEvent>> {
    let mut nft_events = vec![];
    let contract_id = &outcome.receipt.receiver_id;
    match &events.event_kind {
        event_types::Nep171EventKind::NftMint(mint_events) => {
            for mint_event in mint_events {
                for token_id in &mint_event.token_ids {
                    nft_events.push(NftEvent {
                        event_index: BigDecimal::zero(), // initialized later
                        standard: NFT.to_string(),
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_height: BigDecimal::from(block_header.height),
                        block_timestamp: BigDecimal::from(block_header.timestamp),
                        contract_account_id: contract_id.to_string(),
                        token_id: token_id.escape_default().to_string(),
                        cause: "MINT".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        old_owner_account_id: None,
                        new_owner_account_id: Some(
                            mint_event.owner_id.escape_default().to_string(),
                        ),
                        authorized_account_id: None,
                        event_memo: mint_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                }
            }
        }
        event_types::Nep171EventKind::NftTransfer(transfer_events) => {
            for transfer_event in transfer_events {
                for token_id in &transfer_event.token_ids {
                    nft_events.push(NftEvent {
                        event_index: BigDecimal::zero(), // initialized later
                        standard: NFT.to_string(),
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_height: BigDecimal::from(block_header.height),
                        block_timestamp: BigDecimal::from(block_header.timestamp),
                        contract_account_id: contract_id.to_string(),
                        token_id: token_id.escape_default().to_string(),
                        cause: "TRANSFER".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        old_owner_account_id: Some(
                            transfer_event.old_owner_id.escape_default().to_string(),
                        ),
                        new_owner_account_id: Some(
                            transfer_event.new_owner_id.escape_default().to_string(),
                        ),
                        authorized_account_id: transfer_event
                            .authorized_id
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                        event_memo: transfer_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                }
            }
        }
        event_types::Nep171EventKind::NftBurn(burn_events) => {
            for burn_event in burn_events {
                for token_id in &burn_event.token_ids {
                    nft_events.push(NftEvent {
                        event_index: BigDecimal::zero(), // initialized later
                        standard: NFT.to_string(),
                        receipt_id: outcome.receipt.receipt_id.to_string(),
                        block_height: BigDecimal::from(block_header.height),
                        block_timestamp: BigDecimal::from(block_header.timestamp),
                        contract_account_id: contract_id.to_string(),
                        token_id: token_id.escape_default().to_string(),
                        cause: "BURN".to_string(),
                        status: get_status(&outcome.execution_outcome.outcome.status),
                        old_owner_account_id: Some(
                            burn_event.owner_id.escape_default().to_string(),
                        ),
                        new_owner_account_id: None,
                        authorized_account_id: burn_event
                            .authorized_id
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                        event_memo: burn_event
                            .memo
                            .as_ref()
                            .map(|s| s.escape_default().to_string()),
                    });
                }
            }
        }
    }
    Ok(nft_events)
}
