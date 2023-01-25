use crate::models::fungible_token_events::FungibleTokenEvent;
use futures::try_join;
use near_lake_framework::near_indexer_primitives;

mod aurora;
mod rainbow_bridge;
mod skyward;
mod tkn_near;
mod wentokensir;
mod wrap_near;

pub(crate) async fn collect_legacy(
    shard_id: &near_indexer_primitives::types::ShardId,
    receipt_execution_outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    chain_id: &indexer_opts::ChainId,
) -> anyhow::Result<Vec<FungibleTokenEvent>> {
    // We don't need to store legacy events for testnet
    if chain_id != &indexer_opts::ChainId::Mainnet {
        return Ok(vec![]);
    }
    let mut events: Vec<FungibleTokenEvent> = vec![];

    let aurora_future = aurora::collect_aurora(shard_id, receipt_execution_outcomes, block_header);
    let rainbow_bridge_future =
        rainbow_bridge::collect_rainbow_bridge(shard_id, receipt_execution_outcomes, block_header);
    let skyward_future =
        skyward::collect_skyward(shard_id, receipt_execution_outcomes, block_header);
    let tkn_near_future =
        tkn_near::collect_tkn_near(shard_id, receipt_execution_outcomes, block_header);
    let wentokensir_future =
        wentokensir::collect_wentokensir(shard_id, receipt_execution_outcomes, block_header);
    let wrap_near_future =
        wrap_near::collect_wrap_near(shard_id, receipt_execution_outcomes, block_header);

    let (
        aurora_events,
        rainbow_bridge_events,
        skyward_events,
        tkn_near_events,
        wentokensir_events,
        wrap_near_events,
    ) = try_join!(
        aurora_future,
        rainbow_bridge_future,
        skyward_future,
        tkn_near_future,
        wentokensir_future,
        wrap_near_future
    )?;

    events.extend(aurora_events);
    events.extend(rainbow_bridge_events);
    events.extend(skyward_events);
    events.extend(tkn_near_events);
    events.extend(wentokensir_events);
    events.extend(wrap_near_events);
    Ok(events)
}
