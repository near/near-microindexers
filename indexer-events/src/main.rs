// TODO cleanup imports in all the files in the end
use futures::StreamExt;

use near_lake_framework::near_indexer_primitives;

use indexer_opts::{init_tracing, Opts, Parser};

mod db_adapters;
mod metrics;
mod models;

#[macro_use]
extern crate lazy_static;

pub(crate) const LOGGING_PREFIX: &str = "indexer_events";

const INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
const MAX_DELAY_TIME: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct AccountWithContract {
    pub account_id: near_primitives::types::AccountId,
    pub contract_account_id: near_primitives::types::AccountId,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts: Opts = Opts::parse();

    let pool = sqlx::PgPool::connect(&opts.database_url).await?;

    let _worker_guard = init_tracing(opts.debug)?;

    let config: near_lake_framework::LakeConfig = opts.to_lake_config(&pool).await?;

    indexer_opts::update_meta(
        &pool,
        indexer_opts::MetaAction::RegisterIndexer {
            indexer_id: opts.indexer_id.to_string(),
            start_block_height: opts.start_block_height,
        },
    )
    .await?;

    let (_lake_handle, stream) = near_lake_framework::streamer(config);

    tokio::spawn(async move {
        let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
            .map(|streamer_message| {
                handle_streamer_message(streamer_message, &pool, &opts.indexer_id, &opts.chain_id)
            })
            .buffer_unordered(1usize);

        let mut time_now = std::time::Instant::now();
        while let Some(handle_message) = handlers.next().await {
            match handle_message {
                Ok(block_height) => {
                    let elapsed = time_now.elapsed();
                    tracing::info!(
                        target: LOGGING_PREFIX,
                        "Elapsed time spent on block {}: {:.3?}",
                        block_height,
                        elapsed
                    );
                    time_now = std::time::Instant::now();
                }
                Err(e) => {
                    tracing::error!(target: LOGGING_PREFIX, "Stop indexing due to {}", e);
                    // we do not catch this error anywhere, this thread is just stopped with error,
                    // main thread continues serving metrics
                    anyhow::bail!(e)
                }
            }
        }
        Ok(()) // unreachable statement, loop above is endless
    });

    metrics::init_metrics_server(opts.port).await
}

async fn handle_streamer_message(
    streamer_message: near_indexer_primitives::StreamerMessage,
    pool: &sqlx::Pool<sqlx::Postgres>,
    indexer_id: &str,
    chain_id: &indexer_opts::ChainId,
) -> anyhow::Result<u64> {
    metrics::BLOCK_PROCESSED_TOTAL.inc();
    // Prometheus Gauge Metric type do not support u64
    // https://github.com/tikv/rust-prometheus/issues/470
    metrics::LATEST_BLOCK_HEIGHT.set(i64::try_from(streamer_message.block.header.height)?);

    if streamer_message.block.header.height % 100 == 0 {
        tracing::info!(
            target: crate::LOGGING_PREFIX,
            "{} / shards {}",
            streamer_message.block.header.height,
            streamer_message.shards.len()
        );
    }

    db_adapters::events::store_events(pool, &streamer_message, chain_id).await?;

    match indexer_opts::update_meta(
        &pool,
        indexer_opts::MetaAction::UpdateMeta {
            indexer_id: indexer_id.to_string(),
            last_processed_block_height: streamer_message.block.header.height,
        },
    )
    .await
    {
        Ok(_) => {}
        Err(err) => {
            tracing::warn!(
                target: crate::LOGGING_PREFIX,
                "Failed to update meta for INDEXER ID {}\n{:#?}",
                indexer_id,
                err,
            );
        }
    };
    Ok(streamer_message.block.header.height)
}
