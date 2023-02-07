use tracing_subscriber::EnvFilter;

pub(crate) fn init_tracing(debug: bool) -> anyhow::Result<()> {
    let mut env_filter =
        EnvFilter::new("near_lake_framework=info,indexer_balances=info,indexer=info,stats=info");

    if debug {
        env_filter = env_filter
            .add_directive("near_lake_framework=debug".parse()?)
            .add_directive("indexer_balances=debug".parse()?);
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

    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr);

    if std::env::var("ENABLE_JSON_LOGS").is_ok() {
        subscriber.json().init();
    } else {
        subscriber.compact().init();
    }

    Ok(())
}
