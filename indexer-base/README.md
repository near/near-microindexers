# Indexer Base

Async Postgres-compatible solution to load the data from NEAR blockchain.
Based on [NEAR Lake Framework](https://github.com/near/near-lake-framework-rs).

[Indexer For Explorer](https://github.com/near/near-indexer-for-explorer) has some disadvantages that we wanted to fix.
That's why we've created smaller projects, almost independent mini-indexers:
- [Indexer Base](https://github.com/near/near-indexer-base) works with basic information about transactions, receipts;
- [Indexer Accounts](https://github.com/near/near-indexer-accounts) works with accounts and access_keys;
- [Indexer Balances](https://github.com/near/near-indexer-balances) collects the info about native NEAR token balance changes;
- [Indexer Events](https://github.com/near/near-indexer-events) works with events produced by NEPs (FT, NFT, etc).

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

## Redshift

We keep in mind the possibility to move the data to AWS Redshift.
Some notes are [here](redshift/REDSHIFT_NOTES.md).