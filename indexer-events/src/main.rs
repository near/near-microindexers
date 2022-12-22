// TODO cleanup imports in all the files in the end
use futures::StreamExt;
use indexer_opts::Parser;
use near_lake_framework::near_indexer_primitives;

mod configs;
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
    dotenv::dotenv().ok();
    let opts = indexer_opts::Opts::parse();
    let _worker_guard = configs::init_tracing(opts.debug)?;

    let pool = sqlx::PgPool::connect(&opts.database_url).await?;
    let lake_config = opts.to_lake_config(&pool).await?;
    let (_lake_handle, stream) = near_lake_framework::streamer(lake_config);
    let end_block_height = opts.end_block_height.unwrap_or(u64::MAX);

    tokio::spawn(metrics::init_server(opts.port).expect("Failed to start metrics server"));

    let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
        .map(|streamer_message| handle_streamer_message(streamer_message, &pool, &opts.chain_id))
        .buffer_unordered(1usize);

    while let Some(handle_message) = handlers.next().await {
        match handle_message {
            Ok(block_height) => {
                if block_height % 100 == 0 {
                    let _ = indexer_opts::update_meta(&pool, &opts.indexer_id, block_height).await;
                }
                if block_height > end_block_height {
                    let _ = indexer_opts::update_meta(&pool, &opts.indexer_id, block_height).await;
                    tracing::info!(
                        target: LOGGING_PREFIX,
                        "Congrats! Stop indexing because we reached end_block_height {}",
                        end_block_height
                    );
                    break;
                }
            }
            Err(e) => {
                tracing::error!(target: LOGGING_PREFIX, "Stop indexing due to {}", e);
                // we do not catch this error anywhere, this thread is just stopped with error,
                // main thread continues serving metrics
                anyhow::bail!(e)
            }
        }
    }
    Ok(())
}

async fn handle_streamer_message(
    streamer_message: near_indexer_primitives::StreamerMessage,
    pool: &sqlx::Pool<sqlx::Postgres>,
    chain_id: &indexer_opts::ChainId,
) -> anyhow::Result<u64> {
    metrics::BLOCK_PROCESSED_TOTAL.inc();
    // Prometheus Gauge Metric type do not support u64
    // https://github.com/tikv/rust-prometheus/issues/470
    metrics::LATEST_BLOCK_HEIGHT.set(i64::try_from(streamer_message.block.header.height)?);
    db_adapters::events::store_events(pool, &streamer_message, chain_id).await?;
    Ok(streamer_message.block.header.height)
}
