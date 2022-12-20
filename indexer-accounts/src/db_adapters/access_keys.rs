use std::convert::TryFrom;

use futures::future::try_join_all;
use futures::try_join;

use crate::models;

pub(crate) async fn store_access_keys(
    pool: &sqlx::Pool<sqlx::Postgres>,
    shards: &[near_indexer_primitives::IndexerShard],
    block_height: near_indexer_primitives::types::BlockHeight,
) -> anyhow::Result<()> {
    let futures = shards.iter().map(|shard| {
        store_access_keys_for_chunk(pool, &shard.receipt_execution_outcomes, block_height)
    });

    try_join_all(futures).await.map(|_| ())
}

async fn store_access_keys_for_chunk(
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

    let mut created_access_keys: Vec<models::access_keys::AccessKey> = vec![];
    let mut deleted_access_keys: Vec<models::access_keys::AccessKey> = vec![];
    let mut access_keys_from_deleted_accounts: Vec<models::access_keys::AccessKey> = vec![];

    for receipt in successful_receipts {
        if let near_indexer_primitives::views::ReceiptEnumView::Action { actions, .. } =
            &receipt.receipt
        {
            for action in actions {
                match action {
                    near_indexer_primitives::views::ActionView::DeleteAccount { .. } => {
                        access_keys_from_deleted_accounts.push(
                            models::access_keys::AccessKey::access_key_to_delete(
                                "".to_string(),
                                &receipt.receiver_id,
                                &receipt.receipt_id,
                                block_height,
                            ),
                        );
                    }
                    near_indexer_primitives::views::ActionView::AddKey {
                        public_key,
                        access_key,
                    } => {
                        created_access_keys.push(models::access_keys::AccessKey::from_action_view(
                            public_key,
                            &receipt.receiver_id,
                            access_key,
                            &receipt.receipt_id,
                            block_height,
                        ));
                    }
                    near_indexer_primitives::views::ActionView::DeleteKey { public_key } => {
                        deleted_access_keys.push(
                            models::access_keys::AccessKey::access_key_to_delete(
                                public_key.to_string(),
                                &receipt.receiver_id,
                                &receipt.receipt_id,
                                block_height,
                            ),
                        );
                    }
                    near_indexer_primitives::views::ActionView::Transfer { .. } => {
                        if receipt.receiver_id.len() == 64usize {
                            // we can just insert it, the duplicates will be ignored by the db
                            if let Ok(public_key_bytes) = hex::decode(receipt.receiver_id.as_ref())
                            {
                                if let Ok(public_key) =
                                    near_crypto::ED25519PublicKey::try_from(&public_key_bytes[..])
                                {
                                    created_access_keys.push(
                                        models::access_keys::AccessKey::from_action_view(
                                            &near_crypto::PublicKey::from(public_key.clone()),
                                            &receipt.receiver_id,
                                            &near_indexer_primitives::views::AccessKeyView {
                                                nonce: 0,
                                                permission: near_indexer_primitives::views::AccessKeyPermissionView::FullAccess
                                            },
                                            &receipt.receipt_id,
                                            block_height
                                        ),
                                    );
                                }
                            }
                        }
                    }
                    _ => continue,
                }
            }
        }
    }

    let update_access_keys_for_deleted_accounts_future = async {
        let query = r"UPDATE access_keys
                            SET deleted_by_receipt_id = $4, deleted_by_block_height = $6
                            WHERE account_id = $2
                                AND created_by_block_height < $6
                                AND deleted_by_block_height IS NULL";
        models::update_retry_or_panic(pool, query, &access_keys_from_deleted_accounts, 10).await
    };

    let update_access_keys_future = async {
        let query = r"UPDATE access_keys
                            SET deleted_by_receipt_id = $4, deleted_by_block_height = $6
                            WHERE account_id = $2 AND public_key = $1
                                AND created_by_block_height < $6
                                AND deleted_by_block_height IS NULL";
        models::update_retry_or_panic(pool, query, &deleted_access_keys, 10).await
    };

    let add_access_keys_future =
        async { models::chunked_insert(pool, &created_access_keys, 10).await };

    try_join!(
        update_access_keys_for_deleted_accounts_future,
        update_access_keys_future,
        add_access_keys_future
    )?;

    Ok(())
}
