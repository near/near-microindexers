# indexer-opts - Opts struct for CLI arguments for microindexers

This lib is a shared `clap::Parser` instance that defines CLI arguments for all microindexers.

### Features:

- Basic arguments for indexer to start
- `init_tracing` for initializing the logs for indexer
- `StartMode` that allows to choose the strategy for restarts:
  - `from-latest`
  - `from-interruption` (default)

### Parameters

- `indexer-id` | **Required** Sets the micro-indexer instance ID (for reading/writing indexer meta-data)
- `indexer-type` | **Required** Sets the micro-indexer instance type (for reading/writing indexer meta-data)
- `lake-aws-access-key` | **Required** AWS Access Key with the rights to read from AWS S3
- `lake-aws-secret-access-key` | **Required** AWS Secret Access Key with the rights to read from AWS S3
- `chain-id` | **Required** Chain id: testnet or mainnet, used for NEAR Lake initialization
- `database-url` | **Required** Database URL
- `start-block-height` | Block height to start the stream from (required if `start_mode == from-interruption`)
- `end-block-height` | Block to stop indexing at
- `rpc-url` | NEAR JSON RPC URL (required if `start_mode == from-latest`)
- `port` | Default: 3000 Port to enable metrics/health service
- `start-mode` | Default: "from-interruption" Start mode for instance (`from-interruption`, `from-latest`)

#### Using environment variables

Every parameter can be passed through the environment variable

Example:

```
INDEXER_ID=indexer-events-tip
INDEXER_TYPE=fill-gap-week
LAKE_AWS_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE
LAKE_AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
START_BLOCK_HEIGHT=0
NEAR_ARCHICAL_RPC_URL=https://archival.rpc.near.org
CHAIN_ID=mainnet
PORT=300
START_MODE=from-interruption
DATABASE_URL=postgres://user:pass@host/db
```

### Install & Use

Add `indexer-opts` as a dependency

```toml
[dependencies]

indexer-opts = { path = "../indexer-opts" }
```

Import necessary structs and functions

```rust
use indexer_opts::{init_tracing, Opts, Parser};
```

Update your `main()`

```rust
// parse arguments
let opts = Opts::parse();
// inititialize tracing the logs
let _worker_guard = init_tracing(opts.debug)?;
// create database connection pool
let pool = sqlx::PgPool::connect(&opts.database_url).await?;

// Register indexer
// it will try to insert the record about this indexer with
// its ID and TYPE and the block it is starting from.
// If the __meta table doesn't exist it will create it.
// If this indexer is already registered it will update the
// start_block_height in the __meta table
indexer_opts::update_meta(
    &pool,
    indexer_opts::MetaAction::RegisterIndexer {
        indexer_id: opts.indexer_id.to_string(),
        indexer_type: opts.indexer_type.to_string(),
        start_block_height: opts.start_block_height,
    },
)
.await?;
```

Add the code to update meta after processing each block (usually in the and of `handle_streamer_message` function)

```rust
// Update __meta with the last_processed_block_height for this indexer by its ID
//
// async fn handle_streamer_message(
//     streamer_message: near_indexer_primitives::StreamerMessage,
//     pool: &sqlx::Pool<sqlx::Postgres>,
//     chain_id: &str,
//     indexer_id: &str,
//     chain_id: &indexer_opts::ChainId,
// ) -> anyhow::Result<u64> {
match indexer_opts::update_meta(
    &pool,
    indexer_opts::MetaAction::UpdateMeta {
        indexer_id: indexer_id.to_string(),
        last_processed_block_height: streamer_message.block.header.height,
    },
)
.await
{
    Ok(_) => {}
    Err(err) => {
        tracing::warn!(
            target: crate::LOGGING_PREFIX,
            "Failed to update meta for INDEXER ID {}\n{:#?}",
            indexer_id,
            err,
        );
    }
};
```

### Example

```
./indexer \
    --indexer-id domestic-racoon \
    --lake-aws-access-key AKIAIOSFODNN7EXAMPLE \
    --lake-aws-secret-access-key wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY \
    --start-block-height 0 \
    --chain-id mainnet \
    --start-mode from-latest
    --database-url postgres://user:pass@host/db
```

This will start indexer with ID `domestic-racoon` for `mainnet` from the latest block. If it is restarted it will start from the latest block again. **Might skip blocks** depending on how long it was stopped.

```
./indexer \
    --indexer-id wild-snail \
    --lake-aws-access-key AKIAIOSFODNN7EXAMPLE \
    --lake-aws-secret-access-key wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY \
    --start-block-height 100000000 \
    --chain-id mainnet \
    --start-mode from-interruption
    --database-url postgres://user:pass@host/db
```

This will start indexer with ID `wild-snail` for `mainnet` from the block height `100 000 000`. If it is restarted it will start from the block it was stopped. **Won't skip blocks** unless something wrong with the `meta` table of the database.
