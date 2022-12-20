use bigdecimal::BigDecimal;
use sqlx::Arguments;

use crate::models::FieldCount;

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct CoinEvent {
    pub event_index: BigDecimal,
    pub standard: String,
    pub receipt_id: String,
    pub block_height: BigDecimal,
    pub block_timestamp: BigDecimal,
    pub contract_account_id: String,
    pub affected_account_id: String,
    pub involved_account_id: Option<String>,
    pub delta_amount: BigDecimal,
    pub cause: String,
    pub status: String,
    pub event_memo: Option<String>,
}

impl crate::models::SqlMethods for CoinEvent {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.event_index);
        args.add(&self.standard);
        args.add(&self.receipt_id);
        args.add(&self.block_height);
        args.add(&self.block_timestamp);
        args.add(&self.contract_account_id);
        args.add(&self.affected_account_id);
        args.add(&self.involved_account_id);
        args.add(&self.delta_amount);
        args.add(&self.cause);
        args.add(&self.status);
        args.add(&self.event_memo);
    }

    fn insert_query(items_count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO coin_events VALUES ".to_owned()
            + &crate::models::create_placeholders(items_count, CoinEvent::field_count())?
            + " ON CONFLICT DO NOTHING")
    }

    fn name() -> String {
        "coin_events".to_string()
    }
}
