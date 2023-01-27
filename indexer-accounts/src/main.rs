// TODO cleanup imports in all the files in the end
use futures::{try_join, StreamExt};
use indexer_opts::Parser;
use near_lake_framework::near_indexer_primitives;

mod configs;
mod db_adapters;
mod models;

pub(crate) const LOGGING_PREFIX: &str = "indexer_accounts";

const INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
const MAX_DELAY_TIME: std::time::Duration = std::time::Duration::from_secs(120);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let opts = indexer_opts::Opts::parse();
    let _worker_guard = configs::init_tracing(opts.debug)?;

    let pool = sqlx::PgPool::connect(&opts.database_url).await?;
    let lake_config = opts.to_lake_config(&pool).await?;
    let (sender, stream) = near_lake_framework::streamer(lake_config);
    let end_block_height = opts.end_block_height.unwrap_or(u64::MAX);

    let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
        .map(|streamer_message| handle_streamer_message(streamer_message, &pool))
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
) -> anyhow::Result<u64> {
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
