//! AgentTrace collector binary.
//!
//! Reads configuration from the environment, opens SQLite storage, starts a
//! retention sweeper, and serves the ingest/query API plus the viewer.

use std::sync::Arc;
use std::time::Duration;

use agenttrace::RedactionPolicy;
use agenttrace_collector::config::Config;
use agenttrace_collector::routes::{app, AppState};
use agenttrace_collector::storage::Storage;
use anyhow::Result;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=warn".into()),
        )
        .init();

    let config = Config::from_env();

    if !config.is_loopback() {
        warn!(
            addr = %config.bind_addr,
            "collector is binding to a NON-loopback address; it is not hardened for public exposure"
        );
    }

    // Redaction policy reflects the payload-capture setting; storage enforces it.
    let policy = if config.capture_payloads {
        warn!("AGENTTRACE_CAPTURE_PAYLOADS is enabled: raw prompts/tool args will be stored (secrets are still masked)");
        RedactionPolicy::allow_payloads()
    } else {
        RedactionPolicy::strict()
    };

    let storage = Storage::open(&config.db_path, policy)?;
    info!(db = %config.db_path.display(), "opened span database");

    // Retention: prune on startup, then hourly.
    match storage.prune(config.retention_days) {
        Ok(n) if n > 0 => info!(removed = n, "pruned spans past retention window"),
        Ok(_) => {}
        Err(e) => warn!(error = %e, "retention prune failed"),
    }
    {
        let storage = storage.clone();
        let retention_days = config.retention_days;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(3600));
            ticker.tick().await; // first tick is immediate; skip (already pruned)
            loop {
                ticker.tick().await;
                if let Ok(n) = storage.prune(retention_days) {
                    if n > 0 {
                        info!(removed = n, "pruned spans past retention window");
                    }
                }
            }
        });
    }

    let state = AppState {
        storage,
        config: Arc::new(config.clone()),
    };
    let router = app(state);

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    let addr = listener.local_addr()?;
    info!(%addr, "AgentTrace collector listening");
    info!("ingest: POST http://{addr}/v1/spans   |   viewer/API: http://{addr}/");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
