// // TODO cleanup imports in all the files in the end
use futures::StreamExt;
use indexer_opts::Parser;
use near_lake_framework::near_indexer_primitives;
use std::collections::HashMap;
use tokio::sync::RwLock;

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
    std::sync::Arc<RwLock<HashMap<near_indexer_primitives::types::AccountId, BalanceDetails>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let opts = indexer_opts::Opts::parse();
    configs::init_tracing(opts.debug)?;

    let rpc_url = opts
        .rpc_url
        .as_ref()
        .expect("RPC_URL is required to run indexer-balances");
    let json_rpc_client = near_jsonrpc_client::JsonRpcClient::connect(rpc_url);

    let pool = sqlx::PgPool::connect(&opts.database_url).await?;
    let lake_config = opts.to_lake_config(&pool).await?;
    let (sender, stream) = near_lake_framework::streamer(lake_config);
    let end_block_height = opts.end_block_height.unwrap_or(u64::MAX);

    tokio::spawn(metrics::init_server(opts.port).expect("Failed to start metrics server"));

    // We want to prevent unnecessary RPC queries to find previous balance
    let balances_cache: BalanceCache = std::sync::Arc::new(RwLock::new(HashMap::new()));

    let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
        .map(|streamer_message| {
            handle_streamer_message(streamer_message, &pool, &balances_cache, &json_rpc_client)
        })
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
    drop(handlers); // close the channel so the sender will stop
    match sender.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(anyhow::Error::from(e)),
    }
}

async fn handle_streamer_message(
    streamer_message: near_indexer_primitives::StreamerMessage,
    pool: &sqlx::Pool<sqlx::Postgres>,
    balances_cache: &BalanceCache,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
) -> anyhow::Result<u64> {
    tracing::info!(
        target: LOGGING_PREFIX,
        "Processing block {}",
        streamer_message.block.header.height
    );

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
