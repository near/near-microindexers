use futures::future::try_join_all;

use crate::models;

pub(crate) async fn store_account_changes(
    pool: &sqlx::Pool<sqlx::Postgres>,
    shards: &[near_indexer_primitives::IndexerShard],
    block_hash: &near_indexer_primitives::CryptoHash,
    block_timestamp: u64,
) -> anyhow::Result<()> {
    let futures = shards.iter().map(|shard| {
        store_account_changes_for_chunk(
            pool,
            &shard.state_changes,
            block_hash,
            block_timestamp,
            shard.shard_id,
        )
    });

    try_join_all(futures).await.map(|_| ())
}

async fn store_account_changes_for_chunk(
    pool: &sqlx::Pool<sqlx::Postgres>,
    state_changes: &near_indexer_primitives::views::StateChangesView,
    block_hash: &near_indexer_primitives::CryptoHash,
    block_timestamp: u64,
    shard_id: near_indexer_primitives::types::ShardId,
) -> anyhow::Result<()> {
    models::chunked_insert(
        pool,
        &state_changes
            .iter()
            .filter_map(|state_change| {
                models::AccountChange::from_state_change_with_cause(
                    state_change,
                    block_hash,
                    block_timestamp,
                    shard_id as i32,
                    // we fill it later because we can't enumerate before filtering finishes
                    0,
                )
            })
            .enumerate()
            .map(|(i, mut account_change)| {
                account_change.index_in_chunk = i as i32;
                account_change
            })
            .collect::<Vec<models::AccountChange>>(),
    )
    .await?;

    Ok(())
}
