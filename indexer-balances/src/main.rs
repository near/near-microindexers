// // TODO cleanup imports in all the files in the end
use async_trait::async_trait;
use futures::StreamExt;
use indexer_opts::Parser;
use models::select_retry_or_panic;
use near_lake_framework::near_indexer_primitives;
use std::str::FromStr;

mod cache;
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

#[async_trait]
trait BalanceClient {
    async fn get_balance(
        &self,
        account_id: &near_indexer_primitives::types::AccountId,
        block_id: &near_primitives::types::BlockId,
    ) -> anyhow::Result<crate::BalanceDetails>;
}

struct JsonRpcBalanceClient {
    json_rpc_client: near_jsonrpc_client::JsonRpcClient,
}

impl JsonRpcBalanceClient {
    pub fn new(json_rpc_client: near_jsonrpc_client::JsonRpcClient) -> Self {
        Self { json_rpc_client }
    }
}

#[async_trait::async_trait]
impl BalanceClient for JsonRpcBalanceClient {
    async fn get_balance(
        &self,
        account_id: &near_indexer_primitives::types::AccountId,
        block_id: &near_primitives::types::BlockId,
    ) -> anyhow::Result<crate::BalanceDetails> {
        let query = near_jsonrpc_client::methods::query::RpcQueryRequest {
            block_reference: near_primitives::types::BlockReference::BlockId(block_id.clone()),
            request: near_primitives::views::QueryRequest::ViewAccount {
                account_id: account_id.clone(),
            },
        };

        let account_response = self.json_rpc_client.call(query).await;

        if let Err(err) = account_response {
            return match err.handler_error() {
                Some(near_jsonrpc_primitives::types::query::RpcQueryError::UnknownAccount {
                    ..
                }) => Ok(crate::BalanceDetails {
                    non_staked: 0,
                    staked: 0,
                }),
                _ => Err(err.into()),
            };
        }

        let response_kind = account_response.unwrap().kind;

        match response_kind {
            near_jsonrpc_primitives::types::query::QueryResponseKind::ViewAccount(account) => {
                Ok(crate::BalanceDetails {
                    non_staked: account.amount,
                    staked: account.locked,
                })
            }
            _ => unreachable!(
                "Unreachable code! Asked for ViewAccount (block_id {:?}, account_id {})\nReceived\n\
                {:#?}\nReport this to https://github.com/near/near-jsonrpc-client-rs",
                block_id,
                account_id.to_string(),
                response_kind
            ),
        }
    }
}

struct PgBalanceClient {
    pool: sqlx::Pool<sqlx::Postgres>,
}

impl PgBalanceClient {
    pub fn new(pool: sqlx::Pool<sqlx::Postgres>) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl BalanceClient for PgBalanceClient {
    async fn get_balance(
        &self,
        account_id: &near_indexer_primitives::types::AccountId,
        block_id: &near_primitives::types::BlockId,
    ) -> anyhow::Result<crate::BalanceDetails> {
        if let near_primitives::types::BlockId::Hash(_) = block_id {
            anyhow::bail!("Can not query by hash")
        }

        let near_primitives::types::BlockId::Height(block_height) = block_id else { unreachable!() };

        let balance_event = match select_retry_or_panic(&self.pool, block_height, account_id, 5)
            .await
        {
            Ok(Some(balance_event)) => BalanceDetails {
                non_staked: u128::from_str(&balance_event.absolute_nonstaked_amount.to_string())?,
                staked: u128::from_str(&balance_event.absolute_staked_amount.to_string())?,
            },
            // TODO can we trust that DB will have all required values or do we need to check RPC
            // as well?
            Ok(None) => BalanceDetails {
                non_staked: 0,
                staked: 0,
            },
            Err(e) => anyhow::bail!(e),
        };

        Ok(balance_event)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let opts = indexer_opts::Opts::parse();
    configs::init_tracing(opts.debug)?;

    let pool = sqlx::PgPool::connect(&opts.database_url).await?;
    let balance_client = PgBalanceClient::new(pool.clone());
    let lake_config = opts.to_lake_config(&pool).await?;
    let (sender, stream) = near_lake_framework::streamer(lake_config);
    let end_block_height = opts.end_block_height.unwrap_or(u64::MAX);

    tokio::spawn(metrics::init_server(opts.port).expect("Failed to start metrics server"));

    let balances_cache = cache::BalanceCache::new(100_000);

    let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
        .map(|streamer_message| {
            handle_streamer_message(streamer_message, &pool, &balances_cache, &balance_client)
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
    balances_cache: &cache::BalanceCache,
    balance_client: &impl BalanceClient,
) -> anyhow::Result<u64> {
    tracing::info!(
        target: LOGGING_PREFIX,
        "Processing block: {}",
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
        balance_client,
    )
    .await?;

    Ok(streamer_message.block.header.height)
}
