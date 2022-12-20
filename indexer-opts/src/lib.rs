use std::str::FromStr;

use bigdecimal::{FromPrimitive, ToPrimitive};
pub use clap::{self, Parser};
use tracing_subscriber::EnvFilter;

use near_jsonrpc_client::{methods, JsonRpcClient};
use near_lake_framework::near_indexer_primitives::types::{BlockReference, Finality};

pub const LOGGING_PREFIX: &str = "indexer_opts";

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
    /// Sets the micro-indexer instance ID (for reading/writing indexer meta-data)
    #[clap(long)]
    pub debug: bool,
    /// Enabled Indexer for Explorer debug level of logs
    #[clap(long, env)]
    pub indexer_id: String,
    /// Block height to start the stream from
    #[clap(long, short, env)]
    pub start_block_height: u64,
    #[clap(long, short, env)]
    pub near_archival_rpc_url: String,
    // Chain ID: testnet or mainnet, used for NEAR Lake initialization
    #[clap(long, env)]
    pub chain_id: ChainId,
    /// Port to enable metrics/health service
    #[clap(long, short, env, default_value_t = 3000)]
    pub port: u16,
    /// Start mode for instance
    #[clap(long, short, env, default_value = "from-interruption")]
    pub start_mode: StartMode,
    /// Database URL
    #[clap(long, short, env)]
    pub database_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainId {
    Mainnet,
    Testnet,
}

#[derive(Debug, Clone)]
pub enum StartMode {
    FromLatest,
    FromInterruption,
}

#[derive(Debug)]
pub enum MetaAction {
    RegisterIndexer {
        indexer_id: String,
        start_block_height: u64,
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
UPDATE meta SET last_processed_block_height = $1 WHERE indexer_id = $2
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
                        target: LOGGING_PREFIX,
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
            start_block_height,
        } => {
            let block_height: bigdecimal::BigDecimal =
                match bigdecimal::BigDecimal::from_u64(start_block_height) {
                    Some(value) => value,
                    None => anyhow::bail!("Failed to parse u64 to BigDecimal"),
                };
            if (sqlx::query!(
                r#"
INSERT INTO meta (indexer_id, last_processed_block_height, start_block_height)
VALUES ($1, $2, $2)
                "#,
                indexer_id,
                block_height,
            )
            .execute(db_with_meta_data_pool)
            .await)
                .is_err()
            {
                match sqlx::query!(
                    r#"
UPDATE meta SET start_block_height = $1, last_processed_block_height = $1 WHERE indexer_id = $2
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
                            target: LOGGING_PREFIX,
                            "Failed to update meta for INDEXER ID {}\n{:#?}",
                            indexer_id,
                            err,
                        );
                        anyhow::bail!(err)
                    }
                }
            } else {
                Ok(())
            }
        }
    }
}

pub fn init_tracing(debug: bool) -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    let mut env_filter = EnvFilter::new("indexer_events=info");

    if debug {
        env_filter = env_filter.add_directive("near_lake_framework=debug".parse()?);
    }

    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        if !rust_log.is_empty() {
            for directive in rust_log.split(',').filter_map(|s| match s.parse() {
                Ok(directive) => Some(directive),
                Err(err) => {
                    tracing::warn!(
                        target: LOGGING_PREFIX,
                        "Ignoring directive `{}`: {}",
                        s,
                        err
                    );
                    None
                }
            }) {
                env_filter = env_filter.add_directive(directive);
            }
        }
    }

    let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stdout());

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_writer(non_blocking)
        .with_env_filter(env_filter);

    if std::env::var("ENABLE_JSON_LOGS").is_ok() {
        subscriber.json().init();
    } else {
        subscriber.compact().init();
    }

    Ok(guard)
}

impl Opts {
    // returns a Lake Config object where AWS credentials are sourced from .env file first, and then from .aws/credentials if not found.
    // https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credentials.html
    pub async fn to_lake_config(
        &self,
        db_with_meta_data_pool: &sqlx::Pool<sqlx::Postgres>,
    ) -> anyhow::Result<near_lake_framework::LakeConfig> {
        let config_builder = near_lake_framework::LakeConfigBuilder::default();

        tracing::info!(target: LOGGING_PREFIX, "CHAIN_ID: {:?}", self.chain_id);

        let start_block_height = match self.start_mode {
            StartMode::FromLatest => final_block_height(&self.near_archival_rpc_url).await,
            StartMode::FromInterruption => {
                match last_processed_block_height(&self.indexer_id, db_with_meta_data_pool).await {
                    Ok(last_processed_block_height) => last_processed_block_height,
                    Err(err) => {
                        tracing::warn!(
                            target: LOGGING_PREFIX,
                            "Failed to fetch `last_processed_block_height` from meta data. Falling back to provided `start_block_height`\n{:#?}",
                            err,
                        );
                        self.start_block_height
                    }
                }
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

impl FromStr for ChainId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(Self::Mainnet),
            "testnet" => Ok(Self::Testnet),
            _ => Err(format!(
                "Invalid CHAIN_ID: `{}`. Try `mainnet` or `testnet`",
                s
            )),
        }
    }
}

impl FromStr for StartMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "from-latest" => Ok(Self::FromLatest),
            "from-interruption" => Ok(Self::FromInterruption),
            _ => Err(format!(
                "Invalid START_MODE: `{}`. Try `from-latest` or `from-interruption`",
                s
            )),
        }
    }
}

async fn final_block_height(rpc_url: &str) -> u64 {
    let client = JsonRpcClient::connect(rpc_url);
    let request = methods::block::RpcBlockRequest {
        block_reference: BlockReference::Finality(Finality::Final),
    };

    let latest_block = client
        .call(request)
        .await
        .unwrap_or_else(|_| panic!("Failed to fetch final block from RPC {}", rpc_url));

    latest_block.header.height
}

async fn last_processed_block_height(
    indexer_id: &str,
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<u64> {
    let record = sqlx::query!(
        r#"
SELECT last_processed_block_height FROM meta WHERE indexer_id = $1 LIMIT 1
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
