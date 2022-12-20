use crate::db_adapters::Event;
use crate::models;
use crate::models::coin_events::CoinEvent;
use bigdecimal::BigDecimal;
use futures::future::try_join_all;
use futures::try_join;
use near_lake_framework::near_indexer_primitives;
use near_primitives::types::AccountId;
use num_traits::Zero;

mod legacy;
mod nep141_events;

pub const FT: &str = "FT_NEP141";
pub const FT_LEGACY: &str = "FT_LEGACY";

struct FtEvent {
    pub affected_id: AccountId,
    pub involved_id: Option<AccountId>,
    pub delta: BigDecimal,
    pub cause: String,
    pub memo: Option<String>,
}

pub(crate) async fn store_ft(
    pool: &sqlx::Pool<sqlx::Postgres>,
    streamer_message: &near_indexer_primitives::StreamerMessage,
    chain_id: &str,
) -> anyhow::Result<()> {
    let mut events: Vec<CoinEvent> = vec![];

    let events_futures = streamer_message
        .shards
        .iter()
        .map(|shard| collect_ft_for_shard(streamer_message, shard, chain_id));
    for events_by_shard in try_join_all(events_futures).await? {
        events.extend(events_by_shard);
    }
    models::chunked_insert(pool, &events).await
}

pub(crate) fn filter_zeros_and_enumerate_events(
    ft_events: &mut Vec<crate::models::coin_events::CoinEvent>,
    shard_id: &near_indexer_primitives::types::ShardId,
    timestamp: u64,
    event_type: &Event,
) -> anyhow::Result<()> {
    ft_events.retain(|event| !event.delta_amount.is_zero());
    for (index, event) in ft_events.iter_mut().enumerate() {
        event.event_index =
            crate::db_adapters::compose_db_index(timestamp, shard_id, event_type, index)?;
    }
    Ok(())
}

async fn collect_ft_for_shard(
    streamer_message: &near_indexer_primitives::StreamerMessage,
    shard: &near_indexer_primitives::IndexerShard,
    chain_id: &str,
) -> anyhow::Result<Vec<CoinEvent>> {
    let mut events: Vec<CoinEvent> = vec![];

    let nep141_future = nep141_events::collect_nep141_events(
        &shard.shard_id,
        &shard.receipt_execution_outcomes,
        &streamer_message.block.header,
    );
    let legacy_contracts_future = legacy::collect_legacy(
        &shard.shard_id,
        &shard.receipt_execution_outcomes,
        &streamer_message.block.header,
        chain_id,
    );
    let (nep141_events, legacy_events) = try_join!(nep141_future, legacy_contracts_future)?;

    events.extend(nep141_events);
    events.extend(legacy_events);
    Ok(events)
}

async fn build_event(
    base: crate::db_adapters::EventBase,
    custom: FtEvent,
) -> anyhow::Result<CoinEvent> {
    Ok(CoinEvent {
        event_index: BigDecimal::zero(), // initialized later
        standard: base.standard,
        receipt_id: base.receipt_id,
        block_height: base.block_height,
        block_timestamp: base.block_timestamp,
        contract_account_id: base.contract_account_id.to_string(),
        affected_account_id: custom.affected_id.to_string(),
        involved_account_id: custom.involved_id.map(|id| id.to_string()),
        delta_amount: custom.delta,
        // coin_id: "".to_string(),
        cause: custom.cause,
        status: crate::db_adapters::get_status(&base.status),
        event_memo: custom.memo,
    })
}
