use crate::db_adapters::event_types;
use crate::db_adapters::{coin, nft};
use futures::try_join;
use near_lake_framework::near_indexer_primitives;

pub(crate) async fn store_events(
    pool: &sqlx::Pool<sqlx::Postgres>,
    streamer_message: &near_indexer_primitives::StreamerMessage,
    chain_id: &indexer_opts::ChainId,
) -> anyhow::Result<()> {
    try_join!(
        coin::store_ft(pool, streamer_message, chain_id),
        nft::store_nft(pool, streamer_message),
    )?;
    Ok(())
}

pub(crate) fn extract_events(
    outcome: &near_indexer_primitives::IndexerExecutionOutcomeWithReceipt,
) -> Vec<event_types::NearEvent> {
    let prefix = "EVENT_JSON:";
    outcome.execution_outcome.outcome.logs.iter().filter_map(|untrimmed_log| {
        let log = untrimmed_log.trim();
        if !log.starts_with(prefix) {
            return None;
        }

        match serde_json::from_str::<'_, event_types::NearEvent>(
            log[prefix.len()..].trim(),
        ) {
            Ok(result) => Some(result),
            Err(err) => {
                tracing::info!(
                    target: crate::LOGGING_PREFIX,
                    "Provided event log does not correspond to any of formats defined in NEP. Will ignore this event. \n {:#?} \n{:#?}",
                    err,
                    untrimmed_log,
                );
                None
            }
        }
    }).collect()
}
