use bigdecimal::BigDecimal;
use sqlx::Arguments;

use crate::models::FieldCount;

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct NftEvent {
    pub event_index: BigDecimal,
    pub standard: String,
    pub receipt_id: String,
    pub block_height: BigDecimal,
    pub block_timestamp: BigDecimal,
    pub contract_account_id: String,
    pub token_id: String,
    pub cause: String,
    pub status: String,
    pub old_owner_account_id: Option<String>,
    pub new_owner_account_id: Option<String>,
    pub authorized_account_id: Option<String>,
    pub event_memo: Option<String>,
}

impl crate::models::SqlMethods for NftEvent {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.event_index);
        args.add(&self.standard);
        args.add(&self.receipt_id);
        args.add(&self.block_height);
        args.add(&self.block_timestamp);
        args.add(&self.contract_account_id);
        args.add(&self.token_id);
        args.add(&self.cause);
        args.add(&self.status);
        args.add(&self.old_owner_account_id);
        args.add(&self.new_owner_account_id);
        args.add(&self.authorized_account_id);
        args.add(&self.event_memo);
    }

    fn insert_query(items_count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO nft_events VALUES ".to_owned()
            + &crate::models::create_placeholders(items_count, NftEvent::field_count())?
            + " ON CONFLICT DO NOTHING")
    }

    fn name() -> String {
        "nft_events".to_string()
    }
}
