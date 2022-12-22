use bigdecimal::{FromPrimitive, ToPrimitive};
pub use clap::{self, ArgEnum, Parser};

use near_jsonrpc_client::{methods, JsonRpcClient};
use near_lake_framework::near_indexer_primitives::types::{BlockReference, Finality};

/// NEAR Indexer Opts
/// Start options for NEAR micro indexers
#[derive(Parser, Debug)]
#[clap(
    version,
    author,
    about,
    disable_help_subcommand(true),
    propagate_version(true),
    next_line_help(true)
)]
pub struct Opts {
    /// Enabled Indexer for Explorer debug level of logs
    #[clap(long)]
    pub debug: bool,
    /// Sets the micro-indexer instance ID (for reading/writing indexer meta-data)
    #[clap(long, env)]
    pub indexer_id: String,
    /// Sets the micro-indexer instance type (for reading/writing indexer meta-data)
    #[clap(long, env)]
    pub indexer_type: String,
    /// Block height to start the stream from
    #[clap(long, short, env)]
    pub start_block_height: Option<u64>,
    /// Block to stop indexing at
    #[clap(long, short, env)]
    pub end_block_height: Option<u64>,
    /// NEAR JSON RPC URL
    #[clap(long, short, env)]
    pub rpc_url: Option<String>,
    // Chain ID: testnet or mainnet, used for NEAR Lake initialization
    #[clap(long, env, arg_enum)]
    pub chain_id: ChainId,
    /// Port to enable metrics/health service
    #[clap(long, short, env, default_value_t = 3000)]
    pub port: u16,
    /// Start mode for instance
    #[clap(long, short, env, arg_enum, default_value = "from-interruption")]
    pub start_mode: StartMode,
    /// Database URL
    #[clap(long, short, env)]
    pub database_url: String,
}

/// Represents the chain-id variants for indexer to stream from
#[derive(ArgEnum, Debug, Clone, PartialEq, Eq)]
pub enum ChainId {
    Mainnet,
    Testnet,
}

/// Represents the variants of starts mode for the indexer
/// - FromLatest
///  will fetch the final block from the RPC by the given `rpc-url`
///  and then will attempt to register an indexer with given `indexer-id` and `indexer-type`
/// - FromInterruption
///  will register an indexer with the given `indexer-id` and `indexer-type` along with the provided
///  `start-block-height` and then will fetch the `last_processed_block_height` to continue the stream
#[derive(ArgEnum, Debug, Clone)]
pub enum StartMode {
    FromLatest,
    FromInterruption,
}

/// Helper function to perform an update in `__meta` table for the given `indexer-id`
/// with the given `last_processed_block_height`
/// Will throw an error in cases:
/// - database error
/// - conversion u64 to [bigdecimal::BigDecimal] error
pub async fn update_meta(
    db_with_meta_data_pool: &sqlx::Pool<sqlx::Postgres>,
    indexer_id: &str,
    last_processed_block_height: u64,
    tracing_target: &str,
) -> anyhow::Result<()> {
    let block_height: bigdecimal::BigDecimal =
        match bigdecimal::BigDecimal::from_u64(last_processed_block_height) {
            Some(value) => value,
            None => anyhow::bail!("Failed to parse u64 to BigDecimal"),
        };

    match sqlx::query!(
        r#"UPDATE __meta
           SET last_processed_block_height = $1
           WHERE indexer_id = $2 AND last_processed_block_height < $1
        "#,
        block_height,
        indexer_id,
    )
    .execute(db_with_meta_data_pool)
    .await
    {
        Ok(_) => Ok(()),
        Err(err) => {
            tracing::warn!(
                tracing_target,
                "Failed to update meta for INDEXER ID {}\n{:#?}",
                indexer_id,
                err,
            );
            anyhow::bail!(err)
        }
    }
}

impl Opts {
    /// returns a [near_lake_framework::LakeConfig] object where AWS credentials are sourced from
    /// .env file first, and then from .aws/credentials if not found.
    /// https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credentials.html
    pub async fn to_lake_config(
        &self,
        db_with_meta_data_pool: &sqlx::Pool<sqlx::Postgres>,
    ) -> anyhow::Result<near_lake_framework::LakeConfig> {
        let config_builder = near_lake_framework::LakeConfigBuilder::default();

        let start_block_height = match self.start_mode {
            StartMode::FromLatest => {
                let start_block_height_from_rpc = fetch_latest_block_height_from_rpc(
                    self.rpc_url
                        .as_ref()
                        .expect("`rpc-url` must be provided for `--start-mode from-latest"),
                )
                .await?;
                register_indexer(
                    db_with_meta_data_pool,
                    &self.indexer_id,
                    &self.indexer_type,
                    start_block_height_from_rpc,
                    None,
                )
                .await?;
                start_block_height_from_rpc
            }
            StartMode::FromInterruption => {
                register_indexer(
                    db_with_meta_data_pool,
                    &self.indexer_id,
                    &self.indexer_type,
                    self.start_block_height
                        .expect("`start-block-height` must be provided to use `start-mode from-interruption`"),
                    None,
                ).await?;
                // Starting slightly before the interruption to be sure we haven't missed anything
                fetch_last_processed_block_height_from_db(&self.indexer_id, db_with_meta_data_pool)
                    .await?
                    .saturating_sub(100)
            }
        };

        Ok(match self.chain_id {
            ChainId::Mainnet => config_builder.mainnet(),
            ChainId::Testnet => config_builder.testnet(),
        }
        .start_block_height(start_block_height)
        .build()?)
    }
}

/// Internal function to perform a registration of the indexer with the given `indexer-id` and `indexer-type`
/// in the `__meta` table of the provided database.
/// Will call [apply_migration] function in the beginning.
async fn register_indexer(
    db_with_meta_data_pool: &sqlx::Pool<sqlx::Postgres>,
    indexer_id: &str,
    indexer_type: &str,
    start_block_height: u64,
    end_block_height: Option<u64>,
) -> anyhow::Result<()> {
    apply_migration(db_with_meta_data_pool).await?;
    let block_height: bigdecimal::BigDecimal =
        match bigdecimal::BigDecimal::from_u64(start_block_height) {
            Some(value) => value,
            None => anyhow::bail!("Failed to parse u64 to BigDecimal"),
        };
    let end_block_height = if let Some(end_block_height) = end_block_height {
        bigdecimal::BigDecimal::from_u64(end_block_height)
    } else {
        None
    };

    sqlx::query!(
        r#"
INSERT INTO __meta (indexer_id, indexer_type, indexer_started_at, last_processed_block_height, start_block_height, end_block_height)
VALUES ($1, $2, now(), $3, $3, $4)
ON CONFLICT (indexer_id) DO UPDATE
    SET start_block_height = EXCLUDED.start_block_height,
        end_block_height = EXCLUDED.end_block_height
    WHERE __meta.indexer_id = EXCLUDED.indexer_id
        "#,
        indexer_id,
        indexer_type,
        block_height,
        end_block_height,
    )
    .execute(db_with_meta_data_pool)
    .await?;
    Ok(())
}

/// Internal function to fetch the latest from a given `rpc-url`.
/// Returns final block in the chain or throws an error.
async fn fetch_latest_block_height_from_rpc(rpc_url: &str) -> anyhow::Result<u64> {
    let client = JsonRpcClient::connect(rpc_url);
    let request = methods::block::RpcBlockRequest {
        block_reference: BlockReference::Finality(Finality::Final),
    };

    let latest_block = client.call(request).await?;

    Ok(latest_block.header.height)
}

/// Internal function to fetch the `last_processed_block_height` stores in `__meta` table
/// for the given `indexer-id`.
/// Returns a block height [u64] or an error
async fn fetch_last_processed_block_height_from_db(
    indexer_id: &str,
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    let record = sqlx::query!(
        r#"
SELECT last_processed_block_height FROM __meta WHERE indexer_id = $1
        "#,
        indexer_id,
    )
    .fetch_one(pool)
    .await?;

    record
        .last_processed_block_height
        .to_u64()
        .ok_or_else(|| anyhow::anyhow!("Failed to convert `last_processed_block_height` to u64"))
}

/// Internal function to create `__meta` table in the given database.
/// Uses `IF NOT EXISTS` to prevent redundant error.
/// The schema of the `__meta` table is located in the function body.
async fn apply_migration(
    db_with_meta_data_pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<()> {
    sqlx::query!(
        r#"
CREATE TABLE IF NOT EXISTS __meta (
    indexer_id                  text            PRIMARY KEY,
    indexer_type                text            NOT NULL,
    indexer_started_at          timestamptz     NOT NULL,
    last_processed_block_height numeric(20, 0)  NOT NULL,
    start_block_height          numeric(20, 0)  NOT NULL,
    end_block_height            numeric(20,0 )
)
        "#
    )
    .execute(db_with_meta_data_pool)
    .await?;
    Ok(())
}
