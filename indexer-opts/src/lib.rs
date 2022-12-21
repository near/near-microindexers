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

#[derive(ArgEnum, Debug, Clone, PartialEq, Eq)]
pub enum ChainId {
    Mainnet,
    Testnet,
}

#[derive(ArgEnum, Debug, Clone)]
pub enum StartMode {
    FromLatest,
    FromInterruption,
}

#[derive(Debug)]
pub enum MetaAction {
    RegisterIndexer {
        indexer_id: String,
        indexer_type: String,
        start_block_height: u64,
        end_block_height: Option<u64>,
    },
    UpdateMeta {
        indexer_id: String,
        last_processed_block_height: u64,
    },
}

pub async fn update_meta(
    db_with_meta_data_pool: &sqlx::Pool<sqlx::Postgres>,
    action: MetaAction,
) -> anyhow::Result<()> {
    match action {
        MetaAction::UpdateMeta {
            indexer_id,
            last_processed_block_height,
        } => {
            let block_height: bigdecimal::BigDecimal =
                match bigdecimal::BigDecimal::from_u64(last_processed_block_height) {
                    Some(value) => value,
                    None => anyhow::bail!("Failed to parse u64 to BigDecimal"),
                };
            match sqlx::query!(
                r#"
UPDATE __meta SET last_processed_block_height = $1 WHERE indexer_id = $2
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
                        "Failed to update meta for INDEXER ID {}\n{:#?}",
                        indexer_id,
                        err,
                    );
                    anyhow::bail!(err)
                }
            }
        }
        MetaAction::RegisterIndexer {
            indexer_id,
            indexer_type,
            start_block_height,
            end_block_height,
        } => {
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
    }
}

impl Opts {
    // returns a Lake Config object where AWS credentials are sourced from .env file first, and then from .aws/credentials if not found.
    // https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credentials.html
    pub async fn to_lake_config(
        &'static self,
        db_with_meta_data_pool: &sqlx::Pool<sqlx::Postgres>,
    ) -> anyhow::Result<near_lake_framework::LakeConfig> {
        let config_builder = near_lake_framework::LakeConfigBuilder::default();

        tracing::info!("CHAIN_ID: {:?}", self.chain_id);

        let start_block_height = match self.start_mode {
            StartMode::FromLatest => {
                fetch_latest_block_height_from_rpc(
                    self.rpc_url
                        .as_ref()
                        .expect("`rpc-url` must be provided for `--start-mode from-lastest"),
                )
                .await?
            }
            StartMode::FromInterruption => {
                fetch_last_processed_block_height_from_db(&self.indexer_id, db_with_meta_data_pool)
                    .await?
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

async fn fetch_latest_block_height_from_rpc(rpc_url: &str) -> anyhow::Result<u64> {
    let client = JsonRpcClient::connect(rpc_url);
    let request = methods::block::RpcBlockRequest {
        block_reference: BlockReference::Finality(Finality::Final),
    };

    let latest_block = client.call(request).await?;

    Ok(latest_block.header.height)
}

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
