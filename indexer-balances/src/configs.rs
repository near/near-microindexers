use clap::Parser;
use tracing_subscriber::EnvFilter;

/// NEAR Indexer for Explorer
/// Watches for stream of blocks from the chain
#[derive(Parser, Debug)]
#[clap(
    version,
    author,
    about,
    disable_help_subcommand(true),
    propagate_version(true),
    next_line_help(true)
)]
pub(crate) struct Opts {
    /// Enabled Indexer for Explorer debug level of logs
    #[clap(long, env)]
    pub debug: bool,
    /// Block height to start the stream from. If None, start from interruption
    #[clap(long, short, env)]
    pub start_block_height: Option<u64>,
    #[clap(long, short, env)]
    pub near_archival_rpc_url: String,
    // Chain ID: testnet or mainnet, used for NEAR Lake initialization
    #[clap(long, env)]
    pub chain_id: String,
    /// Port to enable metrics service
    #[clap(long, short, env, default_value_t = 3000)]
    pub port: u16,
}

impl Opts {
    // returns a Lake Config object where AWS credentials are sourced from .env file first, and then from .aws/credentials if not found.
    // https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credentials.html
    pub async fn to_lake_config(&self, start_block_height: u64) -> near_lake_framework::LakeConfig {
        let config_builder = near_lake_framework::LakeConfigBuilder::default();

        tracing::info!(target: crate::LOGGING_PREFIX, "CHAIN_ID: {}", self.chain_id);

        match self.chain_id.as_str() {
            "mainnet" => config_builder.mainnet(),
            "testnet" => config_builder.testnet(),
            invalid_chain => panic!(
                "Invalid CHAIN_ID: `{}`. Try `mainnet` or `testnet`",
                invalid_chain
            ),
        }
        .start_block_height(start_block_height)
        .build()
        .expect("Failed to build LakeConfig")
    }
}

pub(crate) fn init_tracing(
    debug: bool,
) -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    let mut env_filter = EnvFilter::new("indexer_balances=info");

    if debug {
        env_filter = env_filter.add_directive("near_lake_framework=debug".parse()?);
    }

    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        if !rust_log.is_empty() {
            for directive in rust_log.split(',').filter_map(|s| match s.parse() {
                Ok(directive) => Some(directive),
                Err(err) => {
                    tracing::warn!(
                        target: crate::LOGGING_PREFIX,
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
