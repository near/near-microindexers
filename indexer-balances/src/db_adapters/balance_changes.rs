use cached::Cached;
use std::collections::HashMap;
use std::ops::Sub;
use std::str::FromStr;

use crate::models::balance_changes::NearBalanceEvent;
use crate::models::PrintEnum;
use bigdecimal::BigDecimal;
use futures::future::try_join_all;
use near_jsonrpc_client::errors::JsonRpcError;
use near_jsonrpc_primitives::types::query::RpcQueryError;
use near_lake_framework::near_indexer_primitives::{
    self,
    views::{ExecutionStatusView, StateChangeCauseView},
};
use num_traits::Zero;

// https://explorer.near.org/transactions/FGSPpucGQBUTPscfjQRs7Poo4XyaXGawX6QriKbhT3sE#7nu7ZAK3T11erEgG8aWTRGmz9uTHGazoNMjJdVyG3piX

// https://nomicon.io/RuntimeSpec/ApplyingChunk#processing-order
pub(crate) async fn store_balance_changes(
    pool: &sqlx::Pool<sqlx::Postgres>,
    shards: &[near_indexer_primitives::IndexerShard],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    balances_cache: &crate::BalanceCache,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
) -> anyhow::Result<()> {
    let futures = shards.iter().map(|shard| {
        store_changes_for_chunk(pool, shard, block_header, balances_cache, json_rpc_client)
    });

    try_join_all(futures).await.map(|_| ())
}

#[derive(Debug, Default)]
struct AccountChangesBalances {
    pub validators: Vec<crate::AccountWithBalance>,
    pub transactions: HashMap<near_indexer_primitives::CryptoHash, crate::AccountWithBalance>,
    pub receipts: HashMap<near_indexer_primitives::CryptoHash, crate::AccountWithBalance>,
    pub rewards: HashMap<near_indexer_primitives::CryptoHash, crate::AccountWithBalance>,
}

async fn store_changes_for_chunk(
    pool: &sqlx::Pool<sqlx::Postgres>,
    shard: &near_indexer_primitives::IndexerShard,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    balances_cache: &crate::BalanceCache,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
) -> anyhow::Result<()> {
    let mut changes: Vec<NearBalanceEvent> = vec![];
    let mut changes_data =
        collect_data_from_balance_changes(&shard.state_changes, block_header.height)?;
    // We should collect these 3 groups sequentially because they all share the same cache
    changes.extend(
        store_validator_accounts_update_for_chunk(
            &changes_data.validators,
            block_header,
            balances_cache,
            json_rpc_client,
        )
        .await?,
    );
    match shard.chunk.as_ref().map(|chunk| &chunk.transactions) {
        None => {}
        Some(x) => changes.extend(
            store_transaction_execution_outcomes_for_chunk(
                x,
                &mut changes_data.transactions,
                block_header,
                balances_cache,
                json_rpc_client,
            )
            .await?,
        ),
    }

    changes.extend(
        store_receipt_execution_outcomes_for_chunk(
            &shard.receipt_execution_outcomes,
            &mut changes_data.receipts,
            &mut changes_data.rewards,
            block_header,
            balances_cache,
            json_rpc_client,
        )
        .await?,
    );

    let start_from_index: u128 = (block_header.timestamp as u128) * 100_000_000 * 100_000_000
        + (shard.shard_id as u128) * 10_000_000;
    for (i, change) in changes.iter_mut().enumerate() {
        change.event_index = BigDecimal::from_str(&(start_from_index + i as u128).to_string())?;
    }
    crate::models::chunked_insert(pool, &changes, 10).await?;
    Ok(())
}

fn collect_data_from_balance_changes(
    state_changes: &near_indexer_primitives::views::StateChangesView,
    block_height: u64,
) -> anyhow::Result<AccountChangesBalances> {
    let mut result: AccountChangesBalances = Default::default();

    for state_change_with_cause in state_changes {
        let near_indexer_primitives::views::StateChangeWithCauseView { cause, value } =
            state_change_with_cause;

        let account_details = match value {
            near_indexer_primitives::views::StateChangeValueView::AccountUpdate {
                account_id,
                account,
            } => crate::AccountWithBalance {
                account_id: account_id.clone(),
                balance: crate::BalanceDetails {
                    non_staked: account.amount,
                    staked: account.locked,
                },
            },
            near_indexer_primitives::views::StateChangeValueView::AccountDeletion {
                account_id,
            } => crate::AccountWithBalance {
                account_id: account_id.clone(),
                balance: crate::BalanceDetails {
                    non_staked: 0,
                    staked: 0,
                },
            },
            // other values do not provide balance changes
            _ => continue,
        };

        match cause {
            StateChangeCauseView::NotWritableToDisk
            | StateChangeCauseView::InitialState
            | StateChangeCauseView::ActionReceiptProcessingStarted { .. }
            | StateChangeCauseView::UpdatedDelayedReceipts
            | StateChangeCauseView::PostponedReceipt { .. }
            | StateChangeCauseView::Resharding => {
                anyhow::bail!("Unexpected state change cause met: {:#?}", cause);
            }
            StateChangeCauseView::ValidatorAccountsUpdate => {
                result.validators.push(account_details);
            }
            StateChangeCauseView::TransactionProcessing { tx_hash } => {
                let prev_inserted_item = result
                    .transactions
                    .insert(*tx_hash, account_details.clone());
                if let Some(details) = prev_inserted_item {
                    anyhow::bail!(
                        "Duplicated balance changes for transaction {} at block_height {}. \
                        One of them may be missed\n{:#?}\n{:#?}",
                        tx_hash.to_string(),
                        block_height,
                        account_details,
                        details
                    );
                }
            }
            StateChangeCauseView::Migration {} => {
                // We had this reason once, in block 44337060
                // It does not affect balances, so we can skip it
            }
            StateChangeCauseView::ActionReceiptGasReward { receipt_hash } => {
                let prev_inserted_item = result
                    .rewards
                    .insert(*receipt_hash, account_details.clone());
                if let Some(details) = prev_inserted_item {
                    anyhow::bail!(
                        "Duplicated balance changes for receipt {} (reward), at block_height {}. \
                        One of them may be missed\n{:#?}\n{:#?}",
                        receipt_hash.to_string(),
                        block_height,
                        account_details,
                        details
                    );
                }
            }
            StateChangeCauseView::ReceiptProcessing { receipt_hash } => {
                let prev_inserted_item = result
                    .receipts
                    .insert(*receipt_hash, account_details.clone());
                if let Some(details) = prev_inserted_item {
                    anyhow::bail!(
                        "Duplicated balance changes for receipt {} at block_height {}. \
                        One of them may be missed\n{:#?}\n{:#?}",
                        receipt_hash.to_string(),
                        block_height,
                        account_details,
                        details
                    );
                }
            }
        }
    }
    Ok(result)
}

async fn store_validator_accounts_update_for_chunk(
    validator_changes: &[crate::AccountWithBalance],
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    balances_cache: &crate::BalanceCache,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
) -> anyhow::Result<Vec<NearBalanceEvent>> {
    let mut result: Vec<NearBalanceEvent> = vec![];
    for new_details in validator_changes {
        let prev_balance = get_balance_retriable(
            &new_details.account_id,
            &block_header.prev_hash,
            balances_cache,
            json_rpc_client,
        )
        .await?;
        let deltas = get_deltas(&new_details.balance, &prev_balance)?;
        save_latest_balance(
            new_details.account_id.clone(),
            &new_details.balance,
            balances_cache,
        )
        .await;

        result.push(NearBalanceEvent {
            event_index: BigDecimal::zero(), // will enumerate later
            block_timestamp: block_header.timestamp.into(),
            block_height: block_header.height.into(),
            receipt_id: None,
            transaction_hash: None,
            affected_account_id: new_details.account_id.to_string(),
            involved_account_id: None,
            direction: crate::models::Direction::Inbound.print().to_string(),
            cause: crate::models::Cause::ValidatorsReward.print().to_string(),
            status: ExecutionStatusView::SuccessValue("".to_string())
                .print()
                .to_string(),
            delta_nonstaked_amount: deltas.0,
            absolute_nonstaked_amount: BigDecimal::from_str(
                &new_details.balance.non_staked.to_string(),
            )
            .unwrap(),
            delta_staked_amount: deltas.1,
            absolute_staked_amount: BigDecimal::from_str(&new_details.balance.staked.to_string())
                .unwrap(),
        });
    }

    Ok(result)
}

async fn store_transaction_execution_outcomes_for_chunk(
    transactions: &[near_indexer_primitives::IndexerTransactionWithOutcome],
    transaction_changes: &mut HashMap<
        near_indexer_primitives::CryptoHash,
        crate::AccountWithBalance,
    >,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    balances_cache: &crate::BalanceCache,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
) -> anyhow::Result<Vec<NearBalanceEvent>> {
    let mut result: Vec<NearBalanceEvent> = vec![];

    for transaction in transactions {
        let affected_account_id = &transaction.transaction.signer_id;
        let involved_account_id = match transaction.transaction.receiver_id.as_str() {
            "system" => None,
            _ => Some(&transaction.transaction.receiver_id),
        };

        let prev_balance = get_balance_retriable(
            affected_account_id,
            &block_header.prev_hash,
            balances_cache,
            json_rpc_client,
        )
        .await?;

        let details_after_transaction = transaction_changes
            .remove(&transaction.transaction.hash)
            .ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to find balance change for transaction {}",
                &transaction.transaction.hash.to_string()
            )
        })?;

        if details_after_transaction.account_id != *affected_account_id {
            anyhow::bail!(
                "Unexpected balance change info found for transaction {}.\nExpected account_id {},\nActual account_id {}",
                &transaction.transaction.hash.to_string(),
                affected_account_id.to_string(),
                details_after_transaction.account_id.to_string()
            );
        }

        let deltas = get_deltas(&details_after_transaction.balance, &prev_balance)?;
        save_latest_balance(
            affected_account_id.clone(),
            &details_after_transaction.balance,
            balances_cache,
        )
        .await;

        result.push(NearBalanceEvent {
            event_index: BigDecimal::zero(), // will enumerate later
            block_timestamp: block_header.timestamp.into(),
            block_height: block_header.height.into(),
            receipt_id: None,
            transaction_hash: Some(transaction.transaction.hash.to_string()),
            affected_account_id: affected_account_id.to_string(),
            involved_account_id: involved_account_id.map(|id| id.to_string()),
            direction: crate::models::Direction::Outbound.print().to_string(),
            cause: crate::models::Cause::Transaction.print().to_string(),
            status: transaction
                .outcome
                .execution_outcome
                .outcome
                .status
                .print()
                .to_string(),
            delta_nonstaked_amount: deltas.0,
            absolute_nonstaked_amount: BigDecimal::from_str(
                &details_after_transaction.balance.non_staked.to_string(),
            )
            .unwrap(),
            delta_staked_amount: deltas.1,
            absolute_staked_amount: BigDecimal::from_str(
                &details_after_transaction.balance.staked.to_string(),
            )
            .unwrap(),
        });

        // Adding the opposite entry to the DB, just to show that the second account_id was there too
        if let Some(account_id) = involved_account_id {
            if account_id != affected_account_id {
                // balance is not changing here, we just note the line here
                let balance = get_balance_retriable(
                    account_id,
                    &block_header.prev_hash,
                    balances_cache,
                    json_rpc_client,
                )
                .await?;
                result.push(NearBalanceEvent {
                    event_index: BigDecimal::zero(), // will enumerate later
                    block_timestamp: block_header.timestamp.into(),
                    block_height: block_header.height.into(),
                    receipt_id: None,
                    transaction_hash: Some(transaction.transaction.hash.to_string()),
                    affected_account_id: account_id.to_string(),
                    involved_account_id: Some(affected_account_id.to_string()),
                    direction: crate::models::Direction::Inbound.print().to_string(),
                    cause: crate::models::Cause::Transaction.print().to_string(),
                    status: transaction
                        .outcome
                        .execution_outcome
                        .outcome
                        .status
                        .print()
                        .to_string(),
                    delta_nonstaked_amount: BigDecimal::zero(),
                    absolute_nonstaked_amount: BigDecimal::from_str(
                        &balance.non_staked.to_string(),
                    )
                    .unwrap(),
                    delta_staked_amount: BigDecimal::zero(),
                    absolute_staked_amount: BigDecimal::from_str(&balance.staked.to_string())
                        .unwrap(),
                });
            }
        }
    }

    if !transaction_changes.is_empty() {
        anyhow::bail!(
            "{} changes for transactions were not applied, block_height {}\n{:#?}",
            transaction_changes.len(),
            block_header.height,
            transaction_changes
        );
    }

    Ok(result)
}

async fn store_receipt_execution_outcomes_for_chunk(
    outcomes_with_receipts: &[near_indexer_primitives::IndexerExecutionOutcomeWithReceipt],
    receipt_changes: &mut HashMap<near_indexer_primitives::CryptoHash, crate::AccountWithBalance>,
    reward_changes: &mut HashMap<near_indexer_primitives::CryptoHash, crate::AccountWithBalance>,
    block_header: &near_indexer_primitives::views::BlockHeaderView,
    balances_cache: &crate::BalanceCache,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
) -> anyhow::Result<Vec<NearBalanceEvent>> {
    let mut result: Vec<NearBalanceEvent> = vec![];

    for outcome_with_receipt in outcomes_with_receipts {
        let receipt_id = &outcome_with_receipt.receipt.receipt_id;
        // predecessor has made the action, as the result, receiver's balance may change
        let affected_account_id = &outcome_with_receipt.receipt.receiver_id;
        let involved_account_id = match outcome_with_receipt.receipt.predecessor_id.as_str() {
            "system" => None,
            _ => Some(&outcome_with_receipt.receipt.predecessor_id),
        };

        if let Some(details_after_receipt) = receipt_changes.remove(receipt_id) {
            if details_after_receipt.account_id != *affected_account_id {
                anyhow::bail!(
                "Unexpected balance change info found for receipt {}.\nExpected account_id {},\nActual account_id {}",
                receipt_id.to_string(),
                affected_account_id.to_string(),
                details_after_receipt.account_id.to_string()
            );
            }

            let prev_balance = get_balance_retriable(
                affected_account_id,
                &block_header.prev_hash,
                balances_cache,
                json_rpc_client,
            )
            .await?;

            let deltas = get_deltas(&details_after_receipt.balance, &prev_balance)?;
            save_latest_balance(
                affected_account_id.clone(),
                &details_after_receipt.balance,
                balances_cache,
            )
            .await;

            result.push(NearBalanceEvent {
                event_index: BigDecimal::zero(), // will enumerate later
                block_timestamp: block_header.timestamp.into(),
                block_height: block_header.height.into(),
                receipt_id: Some(receipt_id.to_string()),
                transaction_hash: None,
                affected_account_id: affected_account_id.to_string(),
                involved_account_id: involved_account_id.map(|id| id.to_string()),
                direction: crate::models::Direction::Inbound.print().to_string(),
                cause: crate::models::Cause::Receipt.print().to_string(),
                status: outcome_with_receipt
                    .execution_outcome
                    .outcome
                    .status
                    .print()
                    .to_string(),
                delta_nonstaked_amount: deltas.0,
                absolute_nonstaked_amount: BigDecimal::from_str(
                    &details_after_receipt.balance.non_staked.to_string(),
                )
                .unwrap(),
                delta_staked_amount: deltas.1,
                absolute_staked_amount: BigDecimal::from_str(
                    &details_after_receipt.balance.staked.to_string(),
                )
                .unwrap(),
            });

            // Adding the opposite entry to the DB, just to show that the second account_id was there too
            if let Some(account_id) = involved_account_id {
                if account_id != affected_account_id {
                    // balance is not changing here, we just note the line here
                    let balance = get_balance_retriable(
                        account_id,
                        &block_header.prev_hash,
                        balances_cache,
                        json_rpc_client,
                    )
                    .await?;
                    result.push(NearBalanceEvent {
                        event_index: BigDecimal::zero(), // will enumerate later
                        block_timestamp: block_header.timestamp.into(),
                        block_height: block_header.height.into(),
                        receipt_id: Some(receipt_id.to_string()),
                        transaction_hash: None,
                        affected_account_id: account_id.to_string(),
                        involved_account_id: Some(affected_account_id.to_string()),
                        direction: crate::models::Direction::Outbound.print().to_string(),
                        cause: crate::models::Cause::Receipt.print().to_string(),
                        status: outcome_with_receipt
                            .execution_outcome
                            .outcome
                            .status
                            .print()
                            .to_string(),
                        delta_nonstaked_amount: BigDecimal::zero(),
                        absolute_nonstaked_amount: BigDecimal::from_str(
                            &balance.non_staked.to_string(),
                        )
                        .unwrap(),
                        delta_staked_amount: BigDecimal::zero(),
                        absolute_staked_amount: BigDecimal::from_str(&balance.staked.to_string())
                            .unwrap(),
                    });
                }
            }
        }

        // REWARDS
        if let Some(details_after_reward) = reward_changes.remove(receipt_id) {
            if details_after_reward.account_id != *affected_account_id {
                anyhow::bail!(
                "Unexpected balance change info found for receipt_id {} (reward).\nExpected account_id {},\nActual account_id {}",
                receipt_id.to_string(),
                affected_account_id.to_string(),
                details_after_reward.account_id.to_string()
            );
            }

            let prev_balance = get_balance_retriable(
                affected_account_id,
                &block_header.prev_hash,
                balances_cache,
                json_rpc_client,
            )
            .await?;
            let deltas = get_deltas(&details_after_reward.balance, &prev_balance)?;
            save_latest_balance(
                affected_account_id.clone(),
                &details_after_reward.balance,
                balances_cache,
            )
            .await;

            result.push(NearBalanceEvent {
                event_index: BigDecimal::zero(), // will enumerate later
                block_timestamp: block_header.timestamp.into(),
                block_height: block_header.height.into(),
                receipt_id: Some(receipt_id.to_string()),
                transaction_hash: None,
                affected_account_id: affected_account_id.to_string(),
                involved_account_id: involved_account_id.map(|id| id.to_string()),
                direction: crate::models::Direction::Inbound.print().to_string(),
                cause: crate::models::Cause::ContractReward.print().to_string(),
                status: outcome_with_receipt
                    .execution_outcome
                    .outcome
                    .status
                    .print()
                    .to_string(),
                delta_nonstaked_amount: deltas.0,
                absolute_nonstaked_amount: BigDecimal::from_str(
                    &details_after_reward.balance.non_staked.to_string(),
                )
                .unwrap(),
                delta_staked_amount: deltas.1,
                absolute_staked_amount: BigDecimal::from_str(
                    &details_after_reward.balance.staked.to_string(),
                )
                .unwrap(),
            });
        }
    }

    if !receipt_changes.is_empty() {
        anyhow::bail!(
            "{} changes for receipts were not applied, block_height {}\n{:#?}",
            receipt_changes.len(),
            block_header.height,
            receipt_changes
        );
    }
    if !reward_changes.is_empty() {
        anyhow::bail!(
            "{} reward changes for receipts were not applied, block_height {}\n{:#?}",
            reward_changes.len(),
            block_header.height,
            reward_changes
        );
    }

    Ok(result)
}

fn get_deltas(
    new_balance: &crate::BalanceDetails,
    old_balance: &crate::BalanceDetails,
) -> anyhow::Result<(BigDecimal, BigDecimal)> {
    Ok((
        BigDecimal::from_str(&new_balance.non_staked.to_string())?
            .sub(BigDecimal::from_str(&old_balance.non_staked.to_string())?),
        BigDecimal::from_str(&new_balance.staked.to_string())?
            .sub(BigDecimal::from_str(&old_balance.staked.to_string())?),
    ))
}

async fn get_balance_retriable(
    account_id: &near_indexer_primitives::types::AccountId,
    block_hash: &near_indexer_primitives::CryptoHash,
    balance_cache: &crate::BalanceCache,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
) -> anyhow::Result<crate::BalanceDetails> {
    let mut interval = crate::INTERVAL;
    let mut retry_attempt = 0usize;

    loop {
        if retry_attempt == crate::RETRY_COUNT {
            anyhow::bail!(
                "Failed to perform query to RPC after {} attempts. Stop trying.\nAccount {}, block_hash {}",
                crate::RETRY_COUNT,
                account_id.to_string(),
                block_hash.to_string()
            );
        }
        retry_attempt += 1;

        match get_balance(account_id, block_hash, balance_cache, json_rpc_client).await {
            Ok(res) => return Ok(res),
            Err(err) => {
                tracing::error!(
                    target: crate::LOGGING_PREFIX,
                    "Failed to request account view details from RPC for account {}, block_hash {}.{}\n Retrying in {} milliseconds...",
                    account_id.to_string(),
                    block_hash.to_string(),
                    err,
                    interval.as_millis(),
                );
                tokio::time::sleep(interval).await;
                if interval < crate::MAX_DELAY_TIME {
                    interval *= 2;
                }
            }
        }
    }
}

async fn get_balance(
    account_id: &near_indexer_primitives::types::AccountId,
    block_hash: &near_indexer_primitives::CryptoHash,
    balance_cache: &crate::BalanceCache,
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
) -> anyhow::Result<crate::BalanceDetails> {
    let mut balances_cache_lock = balance_cache.lock().await;
    let result = match balances_cache_lock.cache_get(account_id) {
        None => {
            let account_balance =
                match get_account_view(json_rpc_client, account_id, block_hash).await {
                    Ok(account_view) => Ok(crate::BalanceDetails {
                        non_staked: account_view.amount,
                        staked: account_view.locked,
                    }),
                    Err(err) => match err.handler_error() {
                        Some(RpcQueryError::UnknownAccount { .. }) => Ok(crate::BalanceDetails {
                            non_staked: 0,
                            staked: 0,
                        }),
                        _ => Err(err.into()),
                    },
                };
            if let Ok(balance) = account_balance {
                balances_cache_lock.cache_set(account_id.clone(), balance);
            }
            account_balance
        }
        Some(balance) => Ok(*balance),
    };
    drop(balances_cache_lock);
    result
}

async fn save_latest_balance(
    account_id: near_indexer_primitives::types::AccountId,
    balance: &crate::BalanceDetails,
    balance_cache: &crate::BalanceCache,
) {
    let mut balances_cache_lock = balance_cache.lock().await;
    balances_cache_lock.cache_set(
        account_id,
        crate::BalanceDetails {
            non_staked: balance.non_staked,
            staked: balance.staked,
        },
    );
    drop(balances_cache_lock);
}

async fn get_account_view(
    json_rpc_client: &near_jsonrpc_client::JsonRpcClient,
    account_id: &near_indexer_primitives::types::AccountId,
    block_hash: &near_indexer_primitives::CryptoHash,
) -> Result<near_indexer_primitives::views::AccountView, JsonRpcError<RpcQueryError>> {
    let query = near_jsonrpc_client::methods::query::RpcQueryRequest {
        block_reference: near_primitives::types::BlockReference::BlockId(
            near_primitives::types::BlockId::Hash(*block_hash),
        ),
        request: near_primitives::views::QueryRequest::ViewAccount {
            account_id: account_id.clone(),
        },
    };

    let account_response = json_rpc_client.call(query).await?;
    match account_response.kind {
        near_jsonrpc_primitives::types::query::QueryResponseKind::ViewAccount(account) => {
            Ok(account)
        }
        _ => unreachable!(
            "Unreachable code! Asked for ViewAccount (block_hash {}, account_id {})\nReceived\n\
                {:#?}\nReport this to https://github.com/near/near-jsonrpc-client-rs",
            block_hash.to_string(),
            account_id.to_string(),
            account_response.kind
        ),
    }
}
