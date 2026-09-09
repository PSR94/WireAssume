use crate::{Oracle, OracleError, OracleResult, OracleStatus};
use async_trait::async_trait;
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, time::{Duration, Instant}};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpOracleSpec {
    pub method: String,
    pub url: Url,
    pub timeout_ms: u64,
    #[serde(default = "default_statuses")]
    pub success_statuses: BTreeSet<u16>,
    #[serde(default)]
    pub body_contains: Vec<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

pub struct HttpOracle {
    spec: HttpOracleSpec,
    client: Client,
}

impl HttpOracle {
    pub fn new(spec: HttpOracleSpec) -> Result<Self, OracleError> {
        if spec.timeout_ms == 0 {
            return Err(OracleError::Invalid("timeout_ms must be greater than zero".into()));
        }
        if spec.success_statuses.is_empty() {
            return Err(OracleError::Invalid("success_statuses must not be empty".into()));
        }
        let client = Client::builder()
            .timeout(Duration::from_millis(spec.timeout_ms))
            .build()
            .map_err(|error| OracleError::Invalid(error.to_string()))?;
        Ok(Self { spec, client })
    }
}

#[async_trait]
impl Oracle for HttpOracle {
    fn kind(&self) -> &'static str { "http" }

    async fn evaluate(&self) -> Result<OracleResult, OracleError> {
        let start = Instant::now();
        let method = Method::from_bytes(self.spec.method.as_bytes())
            .map_err(|error| OracleError::Invalid(format!("invalid HTTP method: {error}")))?;
        let mut request = self.client.request(method, self.spec.url.clone());
        for (name, value) in &self.spec.headers {
            request = request.header(name, value);
        }
        let response = request.send().await
            .map_err(|error| OracleError::Execution(error.to_string()))?;
        let status_code = response.status().as_u16();
        let body = response.text().await
            .map_err(|error| OracleError::Execution(format!("failed to read HTTP oracle body: {error}")))?;

        let mut failures = Vec::new();
        if !self.spec.success_statuses.contains(&status_code) {
            failures.push(format!("HTTP status {status_code} was not accepted"));
        }
        for expected in &self.spec.body_contains {
            if !body.contains(expected) {
                failures.push(format!("response body did not contain {expected:?}"));
            }
        }
        let status = if failures.is_empty() { OracleStatus::Pass } else { OracleStatus::Fail };
        Ok(OracleResult {
            status,
            duration_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            summary: if failures.is_empty() { format!("HTTP oracle passed with status {status_code}") } else { failures.join("; ") },
            stdout: body,
            stderr: String::new(),
        })
    }
}

fn default_statuses() -> BTreeSet<u16> { (200..300).collect() }
