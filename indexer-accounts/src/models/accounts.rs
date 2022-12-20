use bigdecimal::BigDecimal;
use sqlx::Arguments;

use crate::models::FieldCount;

#[derive(Debug, Clone, sqlx::FromRow, FieldCount)]
pub struct Account {
    pub account_id: String,
    pub created_by_receipt_id: Option<String>,
    pub deleted_by_receipt_id: Option<String>,
    pub created_by_block_height: BigDecimal,
    pub deleted_by_block_height: Option<BigDecimal>,
}

impl Account {
    pub fn new_from_receipt(
        account_id: &near_indexer_primitives::types::AccountId,
        created_by_receipt_id: &near_indexer_primitives::CryptoHash,
        created_by_block_height: near_indexer_primitives::types::BlockHeight,
    ) -> Self {
        Self {
            account_id: account_id.to_string(),
            created_by_receipt_id: Some(created_by_receipt_id.to_string()),
            deleted_by_receipt_id: None,
            created_by_block_height: BigDecimal::from(created_by_block_height),
            deleted_by_block_height: None,
        }
    }
}

impl crate::models::MySqlMethods for Account {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.account_id);
        args.add(&self.created_by_receipt_id);
        args.add(&self.deleted_by_receipt_id);
        args.add(&self.created_by_block_height);
        args.add(&self.deleted_by_block_height);
    }

    fn insert_query(items_count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO accounts VALUES ".to_owned()
            + &crate::models::create_placeholders(items_count, Account::field_count())?
            + " ON CONFLICT DO NOTHING")
    }

    fn name() -> String {
        "accounts".to_string()
    }
}
