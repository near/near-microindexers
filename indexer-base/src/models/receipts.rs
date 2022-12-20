use std::str::FromStr;

use bigdecimal::BigDecimal;
use sqlx::Arguments;

use crate::models::{self, FieldCount};

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct DataReceipt {
    pub receipt_id: String,
    pub block_hash: String,
    pub chunk_hash: String,
    pub block_timestamp: BigDecimal,
    pub chunk_index_in_block: i32,
    pub receipt_index_in_chunk: i32,
    pub predecessor_account_id: String,
    pub receiver_account_id: String,
    pub originated_from_transaction_hash: String,
    pub data_id: String,
    pub data: Option<Vec<u8>>,
}

impl DataReceipt {
    pub fn try_from_data_receipt_view(
        receipt: &near_indexer_primitives::views::ReceiptView,
        block_hash: &near_indexer_primitives::CryptoHash,
        transaction_hash: &str,
        chunk_header: &near_indexer_primitives::views::ChunkHeaderView,
        index_in_chunk: i32,
        block_timestamp: u64,
    ) -> anyhow::Result<Self> {
        if let near_indexer_primitives::views::ReceiptEnumView::Data { data_id, data } =
            &receipt.receipt
        {
            Ok(Self {
                receipt_id: receipt.receipt_id.to_string(),
                block_hash: block_hash.to_string(),
                chunk_hash: chunk_header.chunk_hash.to_string(),
                block_timestamp: block_timestamp.into(),
                chunk_index_in_block: chunk_header.shard_id as i32,
                receipt_index_in_chunk: index_in_chunk,
                predecessor_account_id: receipt.predecessor_id.to_string(),
                receiver_account_id: receipt.receiver_id.to_string(),
                originated_from_transaction_hash: transaction_hash.to_string(),
                data_id: data_id.to_string(),
                data: data.clone(),
            })
        } else {
            Err(anyhow::anyhow!("Given ReceiptView is not of Data variant"))
        }
    }
}

impl crate::models::SqlMethods for DataReceipt {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.receipt_id);
        args.add(&self.block_hash);
        args.add(&self.chunk_hash);
        args.add(&self.block_timestamp);
        args.add(&self.chunk_index_in_block);
        args.add(&self.receipt_index_in_chunk);
        args.add(&self.predecessor_account_id);
        args.add(&self.receiver_account_id);
        args.add(&self.originated_from_transaction_hash);
        args.add(&self.data_id);
        args.add(&self.data);
    }

    fn insert_query(items_count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO data_receipts VALUES ".to_owned()
            + &crate::models::create_placeholders(items_count, DataReceipt::field_count())?
            + " ON CONFLICT DO NOTHING")
    }

    fn delete_query() -> String {
        "DELETE FROM data_receipts WHERE block_timestamp >= $1".to_string()
    }

    fn name() -> String {
        "data_receipts".to_string()
    }
}

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct ActionReceipt {
    pub receipt_id: String,
    pub block_hash: String,
    pub chunk_hash: String,
    pub block_timestamp: BigDecimal,
    pub chunk_index_in_block: i32,
    pub receipt_index_in_chunk: i32,
    pub predecessor_account_id: String,
    pub receiver_account_id: String,
    pub originated_from_transaction_hash: String,
    pub signer_account_id: String,
    pub signer_public_key: String,
    pub gas_price: BigDecimal,
}

impl ActionReceipt {
    pub fn try_from_action_receipt_view(
        receipt: &near_indexer_primitives::views::ReceiptView,
        block_hash: &near_indexer_primitives::CryptoHash,
        transaction_hash: &str,
        chunk_header: &near_indexer_primitives::views::ChunkHeaderView,
        index_in_chunk: i32,
        block_timestamp: u64,
    ) -> anyhow::Result<Self> {
        if let near_indexer_primitives::views::ReceiptEnumView::Action {
            signer_id,
            signer_public_key,
            gas_price,
            ..
        } = &receipt.receipt
        {
            Ok(Self {
                receipt_id: receipt.receipt_id.to_string(),
                block_hash: block_hash.to_string(),
                chunk_hash: chunk_header.chunk_hash.to_string(),
                block_timestamp: block_timestamp.into(),
                chunk_index_in_block: chunk_header.shard_id as i32,
                receipt_index_in_chunk: index_in_chunk,
                predecessor_account_id: receipt.predecessor_id.to_string(),
                receiver_account_id: receipt.receiver_id.to_string(),
                originated_from_transaction_hash: transaction_hash.to_string(),
                signer_account_id: signer_id.to_string(),
                signer_public_key: signer_public_key.to_string(),
                gas_price: BigDecimal::from_str(gas_price.to_string().as_str())
                    .expect("gas_price expected to be u128"),
            })
        } else {
            Err(anyhow::anyhow!(
                "Given ReceiptView is not of Action variant"
            ))
        }
    }
}

impl models::SqlMethods for ActionReceipt {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.receipt_id);
        args.add(&self.block_hash);
        args.add(&self.chunk_hash);
        args.add(&self.block_timestamp);
        args.add(&self.chunk_index_in_block);
        args.add(&self.receipt_index_in_chunk);
        args.add(&self.predecessor_account_id);
        args.add(&self.receiver_account_id);
        args.add(&self.originated_from_transaction_hash);
        args.add(&self.signer_account_id);
        args.add(&self.signer_public_key);
        args.add(&self.gas_price);
    }

    fn insert_query(items_count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO action_receipts VALUES ".to_owned()
            + &crate::models::create_placeholders(items_count, ActionReceipt::field_count())?
            + " ON CONFLICT DO NOTHING")
    }

    fn delete_query() -> String {
        "DELETE FROM action_receipts WHERE block_timestamp >= $1".to_string()
    }

    fn name() -> String {
        "action_receipts".to_string()
    }
}

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct ActionReceiptAction {
    pub block_hash: String,
    pub block_timestamp: BigDecimal,
    pub receipt_id: String,
    pub action_kind: String,
    pub args: serde_json::Value,
    pub predecessor_account_id: String,
    pub receiver_account_id: String,
    pub chunk_index_in_block: i32,
    pub index_in_chunk: i32,
}

impl ActionReceiptAction {
    pub fn from_action_view(
        receipt_id: String,
        action_view: &near_indexer_primitives::views::ActionView,
        predecessor_account_id: String,
        receiver_account_id: String,
        block_header: &near_indexer_primitives::views::BlockHeaderView,
        chunk_index_in_block: i32,
        index_in_chunk: i32,
    ) -> Self {
        let (action_kind, args) =
            models::serializers::extract_action_type_and_value_from_action_view(action_view);

        Self {
            block_hash: block_header.hash.to_string(),
            block_timestamp: block_header.timestamp.into(),
            receipt_id,
            action_kind,
            args,
            predecessor_account_id,
            receiver_account_id,
            chunk_index_in_block,
            index_in_chunk,
        }
    }
}

impl crate::models::SqlMethods for ActionReceiptAction {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.block_hash);
        args.add(&self.block_timestamp);
        args.add(&self.receipt_id);
        args.add(&self.action_kind);
        args.add(&self.args);
        args.add(&self.predecessor_account_id);
        args.add(&self.receiver_account_id);
        args.add(&self.chunk_index_in_block);
        args.add(&self.index_in_chunk);
    }

    fn insert_query(items_count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO action_receipts__actions VALUES ".to_owned()
            + &crate::models::create_placeholders(items_count, ActionReceiptAction::field_count())?
            + " ON CONFLICT DO NOTHING")
    }

    fn delete_query() -> String {
        "DELETE FROM action_receipts__actions WHERE block_timestamp >= $1".to_string()
    }

    fn name() -> String {
        "action_receipts__actions".to_string()
    }
}

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct ActionReceiptsOutput {
    pub block_hash: String,
    pub block_timestamp: BigDecimal,
    pub receipt_id: String,
    pub output_data_id: String,
    pub receiver_account_id: String,
    pub chunk_index_in_block: i32,
    pub index_in_chunk: i32,
}

impl ActionReceiptsOutput {
    pub fn from_data_receiver(
        receipt_id: String,
        data_receiver: &near_indexer_primitives::views::DataReceiverView,
        block_hash: &near_indexer_primitives::CryptoHash,
        block_timestamp: u64,
        chunk_index_in_block: i32,
        index_in_chunk: i32,
    ) -> Self {
        Self {
            block_hash: block_hash.to_string(),
            block_timestamp: block_timestamp.into(),
            receipt_id,
            output_data_id: data_receiver.data_id.to_string(),
            receiver_account_id: data_receiver.receiver_id.to_string(),
            chunk_index_in_block,
            index_in_chunk,
        }
    }
}

impl crate::models::SqlMethods for ActionReceiptsOutput {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.block_hash);
        args.add(&self.block_timestamp);
        args.add(&self.receipt_id);
        args.add(&self.output_data_id);
        args.add(&self.receiver_account_id);
        args.add(&self.chunk_index_in_block);
        args.add(&self.index_in_chunk);
    }

    fn insert_query(items_count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO action_receipts__outputs VALUES ".to_owned()
            + &crate::models::create_placeholders(
                items_count,
                ActionReceiptsOutput::field_count(),
            )?
            + " ON CONFLICT DO NOTHING")
    }

    fn delete_query() -> String {
        "DELETE FROM action_receipts__outputs WHERE block_timestamp >= $1".to_string()
    }

    fn name() -> String {
        "action_receipts__outputs".to_string()
    }
}
