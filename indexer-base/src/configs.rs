use clap::Parser;

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
    #[clap(long)]
    pub debug: bool,
    // todo fix wording
    /// Switches indexer to non-strict mode (skips Receipts without parent Transaction hash, puts such block_height into special table)
    #[clap(long)]
    pub non_strict_mode: bool,
    // todo
    /// Block height to start the stream from. If None, start from interruption
    #[clap(long, short)]
    pub start_block_height: u64,
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

        tracing::info!(target: crate::INDEXER, "CHAIN_ID: {}", self.chain_id);

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
