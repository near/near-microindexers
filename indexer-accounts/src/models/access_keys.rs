use bigdecimal::BigDecimal;
use sqlx::Arguments;

use crate::models::{FieldCount, PrintEnum};

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct AccessKey {
    pub public_key: String,
    pub account_id: String,
    pub created_by_receipt_id: Option<String>,
    pub deleted_by_receipt_id: Option<String>,
    pub created_by_block_height: BigDecimal,
    pub deleted_by_block_height: Option<BigDecimal>,
    pub permission_kind: String,
}

impl AccessKey {
    pub fn access_key_to_delete(
        public_key: String,
        account_id: &near_indexer_primitives::types::AccountId,
        deleted_by_receipt_id: &near_indexer_primitives::CryptoHash,
        deleted_by_block_height: near_indexer_primitives::types::BlockHeight,
    ) -> Self {
        Self {
            public_key,
            account_id: account_id.to_string(),
            created_by_receipt_id: None,
            deleted_by_receipt_id: Some(deleted_by_receipt_id.to_string()),
            created_by_block_height: Default::default(),
            deleted_by_block_height: Some(BigDecimal::from(deleted_by_block_height)),
            permission_kind: "".to_string(),
        }
    }

    pub fn from_action_view(
        public_key: &near_crypto::PublicKey,
        account_id: &near_indexer_primitives::types::AccountId,
        access_key: &near_indexer_primitives::views::AccessKeyView,
        created_by_receipt_id: &near_indexer_primitives::CryptoHash,
        created_by_block_height: near_indexer_primitives::types::BlockHeight,
    ) -> Self {
        Self {
            public_key: public_key.to_string(),
            account_id: account_id.to_string(),
            created_by_receipt_id: Some(created_by_receipt_id.to_string()),
            deleted_by_receipt_id: None,
            created_by_block_height: BigDecimal::from(created_by_block_height),
            deleted_by_block_height: None,
            permission_kind: access_key.permission.print().to_string(),
        }
    }
}

impl crate::models::MySqlMethods for AccessKey {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.public_key);
        args.add(&self.account_id);
        args.add(&self.created_by_receipt_id);
        args.add(&self.deleted_by_receipt_id);
        args.add(&self.created_by_block_height);
        args.add(&self.deleted_by_block_height);
        args.add(&self.permission_kind);
    }

    fn insert_query(items_count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO access_keys VALUES ".to_owned()
            + &crate::models::create_placeholders(items_count, AccessKey::field_count())?
            + " ON CONFLICT DO NOTHING")
    }

    fn name() -> String {
        "access_keys".to_string()
    }
}
