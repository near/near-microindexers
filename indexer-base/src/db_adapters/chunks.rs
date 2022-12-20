use crate::models;

pub(crate) async fn store_chunks(
    pool: &sqlx::Pool<sqlx::Postgres>,
    shards: &[near_indexer_primitives::IndexerShard],
    block_hash: &near_indexer_primitives::CryptoHash,
    block_timestamp: u64,
) -> anyhow::Result<()> {
    models::chunked_insert(
        pool,
        &shards
            .iter()
            .filter_map(|shard| {
                shard
                    .chunk
                    .as_ref()
                    .map(|chunk| models::Chunk::from_chunk_view(chunk, block_hash, block_timestamp))
            })
            .collect::<Vec<models::Chunk>>(),
    )
    .await?;

    Ok(())
}
