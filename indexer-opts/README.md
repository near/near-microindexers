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
- `start-block-height` | **Required** Block height to start the stream from (not used if `start_mode == from-latest`)
- `near-archival-rpc-url` | **Required** NEAR JSON RPC URL
- `chain-id` | **Required** Chain id: testnet or mainnet, used for NEAR Lake initialization
- `port` | Default: 3000 Port to enable metrics/health service
- `start-mode` | Default: "from-interruption" Start mode for instance (`from-interruption`, `from-latest`)
- `database-url` | **Required** Database URL

### Important!

You need to create a table `meta` to store meta data about the running indexers. The SQL might be copied from [meta-table.sql](./meta-table.sql) to your migrations or applied manually.

TODO: come up with better way of using the table.


### Example

```
./indexer \
    --indexer-id domestic-racoon \
    --start-block-height 0 \
    --chain-id mainnet \
    --start-mode from-latest
    --database-url postgres://user:pass@host/db
```

This will start indexer with ID `domestic-racoon` for `mainnet` from the latest block. If it is restarted it will start from the latest block again. **Might skip blocks** depending on how long it was stopped.

```
./indexer \
    --indexer-id wild-snail \
    --start-block-height 100000000 \
    --chain-id mainnet \
    --start-mode from-interruption
    --database-url postgres://user:pass@host/db
```

This will start indexer with ID `wild-snail` for `mainnet` from the block height `100 000 000`. If it is restarted it will start from the block it was stopped. **Won't skip blocks** unless something wrong with the `meta` table of the database.
