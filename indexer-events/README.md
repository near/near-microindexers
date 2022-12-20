# Indexer Events

Async Postgres-compatible solution to load the data from NEAR blockchain.
Based on [NEAR Lake Framework](https://github.com/near/near-lake-framework-rs).

See [Indexer Base](https://github.com/near/near-indexer-base#indexer-base) docs for all the explanations, installation guide, etc.

This solution collects balance-changing events about FTs, NFTs, etc.

- We can index the blockchain from any point of time. The code does not check if all the previous history is collected.
- Potentially, some events may be skipped.
- We do not check the correctness of collected events, it should be done separately.
- We can re-run infinite number of indexers writing at the same DB, they may index same or different parts of the blockchain. It should not break the flow.

### Why existing `assets__*` tables are not enough?

`assets__non_fungible_token_events`, `assets__fungible_token_events` do not have the sorting column.
In the current solution, we've added artificial `event_index` column.

The new `coin_events` table stores the data in the format of affected/involved account_id, that simplifies filtering by affected `account_id`.  
`coin_events` still does not have `absolute_value` column, so you have to collect it from RPC if needed.

### What if my contract does not produce events?

Please go and update your contract with our new [SDK](https://github.com/near/near-sdk-rs).

If it's important for you to collect all the previous history as well, you need to make the contribution and implement your own legacy handler.  
You can use [existing handlers](src/db_adapters/coin/legacy) as the example, [wrap_near](src/db_adapters/coin/legacy/wrap_near.rs) may be a good starting point.

### My contract produces events/there's a custom legacy logic for my contract, but the Enhanced API still ignores me. Why?

It means that we've found inconsistency in the data you provided with the data we've queried by RPC.  
To be more clear, we collected all the logs/performed all the legacy logic, we know all the changed balances for all the active users at the end of the block.
After that, we ask the RPC to provide all the needed balances.
The numbers should be the same.
If they are not the same, it means the data is inconsistent.

When we meet the inconsistency, we mark such contract as "non-trusted".  
If you want to fix this, you need to write/edit [legacy handler](src/db_adapters/coin/legacy/DOC.md) for your contract.

### Contribution Guide

Please refer to this [guide](https://github.com/near/near-indexer-for-explorer/blob/master/CONTRIBUTING.md) before submitting PRs to this repo 