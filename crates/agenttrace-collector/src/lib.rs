//! AgentTrace collector library: configuration, SQLite storage and the HTTP API.
//! The `agenttrace-collector` binary is a thin wrapper around [`routes::app`].

pub mod config;
pub mod routes;
pub mod storage;

pub use config::Config;
pub use storage::Storage;
