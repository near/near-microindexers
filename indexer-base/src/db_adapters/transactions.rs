use cached::Cached;
use futures::future::try_join_all;

use crate::models;

pub(crate) async fn store_transactions(
    pool: &sqlx::Pool<sqlx::Postgres>,
    shards: &[near_indexer_primitives::IndexerShard],
    block_hash: &near_indexer_primitives::CryptoHash,
    block_timestamp: u64,
    receipts_cache: crate::ReceiptsCache,
) -> anyhow::Result<()> {
    let tx_futures = shards
        .iter()
        .filter_map(|shard| shard.chunk.as_ref())
        .filter(|chunk| !chunk.transactions.is_empty())
        .map(|chunk| {
            store_chunk_transactions(
                pool,
                &chunk.transactions,
                block_hash,
                block_timestamp,
                &chunk.header,
                std::sync::Arc::clone(&receipts_cache),
            )
        });

    try_join_all(tx_futures).await?;
    Ok(())
}

async fn store_chunk_transactions(
    pool: &sqlx::Pool<sqlx::Postgres>,
    transactions: &[near_indexer_primitives::IndexerTransactionWithOutcome],
    block_hash: &near_indexer_primitives::CryptoHash,
    block_timestamp: u64,
    chunk_view: &near_indexer_primitives::views::ChunkHeaderView,
    receipts_cache: crate::ReceiptsCache,
) -> anyhow::Result<()> {
    let mut receipts_cache_lock = receipts_cache.lock().await;
    let transaction_models = transactions
        .iter()
        .enumerate()
        .map(|(i, transaction)| {
            let converted_into_receipt_id = transaction
                .outcome
                .execution_outcome
                .outcome
                .receipt_ids
                .first()
                .expect("`receipt_ids` must contain one Receipt Id");

            // Save this Transaction hash to ReceiptsCache
            // we use the Receipt ID to which this transaction was converted
            // and the Transaction hash as a value.
            // Later, while Receipt will be looking for a parent Transaction hash
            // it will be able to find it in the ReceiptsCache
            receipts_cache_lock.cache_set(
                crate::ReceiptOrDataId::ReceiptId(*converted_into_receipt_id),
                transaction.transaction.hash.to_string(),
            );

            models::Transaction::from_indexer_transaction(
                transaction,
                &transaction.transaction.hash.to_string(),
                &converted_into_receipt_id.to_string(),
                block_hash,
                block_timestamp,
                chunk_view,
                i as i32,
            )
        })
        .collect::<Vec<models::Transaction>>();
    drop(receipts_cache_lock);

    models::chunked_insert(pool, &transaction_models).await?;

    Ok(())
}
