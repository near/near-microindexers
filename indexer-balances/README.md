# Indexer Balances

Async Postgres-compatible solution to load the data from NEAR blockchain.
Based on [NEAR Lake Framework](https://github.com/near/near-lake-framework-rs).

See [Indexer Base](https://github.com/near/near-indexer-base#indexer-base) docs for all the explanations, installation guide, etc.

### Why `account_changes` is not enough?

1. `account_changes` has only the absolute value for the balance, while we want to see the delta;
2. `account_changes` does not have involved account_id.

### What else do I need to know?

The code does non-trivial work with extracting the balance-changing events and trying to store them in the correct order.

The ordering is taken from the [nearcore implementation](https://github.com/near/nearcore/blob/master/runtime/runtime/src/lib.rs#L1136):
1. validators account update
2. process transactions
3. process receipts

Using [Indexer For Explorer](https://github.com/near/near-indexer-for-explorer) terminology, we merge `account_changes` and `action_receipt_actions` by `receipt_id`.

We have the natural order in these 2 arrays.
1. If `receipt_id` is stored in both arrays -> merge them to one line in the resulting table.
2. If `receipt_id` from `action_receipt_actions` has no pair in `account_changes` -> collect all the possible info from `action_receipt_actions` and put the line in the resulting table.
3. If the line in `account_changes` has no `receipt_id`, we need to check whether it changed someone's balance. If the balance was changed -> collect all the possible info from `account_changes` and put the line in the resulting table.

While merging, we can meet the situation #2 and #3 at the same point of time.
We need to find the right order of storing such cases.  
I feel these 2 situations never affect each other, so any order will work fine.
I decided to put `account_changes` data first (just to be consistent)

### Contribution Guide

Please refer to this [guide](https://github.com/near/near-indexer-for-explorer/blob/master/CONTRIBUTING.md) before submitting PRs to this repo 
