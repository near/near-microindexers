use std::collections::HashMap;
use std::str::FromStr;

use bigdecimal::BigDecimal;
use cached::Cached;
use futures::future::try_join_all;
use futures::try_join;
use itertools::{Either, Itertools};
use sqlx::Arguments;
use sqlx::Row;

use crate::models;

/// Saves receipts to database
pub(crate) async fn store_receipts(
    pool: &sqlx::Pool<sqlx::Postgres>,
    strict_mode: bool,
    shards: &[near_indexer_primitives::IndexerShard],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    receipts_cache: crate::ReceiptsCache,
) -> anyhow::Result<()> {
    let futures = shards
        .iter()
        .filter_map(|shard| shard.chunk.as_ref())
        .filter(|chunk| !chunk.receipts.is_empty())
        .map(|chunk| {
            store_chunk_receipts(
                pool,
                strict_mode,
                &chunk.receipts,
                block_header,
                &chunk.header,
                std::sync::Arc::clone(&receipts_cache),
            )
        });

    try_join_all(futures).await.map(|_| ())
}

async fn store_chunk_receipts(
    pool: &sqlx::Pool<sqlx::Postgres>,
    strict_mode: bool,
    receipts: &[near_indexer_primitives::views::ReceiptView],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    chunk_header: &near_indexer_primitives::views::ChunkHeaderView,
    receipts_cache: crate::ReceiptsCache,
) -> anyhow::Result<()> {
    let tx_hashes_for_receipts: HashMap<
        crate::ReceiptOrDataId,
        crate::ParentTransactionHashString,
    > = find_tx_hashes_for_receipts(
        pool,
        strict_mode,
        receipts.to_vec(),
        block_header.height,
        receipts_cache.clone(),
    )
    .await?;

    // At the moment we can observe output data in the Receipt it's impossible to know
    // the Receipt Id of that Data Receipt. That's why we insert the pair DataId<>ParentTransactionHash
    // to ReceiptsCache
    let mut receipts_cache_lock = receipts_cache.lock().await;
    for receipt in receipts {
        if let near_indexer_primitives::views::ReceiptEnumView::Action {
            output_data_receivers,
            ..
        } = &receipt.receipt
        {
            if !output_data_receivers.is_empty() {
                if let Some(transaction_hash) = tx_hashes_for_receipts
                    .get(&crate::ReceiptOrDataId::ReceiptId(receipt.receipt_id))
                {
                    for data_receiver in output_data_receivers {
                        receipts_cache_lock.cache_set(
                            crate::ReceiptOrDataId::DataId(data_receiver.data_id),
                            transaction_hash.clone(),
                        );
                    }
                }
            }
        }
    }
    // releasing the lock
    drop(receipts_cache_lock);

    // enumeration goes through all the receipts
    let enumerated_receipts_with_parent_tx: Vec<(
        usize,
        &String,
        &near_indexer_primitives::views::ReceiptView,
    )> = receipts
        .iter()
        .enumerate()
        .filter_map(|(index, receipt)| match receipt.receipt {
            near_indexer_primitives::views::ReceiptEnumView::Action { .. } => {
                tx_hashes_for_receipts
                    .get(&crate::ReceiptOrDataId::ReceiptId(receipt.receipt_id))
                    .map(|tx| (index, tx, receipt))
            }
            near_indexer_primitives::views::ReceiptEnumView::Data { data_id, .. } => {
                tx_hashes_for_receipts
                    .get(&crate::ReceiptOrDataId::DataId(data_id))
                    .map(|tx| (index, tx, receipt))
            }
        })
        .collect();
    if strict_mode && receipts.len() != enumerated_receipts_with_parent_tx.len() {
        // todo maybe it's better to collect blocks for rerun here
        return Err(anyhow::anyhow!(
            "Some tx hashes were not found at block {}",
            block_header.height
        ));
    }

    let (action_receipts, data_receipts) = enumerated_receipts_with_parent_tx.iter().partition_map(
        |(index, tx, receipt)| match receipt.receipt {
            near_indexer_primitives::views::ReceiptEnumView::Action { .. } => {
                Either::Left((*index, *tx, *receipt))
            }
            near_indexer_primitives::views::ReceiptEnumView::Data { .. } => {
                Either::Right((*index, *tx, *receipt))
            }
        },
    );

    let process_receipt_actions_future =
        store_receipt_actions(pool, action_receipts, block_header, chunk_header);

    let process_receipt_data_future =
        store_data_receipts(pool, data_receipts, block_header, chunk_header);

    try_join!(process_receipt_actions_future, process_receipt_data_future)?;
    Ok(())
}

/// Looks for already created parent transaction hash for given receipts
async fn find_tx_hashes_for_receipts(
    pool: &sqlx::Pool<sqlx::Postgres>,
    strict_mode: bool,
    mut receipts: Vec<near_indexer_primitives::views::ReceiptView>,
    block_height: u64,
    receipts_cache: crate::ReceiptsCache,
) -> anyhow::Result<HashMap<crate::ReceiptOrDataId, crate::ParentTransactionHashString>> {
    let mut tx_hashes_for_receipts: HashMap<
        crate::ReceiptOrDataId,
        crate::ParentTransactionHashString,
    > = HashMap::new();

    let mut receipts_cache_lock = receipts_cache.lock().await;
    // add receipt-transaction pairs from the cache to the response
    tx_hashes_for_receipts.extend(receipts.iter().filter_map(|receipt| {
        match receipt.receipt {
            near_indexer_primitives::views::ReceiptEnumView::Action { .. } => receipts_cache_lock
                .cache_get(&crate::ReceiptOrDataId::ReceiptId(receipt.receipt_id))
                .map(|parent_transaction_hash| {
                    (
                        crate::ReceiptOrDataId::ReceiptId(receipt.receipt_id),
                        parent_transaction_hash.clone(),
                    )
                }),
            near_indexer_primitives::views::ReceiptEnumView::Data { data_id, .. } => {
                // Pair DataId:ParentTransactionHash won't be used after this moment
                // We want to clean it up to prevent our cache from growing
                receipts_cache_lock
                    .cache_remove(&crate::ReceiptOrDataId::DataId(data_id))
                    .map(|parent_transaction_hash| {
                        (
                            crate::ReceiptOrDataId::DataId(data_id),
                            parent_transaction_hash,
                        )
                    })
            }
        }
    }));
    // releasing the lock
    drop(receipts_cache_lock);

    // discard the Receipts already in cache from the attempts to search
    receipts.retain(|r| match r.receipt {
        near_indexer_primitives::views::ReceiptEnumView::Data { data_id, .. } => {
            !tx_hashes_for_receipts.contains_key(&crate::ReceiptOrDataId::DataId(data_id))
        }
        near_indexer_primitives::views::ReceiptEnumView::Action { .. } => {
            !tx_hashes_for_receipts.contains_key(&crate::ReceiptOrDataId::ReceiptId(r.receipt_id))
        }
    });

    if receipts.is_empty() {
        return Ok(tx_hashes_for_receipts);
    }

    eprintln!(
        "Looking for parent transaction hash in database for {} receipts", // {:#?}",
        &receipts.len(),
        //  &receipts,
    );

    let (action_receipt_ids, data_ids): (Vec<String>, Vec<String>) =
        receipts.iter().partition_map(|r| match r.receipt {
            near_indexer_primitives::views::ReceiptEnumView::Action { .. } => {
                Either::Left(r.receipt_id.to_string())
            }
            near_indexer_primitives::views::ReceiptEnumView::Data { data_id, .. } => {
                Either::Right(data_id.to_string())
            }
        });

    if !data_ids.is_empty() {
        let tx_hashes_for_data_receipts =
            find_transaction_hashes_for_data_receipts(pool, &data_ids).await?;
        tx_hashes_for_receipts.extend(tx_hashes_for_data_receipts.clone());

        receipts.retain(|r| match r.receipt {
            near_indexer_primitives::views::ReceiptEnumView::Action { .. } => true,
            near_indexer_primitives::views::ReceiptEnumView::Data { data_id, .. } => {
                !tx_hashes_for_data_receipts.contains_key(&crate::ReceiptOrDataId::DataId(data_id))
            }
        });
        if receipts.is_empty() {
            return Ok(tx_hashes_for_receipts);
        }
    }

    if !action_receipt_ids.is_empty() {
        let tx_hashes_for_receipts_via_outcomes =
            find_transaction_hashes_for_receipts_via_outcomes(pool, &action_receipt_ids).await?;
        tx_hashes_for_receipts.extend(tx_hashes_for_receipts_via_outcomes.clone());

        receipts.retain(|r| {
            !tx_hashes_for_receipts_via_outcomes
                .contains_key(&crate::ReceiptOrDataId::ReceiptId(r.receipt_id))
        });
        if receipts.is_empty() {
            return Ok(tx_hashes_for_receipts);
        }

        let tx_hashes_for_receipt_via_transactions =
            find_transaction_hashes_for_receipt_via_transactions(pool, &action_receipt_ids).await?;
        tx_hashes_for_receipts.extend(tx_hashes_for_receipt_via_transactions.clone());

        receipts.retain(|r| {
            !tx_hashes_for_receipt_via_transactions
                .contains_key(&crate::ReceiptOrDataId::ReceiptId(r.receipt_id))
        });
    }

    if !receipts.is_empty() {
        eprintln!(
            "The block {} has {} receipt(s) we still need to put to the DB later: {:?}",
            block_height,
            receipts.len(),
            receipts
                .iter()
                .map(|r| r.receipt_id.to_string())
                .collect::<Vec<String>>()
        );
        if strict_mode {
            panic!("all the transactions should be found by this place");
        }

        let mut args = sqlx::postgres::PgArguments::default();
        args.add(BigDecimal::from(block_height));
        let query = "INSERT INTO _blocks_to_rerun VALUES ($1) ON CONFLICT DO NOTHING";
        sqlx::query_with(query, args).execute(pool).await?;
    }

    Ok(tx_hashes_for_receipts)
}

async fn find_transaction_hashes_for_data_receipts(
    pool: &sqlx::Pool<sqlx::Postgres>,
    data_ids: &[String],
) -> anyhow::Result<HashMap<crate::ReceiptOrDataId, crate::ParentTransactionHashString>> {
    let query = "SELECT action_receipts__outputs.output_data_id, action_receipts.originated_from_transaction_hash
                        FROM action_receipts__outputs JOIN action_receipts ON action_receipts__outputs.receipt_id = action_receipts.receipt_id
                        WHERE action_receipts__outputs.output_data_id IN ".to_owned() + &models::create_placeholder(&mut 1,data_ids.len())?;

    let res = models::select_retry_or_panic(pool, &query, data_ids).await?;
    Ok(res
        .iter()
        .map(|q| (q.get(0), q.get(1)))
        .map(
            |(data_id_string, transaction_hash_string): (String, String)| {
                (
                    crate::ReceiptOrDataId::DataId(
                        near_indexer_primitives::CryptoHash::from_str(&data_id_string)
                            .expect("Failed to convert String to CryptoHash"),
                    ),
                    transaction_hash_string,
                )
            },
        )
        .collect())
}

async fn find_transaction_hashes_for_receipts_via_outcomes(
    pool: &sqlx::Pool<sqlx::Postgres>,
    action_receipt_ids: &[String],
) -> anyhow::Result<HashMap<crate::ReceiptOrDataId, crate::ParentTransactionHashString>> {
    let query = "SELECT execution_outcomes__receipts.produced_receipt_id, action_receipts.originated_from_transaction_hash
                        FROM execution_outcomes__receipts JOIN action_receipts ON execution_outcomes__receipts.executed_receipt_id = action_receipts.receipt_id
                        WHERE execution_outcomes__receipts.produced_receipt_id IN ".to_owned() + &models::create_placeholder(&mut 1,action_receipt_ids.len())?;

    let res = models::select_retry_or_panic(pool, &query, action_receipt_ids).await?;
    Ok(res
        .iter()
        .map(|q| (q.get(0), q.get(1)))
        .map(
            |(receipt_id_string, transaction_hash_string): (String, String)| {
                (
                    crate::ReceiptOrDataId::ReceiptId(
                        near_indexer_primitives::CryptoHash::from_str(&receipt_id_string)
                            .expect("Failed to convert String to CryptoHash"),
                    ),
                    transaction_hash_string,
                )
            },
        )
        .collect())
}

async fn find_transaction_hashes_for_receipt_via_transactions(
    pool: &sqlx::Pool<sqlx::Postgres>,
    action_receipt_ids: &[String],
) -> anyhow::Result<HashMap<crate::ReceiptOrDataId, crate::ParentTransactionHashString>> {
    let query = "SELECT converted_into_receipt_id, transaction_hash
                        FROM transactions
                        WHERE converted_into_receipt_id IN "
        .to_owned()
        + &models::create_placeholder(&mut 1, action_receipt_ids.len())?;

    let res = models::select_retry_or_panic(pool, &query, action_receipt_ids).await?;
    Ok(res
        .iter()
        .map(|q| (q.get(0), q.get(1)))
        .map(
            |(receipt_id_string, transaction_hash_string): (String, String)| {
                (
                    crate::ReceiptOrDataId::ReceiptId(
                        near_indexer_primitives::CryptoHash::from_str(&receipt_id_string)
                            .expect("Failed to convert String to CryptoHash"),
                    ),
                    transaction_hash_string,
                )
            },
        )
        .collect())
}

async fn store_receipt_actions(
    pool: &sqlx::Pool<sqlx::Postgres>,
    receipts: Vec<(usize, &String, &near_indexer_primitives::views::ReceiptView)>,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    chunk_header: &near_indexer_primitives::views::ChunkHeaderView,
) -> anyhow::Result<()> {
    let receipt_actions: Vec<models::ActionReceipt> = receipts
        .iter()
        .filter_map(|(index, tx, receipt)| {
            models::ActionReceipt::try_from_action_receipt_view(
                *receipt,
                &block_header.hash,
                *tx,
                chunk_header,
                *index as i32,
                block_header.timestamp,
            )
            .ok()
        })
        .collect();

    let receipt_action_actions: Vec<models::ActionReceiptAction> = receipts
        .iter()
        .filter_map(|(_, _, receipt)| {
            if let near_indexer_primitives::views::ReceiptEnumView::Action { actions, .. } =
                &receipt.receipt
            {
                Some(actions.iter().map(move |action| {
                    models::ActionReceiptAction::from_action_view(
                        receipt.receipt_id.to_string(),
                        action,
                        receipt.predecessor_id.to_string(),
                        receipt.receiver_id.to_string(),
                        block_header,
                        chunk_header.shard_id as i32,
                        // we fill it later because we can't enumerate before filtering finishes
                        0,
                    )
                }))
            } else {
                None
            }
        })
        .flatten()
        .enumerate()
        .map(|(i, mut action)| {
            action.index_in_chunk = i as i32;
            action
        })
        .collect();

    let receipt_action_output_data: Vec<models::ActionReceiptsOutput> = receipts
        .iter()
        .filter_map(|(_, _, receipt)| {
            if let near_indexer_primitives::views::ReceiptEnumView::Action {
                output_data_receivers,
                ..
            } = &receipt.receipt
            {
                Some(output_data_receivers.iter().map(move |receiver| {
                    models::ActionReceiptsOutput::from_data_receiver(
                        receipt.receipt_id.to_string(),
                        receiver,
                        &block_header.hash,
                        block_header.timestamp,
                        chunk_header.shard_id as i32,
                        // we fill it later because we can't enumerate before filtering finishes
                        0,
                    )
                }))
            } else {
                None
            }
        })
        .flatten()
        .enumerate()
        .map(|(i, mut output)| {
            output.index_in_chunk = i as i32;
            output
        })
        .collect();

    // Next 2 tables depend on action_receipts, so we have to wait for it at first
    models::chunked_insert(pool, &receipt_actions).await?;
    try_join!(
        models::chunked_insert(pool, &receipt_action_actions),
        models::chunked_insert(pool, &receipt_action_output_data),
    )?;

    Ok(())
}

async fn store_data_receipts(
    pool: &sqlx::Pool<sqlx::Postgres>,
    receipts: Vec<(usize, &String, &near_indexer_primitives::views::ReceiptView)>,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    chunk_header: &near_indexer_primitives::views::ChunkHeaderView,
) -> anyhow::Result<()> {
    models::chunked_insert(
        pool,
        &receipts
            .iter()
            .filter_map(|(index, tx, receipt)| {
                models::DataReceipt::try_from_data_receipt_view(
                    receipt,
                    &block_header.hash,
                    tx,
                    chunk_header,
                    *index as i32,
                    block_header.timestamp,
                )
                .ok()
            })
            .collect::<Vec<models::DataReceipt>>(),
    )
    .await
}
