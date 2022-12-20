use crate::models;
use bigdecimal::BigDecimal;
use futures::future::try_join_all;
use futures::try_join;

pub(crate) async fn store_accounts(
    pool: &sqlx::Pool<sqlx::Postgres>,
    shards: &[near_indexer_primitives::IndexerShard],
    block_height: near_indexer_primitives::types::BlockHeight,
) -> anyhow::Result<()> {
    let futures = shards.iter().map(|shard| {
        store_accounts_for_chunk(pool, &shard.receipt_execution_outcomes, block_height)
    });

    try_join_all(futures).await.map(|_| ())
}

async fn store_accounts_for_chunk(
    pool: &sqlx::Pool<sqlx::Postgres>,
    outcomes: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    block_height: near_indexer_primitives::types::BlockHeight,
) -> anyhow::Result<()> {
    if outcomes.is_empty() {
        return Ok(());
    }
    let successful_receipts = outcomes
        .iter()
        .filter(|outcome_with_receipt| {
            matches!(
                outcome_with_receipt.execution_outcome.outcome.status,
                near_indexer_primitives::views::ExecutionStatusView::SuccessValue(_)
                    | near_indexer_primitives::views::ExecutionStatusView::SuccessReceiptId(_)
            )
        })
        .map(|outcome_with_receipt| &outcome_with_receipt.receipt);

    let mut accounts_to_create: Vec<models::accounts::Account> = vec![];
    let mut accounts_to_update: Vec<models::accounts::Account> = vec![];

    for receipt in successful_receipts {
        if let near_indexer_primitives::views::ReceiptEnumView::Action { actions, .. } =
            &receipt.receipt
        {
            for action in actions {
                match action {
                    near_indexer_primitives::views::ActionView::CreateAccount => {
                        accounts_to_create.push(models::accounts::Account::new_from_receipt(
                            &receipt.receiver_id,
                            &receipt.receipt_id,
                            block_height,
                        ));
                    }
                    near_indexer_primitives::views::ActionView::Transfer { .. } => {
                        if receipt.receiver_id.len() == 64usize {
                            let query = r"SELECT * FROM accounts
                                                WHERE account_id = $1
                                                    AND created_by_block_height < $2::numeric(20, 0)
                                                    AND (deleted_by_block_height IS NULL OR deleted_by_block_height > $2::numeric(20, 0))";
                            let previously_created = models::select_retry_or_panic(
                                pool,
                                query,
                                &[receipt.receiver_id.to_string(), block_height.to_string()],
                                10,
                            )
                            .await?;
                            if previously_created.is_empty() {
                                accounts_to_create.push(
                                    models::accounts::Account::new_from_receipt(
                                        &receipt.receiver_id,
                                        &receipt.receipt_id,
                                        block_height,
                                    ),
                                );
                            }
                        }
                    }
                    near_indexer_primitives::views::ActionView::DeleteAccount { .. } => {
                        accounts_to_update.push(models::accounts::Account {
                            account_id: receipt.receiver_id.to_string(),
                            created_by_receipt_id: None,
                            deleted_by_receipt_id: Some(receipt.receipt_id.to_string()),
                            created_by_block_height: Default::default(),
                            deleted_by_block_height: Some(BigDecimal::from(block_height)),
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    let create_accounts_future =
        async { models::chunked_insert(pool, &accounts_to_create, 10).await };

    let update_accounts_future = async {
        let query = r"UPDATE accounts
                            SET deleted_by_receipt_id = $3, deleted_by_block_height = $5
                            WHERE account_id = $1
                                AND created_by_block_height < $5
                                AND deleted_by_block_height IS NULL";
        models::update_retry_or_panic(pool, query, &accounts_to_update, 10).await
    };

    try_join!(create_accounts_future, update_accounts_future)?;
    Ok(())
}
