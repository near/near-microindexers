use crate::models;
use near_lake_framework::near_indexer_primitives;

pub(crate) async fn store_block(
    pool: &sqlx::Pool<sqlx::Postgres>,
    block: &near_indexer_primitives::views::BlockView,
) -> anyhow::Result<()> {
    models::chunked_insert(pool, &vec![models::Block::from_block_view(block)]).await?;
    Ok(())
}
