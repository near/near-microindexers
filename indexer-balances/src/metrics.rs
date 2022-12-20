use actix_web::{get, App, HttpServer, Responder};
use prometheus::{Encoder, IntCounter, IntGauge, Opts};

use crate::LOGGING_PREFIX;

type Result<T, E> = std::result::Result<T, E>;

fn try_create_int_counter(name: &str, help: &str) -> Result<IntCounter, prometheus::Error> {
    let opts = Opts::new(name, help);
    let counter = IntCounter::with_opts(opts)?;
    prometheus::register(Box::new(counter.clone()))?;
    Ok(counter)
}

fn try_create_int_gauge(name: &str, help: &str) -> Result<IntGauge, prometheus::Error> {
    let opts = Opts::new(name, help);
    let gauge = IntGauge::with_opts(opts)?;
    prometheus::register(Box::new(gauge.clone()))?;
    Ok(gauge)
}

lazy_static! {
    pub(crate) static ref BLOCK_PROCESSED_TOTAL: IntCounter = try_create_int_counter(
        "indexer_balances_total_blocks_processed",
        "Total number of blocks processed by indexer regardless of restarts. Used to calculate Block Processing Rate(BPS)"
    )
    .unwrap();
    pub(crate) static ref LATEST_BLOCK_HEIGHT: IntGauge = try_create_int_gauge(
        "indexer_balances_latest_block_height",
        "Last seen block height by indexer"
    )
    .unwrap();
}

#[get("/metrics")]
async fn get_metrics() -> impl Responder {
    let encoder = prometheus::TextEncoder::new();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&prometheus::gather(), &mut buffer) {
        tracing::error!(target: LOGGING_PREFIX, "could not encode metrics: {}", e);
    };

    match String::from_utf8(buffer.clone()) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(
                target: LOGGING_PREFIX,
                "custom metrics could not be from_utf8'd: {}",
                e
            );
            String::default()
        }
    }
}

pub(crate) async fn init_metrics_server(port: u16) -> anyhow::Result<()> {
    tracing::info!(
        target: LOGGING_PREFIX,
        "Starting metrics server on http://0.0.0.0:{port}/metrics"
    );

    HttpServer::new(|| App::new().service(get_metrics))
        .bind(("0.0.0.0", port))?
        .run()
        .await
        .map_err(|e| anyhow::anyhow!("Error while executing HTTP Server: {}", e))
}
