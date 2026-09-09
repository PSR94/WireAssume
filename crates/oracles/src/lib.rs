//! Consumer workflow oracles.

mod command;
mod http;

pub use command::{CommandOracle, CommandOracleSpec};
pub use http::{HttpOracle, HttpOracleSpec};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OracleStatus {
    Pass,
    Fail,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleResult {
    pub status: OracleStatus,
    pub duration_ms: u64,
    pub summary: String,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}

#[derive(Debug, Error)]
pub enum OracleError {
    #[error("oracle configuration is invalid: {0}")]
    Invalid(String),
    #[error("oracle execution failed: {0}")]
    Execution(String),
}

#[async_trait]
pub trait Oracle: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn evaluate(&self) -> Result<OracleResult, OracleError>;
}
