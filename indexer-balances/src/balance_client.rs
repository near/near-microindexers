use std::str::FromStr;

use crate::models::balance_changes::NearBalanceEvent;
use crate::models::SqlxMethods;
use async_trait::async_trait;
use near_lake_framework::near_indexer_primitives;

#[async_trait]
pub trait BalanceClient {
    async fn get_balance_before_block(
        &self,
        account_id: &near_indexer_primitives::types::AccountId,
        block_header: &near_indexer_primitives::views::BlockHeaderView,
    ) -> anyhow::Result<crate::BalanceDetails>;
}

pub struct JsonRpcBalanceClient {
    json_rpc_client: near_jsonrpc_client::JsonRpcClient,
}

impl JsonRpcBalanceClient {
    pub fn new(json_rpc_client: near_jsonrpc_client::JsonRpcClient) -> Self {
        Self { json_rpc_client }
    }
}

#[async_trait]
impl BalanceClient for JsonRpcBalanceClient {
    async fn get_balance_before_block(
        &self,
        account_id: &near_indexer_primitives::types::AccountId,
        block_header: &near_indexer_primitives::views::BlockHeaderView,
    ) -> anyhow::Result<crate::BalanceDetails> {
        let query = near_jsonrpc_client::methods::query::RpcQueryRequest {
            block_reference: near_primitives::types::BlockReference::BlockId(
                near_primitives::types::BlockId::Hash(block_header.prev_hash),
            ),
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

        let response_kind = account_response?.kind;

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
                block_header.prev_hash,
                account_id.to_string(),
                response_kind
            ),
        }
    }
}

pub struct PgBalanceClient {
    pool: sqlx::Pool<sqlx::Postgres>,
}

impl PgBalanceClient {
    pub fn new(pool: sqlx::Pool<sqlx::Postgres>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BalanceClient for PgBalanceClient {
    async fn get_balance_before_block(
        &self,
        account_id: &near_indexer_primitives::types::AccountId,
        block_header: &near_indexer_primitives::views::BlockHeaderView,
    ) -> anyhow::Result<crate::BalanceDetails> {
        let account_balance = match crate::models::select_one_retry_or_panic(
            &self.pool,
            &NearBalanceEvent::select_prev_balance_query(block_header.height, account_id),
            crate::RETRY_COUNT,
        )
        .await
        {
            Ok(Some(balance_event)) => crate::BalanceDetails {
                non_staked: u128::from_str(&balance_event.absolute_nonstaked_amount.to_string())?,
                staked: u128::from_str(&balance_event.absolute_staked_amount.to_string())?,
            },
            Ok(None) => crate::BalanceDetails {
                non_staked: 0,
                staked: 0,
            },
            Err(e) => anyhow::bail!(e),
        };

        Ok(account_balance)
    }
}
