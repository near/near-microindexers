use cached::SizedCache;
use clap::Parser;
use dotenv::dotenv;
use futures::{try_join, StreamExt};
use std::env;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

use crate::configs::Opts;

mod configs;
mod db_adapters;
mod models;

// Categories for logging
// TODO naming
pub(crate) const INDEXER: &str = "indexer";

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
    dotenv().ok();

    let opts: Opts = Opts::parse();
    let pool = sqlx::PgPool::connect(&env::var("DATABASE_URL")?).await?;
    let config = near_lake_framework::LakeConfig {
        s3_config: None,
        s3_bucket_name: opts.s3_bucket_name.clone(),
        s3_region_name: opts.s3_region_name.clone(),
        start_block_height: opts.start_block_height.unwrap(),
    };
    init_tracing();

    let stream = near_lake_framework::streamer(config);

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
                !opts.non_strict_mode,
            )
        })
        .buffer_unordered(1usize);

    // let mut time_now = std::time::Instant::now();
    while let Some(handle_message) = handlers.next().await {
        match handle_message {
            Ok(_block_height) => {
                // let elapsed = time_now.elapsed();
                // println!(
                //     "Elapsed time spent on block {}: {:.3?}",
                //     _block_height, elapsed
                // );
                // time_now = std::time::Instant::now();
            }
            Err(e) => {
                return Err(anyhow::anyhow!(e));
            }
        }
    }

    Ok(())
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

fn init_tracing() {
    let mut env_filter = EnvFilter::new("near_lake_framework=info");

    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        if !rust_log.is_empty() {
            for directive in rust_log.split(',').filter_map(|s| match s.parse() {
                Ok(directive) => Some(directive),
                Err(err) => {
                    eprintln!("Ignoring directive `{}`: {}", s, err);
                    None
                }
            }) {
                env_filter = env_filter.add_directive(directive);
            }
        }
    }

    tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .init();
}
