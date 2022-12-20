// // TODO cleanup imports in all the files in the end
use cached::SizedCache;
use clap::Parser;
use configs::{init_tracing, Opts};
use futures::StreamExt;
use near_lake_framework::near_indexer_primitives;
use tokio::sync::Mutex;

mod configs;
mod db_adapters;
mod metrics;
mod models;

#[macro_use]
extern crate lazy_static;

pub(crate) const LOGGING_PREFIX: &str = "indexer_balances";

const INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
const MAX_DELAY_TIME: std::time::Duration = std::time::Duration::from_secs(120);
const RETRY_COUNT: usize = 10;

#[derive(Debug, Default, Clone, Copy)]
pub struct BalanceDetails {
    pub non_staked: near_indexer_primitives::types::Balance,
    pub staked: near_indexer_primitives::types::Balance,
}

#[derive(Debug, Clone)]
pub struct AccountWithBalance {
    pub account_id: near_indexer_primitives::types::AccountId,
    pub balance: BalanceDetails,
}

pub type BalanceCache =
    std::sync::Arc<Mutex<SizedCache<near_indexer_primitives::types::AccountId, BalanceDetails>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let opts = Opts::parse();
    let _worker_guard = init_tracing(opts.debug)?;

    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    // TODO Error: while executing migrations: error returned from database: 1128 (HY000): Function 'near_indexer.GET_LOCK' is not defined
    // sqlx::migrate!().run(&pool).await?;

    let start_block_height = match opts.start_block_height {
        Some(x) => x,
        None => models::start_after_interruption(&pool).await?,
    };
    tracing::info!(
        target: LOGGING_PREFIX,
        "Indexer will start from block {}",
        start_block_height
    );

    // create a lake configuration with S3 information passed in as ENV vars
    let config = opts.to_lake_config(start_block_height).await;
    let (_lake_handle, stream) = near_lake_framework::streamer(config);

    // We want to prevent unnecessary RPC queries to find previous balance
    let balances_cache: BalanceCache =
        std::sync::Arc::new(Mutex::new(SizedCache::with_size(100_000)));

    let json_rpc_client = near_jsonrpc_client::JsonRpcClient::connect(&opts.near_archival_rpc_url);
    tokio::spawn(async move {
        let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
            .map(|streamer_message| {
                handle_streamer_message(streamer_message, &pool, &balances_cache, &json_rpc_client)
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
    balances_cache: &BalanceCache,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
) -> anyhow::Result<u64> {
    metrics::BLOCK_PROCESSED_TOTAL.inc();
    // Prometheus Gauge Metric type do not support u64
    // https://github.com/tikv/rust-prometheus/issues/470
    metrics::LATEST_BLOCK_HEIGHT.set(i64::try_from(streamer_message.block.header.height)?);

    db_adapters::balance_changes::store_balance_changes(
        pool,
        &streamer_message.shards,
        &streamer_message.block.header,
        balances_cache,
        json_rpc_client,
    )
    .await?;

    Ok(streamer_message.block.header.height)
}
