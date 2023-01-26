use cached::SizedCache;
use futures::{try_join, StreamExt};
use tokio::sync::Mutex;

use indexer_opts::Parser;
use near_lake_framework::near_indexer_primitives;

mod configs;
mod db_adapters;
mod models;

pub(crate) const LOGGING_PREFIX: &str = "indexer_base";

const INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
const MAX_DELAY_TIME: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub enum ReceiptOrDataId {
    ReceiptId(near_indexer_primitives::CryptoHash),
    DataId(near_indexer_primitives::CryptoHash),
}
// Creating type aliases to make HashMap types for cache more explicit
pub type ParentTransactionHashString = String;
// Introducing a simple cache for Receipts to find their parent Transactions without
// touching the database
// The key is ReceiptID
// The value is TransactionHash (the very parent of the Receipt)
pub type ReceiptsCache =
    std::sync::Arc<Mutex<SizedCache<ReceiptOrDataId, ParentTransactionHashString>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let opts = indexer_opts::Opts::parse();
    let _worker_guard = configs::init_tracing(opts.debug)?;

    let pool = sqlx::PgPool::connect(&opts.database_url).await?;
    let lake_config = opts.to_lake_config(&pool).await?;
    let (sender, stream) = near_lake_framework::streamer(lake_config);
    let end_block_height = opts.end_block_height.unwrap_or(u64::MAX);

    // We want to prevent unnecessary SELECT queries to the database to find
    // the Transaction hash for the Receipt.
    // Later we need to find the Receipt which is a parent to underlying Receipts.
    // Receipt ID will of the child will be stored as key and parent Transaction hash/Receipt ID
    // will be stored as a value
    let receipts_cache: ReceiptsCache =
        std::sync::Arc::new(Mutex::new(SizedCache::with_size(100_000)));

    let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
        .map(|streamer_message| {
            handle_streamer_message(
                streamer_message,
                &pool,
                receipts_cache.clone(),
                true, // !opts.non_strict_mode, // TODO support one more flag
            )
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
    match sender.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(anyhow::Error::from(e)),
    }
}

async fn handle_streamer_message(
    streamer_message: near_indexer_primitives::StreamerMessage,
    pool: &sqlx::Pool<sqlx::Postgres>,
    receipts_cache: ReceiptsCache,
    strict_mode: bool,
) -> anyhow::Result<u64> {
    if streamer_message.block.header.height % 100 == 0 {
        eprintln!(
            "{} / shards {}",
            streamer_message.block.header.height,
            streamer_message.shards.len()
        );
    }

    let blocks_future = db_adapters::blocks::store_block(pool, &streamer_message.block);

    let chunks_future = db_adapters::chunks::store_chunks(
        pool,
        &streamer_message.shards,
        &streamer_message.block.header.hash,
        streamer_message.block.header.timestamp,
    );

    let transactions_future = db_adapters::transactions::store_transactions(
        pool,
        &streamer_message.shards,
        &streamer_message.block.header.hash,
        streamer_message.block.header.timestamp,
        receipts_cache.clone(),
    );

    let receipts_future = db_adapters::receipts::store_receipts(
        pool,
        strict_mode,
        &streamer_message.shards,
        &streamer_message.block.header,
        receipts_cache.clone(),
    );

    let execution_outcomes_future = db_adapters::execution_outcomes::store_execution_outcomes(
        pool,
        &streamer_message.shards,
        &streamer_message.block.header.hash,
        streamer_message.block.header.timestamp,
        receipts_cache.clone(),
    );

    let account_changes_future = db_adapters::account_changes::store_account_changes(
        pool,
        &streamer_message.shards,
        &streamer_message.block.header.hash,
        streamer_message.block.header.timestamp,
    );

    blocks_future.await?;
    // FK to block_hash
    chunks_future.await?;
    // we have FK both to blocks and chunks
    transactions_future.await?;
    // this guy can contain local receipts, so we have to do that after transactions_future finished the work
    receipts_future.await?;
    try_join!(
        // this guy depends on transactions and receipts with its FKs
        account_changes_future,
        // this guy thinks that receipts_future finished, and clears the cache
        execution_outcomes_future
    )?;
    Ok(streamer_message.block.header.height)
}
