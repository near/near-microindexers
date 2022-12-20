// TODO cleanup imports in all the files in the end
use clap::Parser;
use dotenv::dotenv;
use futures::{try_join, StreamExt};
use std::env;
use tracing_subscriber::EnvFilter;

use near_lake_framework::near_indexer_primitives;

use crate::configs::Opts;

mod configs;
mod db_adapters;
mod models;

// Categories for logging
// TODO naming
pub(crate) const INDEXER: &str = "indexer";

const INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
const MAX_DELAY_TIME: std::time::Duration = std::time::Duration::from_secs(120);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    let opts: Opts = Opts::parse();
    let pool = sqlx::PgPool::connect(&env::var("DATABASE_URL")?).await?;

    // let start_block_height = match opts.start_block_height {
    //     Some(x) => x,
    //     None => models::start_after_interruption(&pool).await?,
    // };
    let config = opts.to_lake_config(opts.start_block_height).await;
    init_tracing();

    let (_lake_handle, stream) = near_lake_framework::streamer(config);

    let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
        .map(|streamer_message| handle_streamer_message(streamer_message, &pool))
        .buffer_unordered(1usize);

    let mut time_now = std::time::Instant::now();
    while let Some(handle_message) = handlers.next().await {
        match handle_message {
            Ok(block_height) => {
                let elapsed = time_now.elapsed();
                println!(
                    "Elapsed time spent on block {}: {:.3?}",
                    block_height, elapsed
                );
                time_now = std::time::Instant::now();
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
) -> anyhow::Result<u64> {
    if streamer_message.block.header.height % 100 == 0 {
        eprintln!(
            "{} / shards {}",
            streamer_message.block.header.height,
            streamer_message.shards.len()
        );
    }

    let accounts_future = db_adapters::accounts::store_accounts(
        pool,
        &streamer_message.shards,
        streamer_message.block.header.height,
    );

    let access_keys_future = db_adapters::access_keys::store_access_keys(
        pool,
        &streamer_message.shards,
        streamer_message.block.header.height,
    );

    try_join!(accounts_future, access_keys_future)?;
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
