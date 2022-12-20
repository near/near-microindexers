use std::str::FromStr;

use bigdecimal::BigDecimal;
use sqlx::Arguments;

use crate::models::{FieldCount, PrintEnum};

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct AccountChange {
    pub account_id: String,
    pub block_timestamp: BigDecimal,
    pub block_hash: String,
    pub caused_by_transaction_hash: Option<String>,
    pub caused_by_receipt_id: Option<String>,
    pub update_reason: String,
    pub nonstaked_balance: BigDecimal,
    pub staked_balance: BigDecimal,
    pub storage_usage: BigDecimal,
    pub chunk_index_in_block: i32,
    pub index_in_chunk: i32,
}

impl AccountChange {
    pub fn from_state_change_with_cause(
        state_change_with_cause: &near_indexer_primitives::views::StateChangeWithCauseView,
        changed_in_block_hash: &near_indexer_primitives::CryptoHash,
        changed_in_block_timestamp: u64,
        chunk_index_in_block: i32,
        index_in_chunk: i32,
    ) -> Option<Self> {
        let near_indexer_primitives::views::StateChangeWithCauseView { cause, value } =
            state_change_with_cause;

        let (account_id, account): (String, Option<&near_indexer_primitives::views::AccountView>) =
            match value {
                near_indexer_primitives::views::StateChangeValueView::AccountUpdate {
                    account_id,
                    account,
                } => (account_id.to_string(), Some(account)),
                near_indexer_primitives::views::StateChangeValueView::AccountDeletion {
                    account_id,
                } => (account_id.to_string(), None),
                _ => return None,
            };

        Some(Self {
            account_id,
            block_timestamp: changed_in_block_timestamp.into(),
            block_hash: changed_in_block_hash.to_string(),
            caused_by_transaction_hash: if let near_indexer_primitives::views::StateChangeCauseView::TransactionProcessing { tx_hash } = cause {
                Some(tx_hash.to_string())
            } else {
                None
            },
            caused_by_receipt_id: match cause {
                near_indexer_primitives::views::StateChangeCauseView::ActionReceiptProcessingStarted { receipt_hash } => Some(receipt_hash.to_string()),
                near_indexer_primitives::views::StateChangeCauseView::ActionReceiptGasReward { receipt_hash } => Some(receipt_hash.to_string()),
                near_indexer_primitives::views::StateChangeCauseView::ReceiptProcessing { receipt_hash } => Some(receipt_hash.to_string()),
                near_indexer_primitives::views::StateChangeCauseView::PostponedReceipt { receipt_hash } => Some(receipt_hash.to_string()),
                _ => None,
            },
            update_reason: cause.print().to_string(),
            nonstaked_balance: if let Some(acc) = account {
                BigDecimal::from_str(acc.amount.to_string().as_str())
                    .expect("`amount` expected to be u128")
            } else {
                BigDecimal::from(0)
            },
            staked_balance: if let Some(acc) = account {
                BigDecimal::from_str(acc.locked.to_string().as_str())
                    .expect("`locked` expected to be u128")
            } else {
                BigDecimal::from(0)
            },
            storage_usage: if let Some(acc) = account {
                acc.storage_usage.into()
            } else {
                BigDecimal::from(0)
            },
            chunk_index_in_block,
            index_in_chunk
        })
    }
}

impl crate::models::SqlMethods for AccountChange {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.account_id);
        args.add(&self.block_timestamp);
        args.add(&self.block_hash);
        args.add(&self.caused_by_transaction_hash);
        args.add(&self.caused_by_receipt_id);
        args.add(&self.update_reason);
        args.add(&self.nonstaked_balance);
        args.add(&self.staked_balance);
        args.add(&self.storage_usage);
        args.add(&self.chunk_index_in_block);
        args.add(&self.index_in_chunk);
    }

    fn insert_query(account_changes_count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO account_changes VALUES ".to_owned()
            + &crate::models::create_placeholders(
                account_changes_count,
                AccountChange::field_count(),
            )?
            + " ON CONFLICT DO NOTHING")
    }

    fn delete_query() -> String {
        "DELETE FROM account_changes WHERE block_timestamp >= $1".to_string()
    }

    fn name() -> String {
        "account_changes".to_string()
    }
}
