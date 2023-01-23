# NEAR Microindexers

Async Postgres-compatible solution to load the data from NEAR blockchain.
Based on [NEAR Lake Framework](https://github.com/near/near-lake-framework-rs).

[Indexer For Explorer](https://github.com/near/near-indexer-for-explorer) has some disadvantages that we wanted to fix.
That's why we've created smaller projects, almost independent mini-indexers:
- `indexer-base` works with basic information about transactions, receipts;
- `indexer-accounts` works with accounts and access_keys;
- `indexer-balances` collects the info about native NEAR token balance changes (all the changes are validated);
- `indexer-events` works with events produced by NEPs: FT, NFT (the events need to be validated separately).

### What are the differences with Indexer For Explorer?

- The data model changed a bit, naming changed;
- We moved from `diesel` to `sqlx`, we prefer having lightweight ORM and write raw SQL queries;
- Separate projects are easier to maintain;
- The main difference is in the future: we are thinking where to go next if we decide to get rid of Postgres.

### Why do the projects _almost_ independent?

We still hope to leave foreign keys in the tables.
The data provided by all the indexers depend on Indexer Base.  
All the indexers may have the dependency to Indexer Accounts, but it will give us circular dependency, that's why we don't use these constraints.

### Can I create my own indexer?

Sure!
Feel free to use this project as the example.

## Linux installation guide

```bash
sudo apt install git build-essential pkg-config libssl-dev tmux postgresql-client libpq-dev -y
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
cargo install --version=0.5.13 sqlx-cli --features postgres
ulimit -n 30000
cargo build --release
#!!! here you need to create .env in the root of the project, and .aws in ~
cargo run --release -- --s3-bucket-name near-lake-data-mainnet --s3-region-name eu-central-1 --start-block-height 9820210
```

## Migrations

Unfortunately, migrations do not work if you have several projects writing to the same DB.
We still use the migrations folder in each project, but we have to apply the changes manually.

## Creating read-only PostgreSQL user

We highly recommend using a separate read-only user to access the data.
It helps you to avoid unexpected corruption of the indexed data.

We use `public` schema for all tables.
By default, new users have the possibility to create new tables/views/etc there.
If you want to restrict that, you have to revoke these rights:

```sql
REVOKE CREATE ON SCHEMA PUBLIC FROM PUBLIC;
REVOKE ALL PRIVILEGES ON ALL TABLES IN SCHEMA PUBLIC FROM PUBLIC;
ALTER DEFAULT PRIVILEGES IN SCHEMA PUBLIC GRANT SELECT ON TABLES TO PUBLIC;
```

After that, you could create read-only user in PostgreSQL:

```sql
CREATE ROLE readonly;
GRANT USAGE ON SCHEMA public TO readonly;
GRANT SELECT ON ALL TABLES IN SCHEMA public to readonly;
-- Put here your limit or just ignore this command
ALTER ROLE readonly SET statement_timeout = '30s';

CREATE USER explorer with login password 'password';
GRANT readonly TO explorer;
```

```bash
$ PGPASSWORD="password" psql -h 127.0.0.1 -U explorer databasename
```

### Contribution Guide

Please refer to this [guide](https://github.com/near/near-indexer-for-explorer/blob/master/CONTRIBUTING.md) before submitting PRs to this repo

## Why do we need `indexer-balances`? Why `account_changes` table is not enough?

1. `account_changes` has only the absolute value for the balance, while we want to see the delta;
2. `account_changes` does not have involved account_id.

`indexer-balances` implementation does non-trivial work with extracting the balance-changing events and storing them in the correct order.

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

## Why do we need `indexer-events`? Why `assets__*` tables are not enough?

`assets__non_fungible_token_events`, `assets__fungible_token_events` do not have the sorting column.
In the current solution, we've added artificial `event_index` column.

The new `fungible_token_events` table stores the data in the format of affected/involved account_id, that simplifies filtering by affected `account_id`.  
`fungible_token_events` still does not have `absolute_value` column, so you have to collect it from RPC if needed.

### What if my contract does not produce events?

Please go and update your contract with our new [SDK](https://github.com/near/near-sdk-rs).

If it's important for you to collect all the previous history as well, you need to make the contribution and implement your own legacy handler.  
You can use [existing handlers](src/db_adapters/coin/legacy) as the example, [wrap_near](src/db_adapters/coin/legacy/wrap_near.rs) may be a good starting point.

### What do I need to know about `indexer-accounts`?

Indexer Accounts is the only indexer that modifies the existing data.  
While other indexers are append-only, Indexer Accounts updates the existing records with the deletion info.

`accounts` table in [Indexer For Explorer](https://github.com/near/near-indexer-for-explorer) stored only the first creation and last deletion of the account.  
This solution stores all the creations/deletions, so accounts may appear in the table more than once.
