use bigdecimal::BigDecimal;
use sqlx::Arguments;

use crate::models::FieldCount;

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct NearBalanceEvent {
    pub event_index: BigDecimal,
    pub block_timestamp: BigDecimal,
    pub block_height: BigDecimal,
    pub receipt_id: Option<String>,
    pub transaction_hash: Option<String>,
    pub affected_account_id: String,
    pub involved_account_id: Option<String>,
    pub direction: String,
    pub cause: String,
    pub status: String,
    pub delta_nonstaked_amount: BigDecimal,
    pub absolute_nonstaked_amount: BigDecimal,
    pub delta_staked_amount: BigDecimal,
    pub absolute_staked_amount: BigDecimal,
}

impl crate::models::SqlxMethods for NearBalanceEvent {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.event_index);
        args.add(&self.block_timestamp);
        args.add(&self.block_height);
        args.add(&self.receipt_id);
        args.add(&self.transaction_hash);
        args.add(&self.affected_account_id);
        args.add(&self.involved_account_id);
        args.add(&self.direction);
        args.add(&self.cause);
        args.add(&self.status);
        args.add(&self.delta_nonstaked_amount);
        args.add(&self.absolute_nonstaked_amount);
        args.add(&self.delta_staked_amount);
        args.add(&self.absolute_staked_amount);
    }

    fn insert_query(count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO near_balance_events VALUES ".to_owned()
            + &crate::models::create_placeholders(count, NearBalanceEvent::field_count())?
            + " ON CONFLICT DO NOTHING")
    }

    fn name() -> String {
        "near_balance_events".to_string()
    }
}
