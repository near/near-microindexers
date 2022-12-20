use cached::Cached;
use futures::future::try_join_all;

use crate::models;

pub(crate) async fn store_execution_outcomes(
    pool: &sqlx::Pool<sqlx::Postgres>,
    shards: &[near_indexer_primitives::IndexerShard],
    block_hash: &near_indexer_primitives::CryptoHash,
    block_timestamp: u64,
    receipts_cache: crate::ReceiptsCache,
) -> anyhow::Result<()> {
    let futures = shards.iter().map(|shard| {
        store_execution_outcomes_for_chunk(
            pool,
            &shard.receipt_execution_outcomes,
            shard.shard_id,
            block_hash,
            block_timestamp,
            receipts_cache.clone(),
        )
    });

    try_join_all(futures).await.map(|_| ())
}

/// Saves ExecutionOutcome to database and then saves ExecutionOutcomesReceipts
pub async fn store_execution_outcomes_for_chunk(
    pool: &sqlx::Pool<sqlx::Postgres>,
    execution_outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    shard_id: near_indexer_primitives::types::ShardId,
    block_hash: &near_indexer_primitives::CryptoHash,
    block_timestamp: u64,
    receipts_cache: crate::ReceiptsCache,
) -> anyhow::Result<()> {
    models::chunked_insert(
        pool,
        &execution_outcomes
            .iter()
            .enumerate()
            .map(|(index_in_chunk, outcome)| {
                models::ExecutionOutcome::from_execution_outcome(
                    &outcome.execution_outcome,
                    index_in_chunk as i32,
                    block_timestamp,
                    shard_id,
                )
            })
            .collect::<Vec<models::ExecutionOutcome>>(),
    )
    .await?;

    let mut outcome_receipt_models: Vec<models::ExecutionOutcomeReceipt> = vec![];
    let mut receipts_cache_lock = receipts_cache.lock().await;
    for outcome in execution_outcomes {
        // Trying to take the parent Transaction hash for the Receipt from ReceiptsCache
        // remove it from cache once found as it is not expected to observe the Receipt for
        // second time
        let parent_transaction_hash = receipts_cache_lock.cache_remove(
            &crate::ReceiptOrDataId::ReceiptId(outcome.execution_outcome.id),
        );

        outcome_receipt_models.extend(outcome.execution_outcome.outcome.receipt_ids.iter().map(
            |receipt_id| {
                // if we have `parent_transaction_hash` from cache, then we put all "produced" Receipt IDs
                // as key and `parent_transaction_hash` as value, so the Receipts from one of the next blocks
                // could find their parents in cache
                if let Some(transaction_hash) = &parent_transaction_hash {
                    receipts_cache_lock.cache_set(
                        crate::ReceiptOrDataId::ReceiptId(*receipt_id),
                        transaction_hash.clone(),
                    );
                }

                models::ExecutionOutcomeReceipt {
                    block_hash: block_hash.to_string(),
                    block_timestamp: block_timestamp.into(),
                    executed_receipt_id: outcome.execution_outcome.id.to_string(),
                    produced_receipt_id: receipt_id.to_string(),
                    chunk_index_in_block: shard_id as i32,
                    // we fill it later because we need flatmap result
                    index_in_chunk: 0,
                }
            },
        ));
    }
    drop(receipts_cache_lock);

    outcome_receipt_models
        .iter_mut()
        .enumerate()
        .for_each(|(i, execution_outcomes_receipt)| {
            execution_outcomes_receipt.index_in_chunk = i as i32;
        });

    models::chunked_insert(pool, &outcome_receipt_models).await?;

    Ok(())
}
