use std::str::FromStr;

use bigdecimal::BigDecimal;
use sqlx::Arguments;

use crate::models::{FieldCount, PrintEnum};

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct ExecutionOutcome {
    pub receipt_id: String,
    pub block_hash: String,
    pub block_timestamp: BigDecimal,
    pub chunk_index_in_block: i32,
    pub index_in_chunk: i32,
    pub gas_burnt: BigDecimal,
    pub tokens_burnt: BigDecimal,
    pub executor_account_id: String,
    pub status: String,
}

impl ExecutionOutcome {
    pub fn from_execution_outcome(
        execution_outcome: &near_indexer_primitives::views::ExecutionOutcomeWithIdView,
        index_in_chunk: i32,
        executed_in_block_timestamp: u64,
        shard_id: u64,
    ) -> Self {
        Self {
            receipt_id: execution_outcome.id.to_string(),
            block_hash: execution_outcome.block_hash.to_string(),
            block_timestamp: executed_in_block_timestamp.into(),
            chunk_index_in_block: shard_id as i32,
            index_in_chunk,
            gas_burnt: execution_outcome.outcome.gas_burnt.into(),
            tokens_burnt: BigDecimal::from_str(
                execution_outcome.outcome.tokens_burnt.to_string().as_str(),
            )
            .expect("`tokens_burnt` expected to be u128"),
            executor_account_id: execution_outcome.outcome.executor_id.to_string(),
            status: execution_outcome.outcome.status.print().to_string(),
        }
    }
}

impl crate::models::SqlMethods for ExecutionOutcome {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.receipt_id);
        args.add(&self.block_hash);
        args.add(&self.block_timestamp);
        args.add(&self.chunk_index_in_block);
        args.add(&self.index_in_chunk);
        args.add(&self.gas_burnt);
        args.add(&self.tokens_burnt);
        args.add(&self.executor_account_id);
        args.add(&self.status);
    }

    fn insert_query(execution_outcome_count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO execution_outcomes VALUES ".to_owned()
            + &crate::models::create_placeholders(
                execution_outcome_count,
                ExecutionOutcome::field_count(),
            )?
            + " ON CONFLICT DO NOTHING")
    }

    fn delete_query() -> String {
        "DELETE FROM execution_outcomes WHERE block_timestamp >= $1".to_string()
    }

    fn name() -> String {
        "execution_outcomes".to_string()
    }
}

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct ExecutionOutcomeReceipt {
    pub block_hash: String,
    pub block_timestamp: BigDecimal,
    pub executed_receipt_id: String,
    pub produced_receipt_id: String,
    pub chunk_index_in_block: i32,
    pub index_in_chunk: i32,
}

impl crate::models::SqlMethods for ExecutionOutcomeReceipt {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.block_hash);
        args.add(&self.block_timestamp);
        args.add(&self.executed_receipt_id);
        args.add(&self.produced_receipt_id);
        args.add(&self.chunk_index_in_block);
        args.add(&self.index_in_chunk);
    }

    fn insert_query(execution_outcome_receipt_count: usize) -> anyhow::Result<String> {
        Ok(
            "INSERT INTO execution_outcomes__receipts VALUES ".to_owned()
                + &crate::models::create_placeholders(
                    execution_outcome_receipt_count,
                    ExecutionOutcomeReceipt::field_count(),
                )?
                + " ON CONFLICT DO NOTHING",
        )
    }

    fn delete_query() -> String {
        "DELETE FROM execution_outcomes__receipts WHERE block_timestamp >= $1".to_string()
    }

    fn name() -> String {
        "execution_outcomes__receipts".to_string()
    }
}
