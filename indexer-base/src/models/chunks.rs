use bigdecimal::BigDecimal;
use sqlx::Arguments;

use crate::models::FieldCount;

#[derive(Debug, sqlx::FromRow, FieldCount)]
pub struct Chunk {
    pub block_timestamp: BigDecimal,
    pub block_hash: String,
    pub chunk_hash: String,
    pub index_in_block: BigDecimal,
    pub signature: String,
    pub gas_limit: BigDecimal,
    pub gas_used: BigDecimal,
    pub author_account_id: String,
}

impl Chunk {
    pub fn from_chunk_view(
        chunk_view: &near_indexer_primitives::IndexerChunkView,
        block_hash: &near_indexer_primitives::CryptoHash,
        block_timestamp: u64,
    ) -> Self {
        Self {
            block_timestamp: block_timestamp.into(),
            block_hash: block_hash.to_string(),
            chunk_hash: chunk_view.header.chunk_hash.to_string(),
            index_in_block: chunk_view.header.shard_id.into(),
            signature: chunk_view.header.signature.to_string(),
            gas_limit: chunk_view.header.gas_limit.into(),
            gas_used: chunk_view.header.gas_used.into(),
            author_account_id: chunk_view.author.to_string(),
        }
    }
}

impl crate::models::SqlMethods for Chunk {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments) {
        args.add(&self.block_timestamp);
        args.add(&self.block_hash);
        args.add(&self.chunk_hash);
        args.add(&self.index_in_block);
        args.add(&self.signature);
        args.add(&self.gas_limit);
        args.add(&self.gas_used);
        args.add(&self.author_account_id);
    }

    fn insert_query(chunks_count: usize) -> anyhow::Result<String> {
        Ok("INSERT INTO chunks VALUES ".to_owned()
            + &crate::models::create_placeholders(chunks_count, Chunk::field_count())?
            + " ON CONFLICT DO NOTHING")
    }

    fn delete_query() -> String {
        "DELETE FROM chunks WHERE block_timestamp >= $1".to_string()
    }

    fn name() -> String {
        "chunks".to_string()
    }
}
