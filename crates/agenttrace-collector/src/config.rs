//! Collector configuration, read from environment variables with safe defaults.
//!
//! Defaults are chosen so an out-of-the-box run is safe: bind to loopback only,
//! raw payload capture OFF, a bounded request size, and 30-day retention.

use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    /// Address to bind. Defaults to 127.0.0.1 (loopback only) - never 0.0.0.0.
    pub bind_addr: SocketAddr,
    /// SQLite database path.
    pub db_path: PathBuf,
    /// Whether to keep raw payload attributes (prompts, tool args). Default OFF.
    pub capture_payloads: bool,
    /// Max accepted request body size in bytes.
    pub max_body_bytes: usize,
    /// Delete spans older than this many days (0 disables retention).
    pub retention_days: u64,
    /// Directory of the built viewer to serve, if present.
    pub viewer_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            bind_addr: "127.0.0.1:4357".parse().unwrap(),
            db_path: PathBuf::from("agenttrace.db"),
            capture_payloads: false,
            max_body_bytes: 4 * 1024 * 1024, // 4 MiB
            retention_days: 30,
            viewer_dir: PathBuf::from("viewer/dist"),
        }
    }
}

impl Config {
    /// Build config from `AGENTTRACE_*` environment variables, falling back to
    /// defaults. Binding to a non-loopback host requires an explicit opt-in.
    pub fn from_env() -> Self {
        let mut cfg = Config::default();

        if let Ok(v) = std::env::var("AGENTTRACE_DB") {
            cfg.db_path = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("AGENTTRACE_VIEWER_DIR") {
            cfg.viewer_dir = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("AGENTTRACE_CAPTURE_PAYLOADS") {
            cfg.capture_payloads = matches!(v.to_lowercase().as_str(), "1" | "true" | "yes");
        }
        if let Ok(v) = std::env::var("AGENTTRACE_MAX_BODY_BYTES") {
            if let Ok(n) = v.parse() {
                cfg.max_body_bytes = n;
            }
        }
        if let Ok(v) = std::env::var("AGENTTRACE_RETENTION_DAYS") {
            if let Ok(n) = v.parse() {
                cfg.retention_days = n;
            }
        }

        // Host/port. Default host is loopback; binding elsewhere is an explicit,
        // logged choice (the collector is not meant to be publicly exposed).
        let host = std::env::var("AGENTTRACE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = std::env::var("AGENTTRACE_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(4357u16);
        if let Ok(addr) = format!("{host}:{port}").parse() {
            cfg.bind_addr = addr;
        }

        cfg
    }

    pub fn is_loopback(&self) -> bool {
        self.bind_addr.ip().is_loopback()
    }
}
