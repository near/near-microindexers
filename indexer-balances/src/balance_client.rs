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
