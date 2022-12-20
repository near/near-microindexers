use crate::db_adapters::coin::{FT, FT_LEGACY};
use crate::db_adapters::nft::NFT;
use bigdecimal::BigDecimal;
use near_lake_framework::near_indexer_primitives;
use near_lake_framework::near_indexer_primitives::views::ExecutionStatusView;
use std::str::FromStr;

mod coin;
mod event_types;
pub(crate) mod events;
mod nft;
mod numeric_types;

pub(crate) const CHUNK_SIZE_FOR_BATCH_INSERT: usize = 100;
pub(crate) const RETRY_COUNT: usize = 10;

pub(crate) enum Event {
    Nep141,
    Nep171,
    Aurora,
    RainbowBridge,
    Skyward,
    TknNear,
    Wentokensir,
    WrapNear,
}

pub(crate) struct EventBase {
    pub standard: String,
    pub receipt_id: String,
    pub block_height: BigDecimal,
    pub block_timestamp: BigDecimal,
    pub contract_account_id: near_primitives::types::AccountId,
    pub status: ExecutionStatusView,
}

pub(crate) fn get_base(
    event_type: Event,
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
) -> anyhow::Result<EventBase> {
    Ok(EventBase {
        standard: get_standard(&event_type),
        receipt_id: outcome.receipt.receipt_id.to_string(),
        block_height: BigDecimal::from(block_header.height),
        block_timestamp: BigDecimal::from(block_header.timestamp),
        contract_account_id: outcome.execution_outcome.outcome.executor_id.clone(),
        status: outcome.execution_outcome.outcome.status.clone(),
    })
}

fn get_standard(event_type: &Event) -> String {
    match event_type {
        Event::Nep141 => FT,
        Event::Nep171 => NFT,
        Event::Aurora => FT_LEGACY,
        Event::RainbowBridge => FT_LEGACY,
        Event::Skyward => FT_LEGACY,
        Event::TknNear => FT_LEGACY,
        Event::Wentokensir => FT_LEGACY,
        Event::WrapNear => FT_LEGACY,
    }
    .to_string()
}

fn get_status(status: &ExecutionStatusView) -> String {
    match status {
        ExecutionStatusView::Unknown => {
            tracing::warn!(
                target: crate::LOGGING_PREFIX,
                "Unknown execution status view",
            );
            "UNKNOWN"
        }
        ExecutionStatusView::Failure(_) => "FAILURE",
        ExecutionStatusView::SuccessValue(_) => "SUCCESS",
        ExecutionStatusView::SuccessReceiptId(_) => "SUCCESS",
    }
    .to_string()
}

fn compose_db_index(
    block_timestamp: u64,
    shard_id: &near_primitives::types::ShardId,
    event: &Event,
    event_index: usize,
) -> anyhow::Result<BigDecimal> {
    let event_type_index: u128 = match event {
        Event::Nep141 => 1,
        Event::Nep171 => 2,
        Event::Aurora => 3,
        Event::RainbowBridge => 4,
        Event::Skyward => 5,
        Event::TknNear => 6,
        Event::Wentokensir => 7,
        Event::WrapNear => 8,
    };
    let db_index: u128 = (block_timestamp as u128) * 100_000_000 * 100_000_000
        + (*shard_id as u128) * 1_000_000_000
        + event_type_index * 1_000_000
        + (event_index as u128);
    Ok(BigDecimal::from_str(&db_index.to_string())?)
}
