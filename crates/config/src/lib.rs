//! `.wireassume.yml` parsing and validation.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    net::{IpAddr, SocketAddr},
    path::Path,
};
use thiserror::Error;
use url::Url;
use wireassume_model::RedactionPolicy;
use wireassume_mutation_engine::MutationKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireAssumeConfig {
    pub project: ProjectConfig,
    pub provider: ProviderConfig,
    #[serde(default)]
    pub proxy: ProxyConfig,
    pub scenarios: Vec<ScenarioConfig>,
    #[serde(default)]
    pub redaction: RedactionPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    #[serde(default)]
    pub openapi: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    #[serde(default = "default_listen")]
    pub listen: SocketAddr,
    #[serde(default)]
    pub upstream: Option<Url>,
    #[serde(default = "default_max_payload")]
    pub max_payload_bytes: usize,
    #[serde(default)]
    pub allow_non_loopback: bool,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            upstream: None,
            max_payload_bytes: default_max_payload(),
            allow_non_loopback: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioConfig {
    pub name: String,
    pub traffic: TrafficConfig,
    pub oracle: OracleConfig,
    #[serde(default)]
    pub mutation: MutationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficConfig {
    pub endpoint: EndpointMatcher,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointMatcher {
    pub method: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum OracleConfig {
    Command(CommandOracleConfig),
    Http(HttpOracleConfig),
    Playwright(PlaywrightOracleConfig),
    Custom(CommandOracleConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOracleConfig {
    pub argv: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub inherit_env: bool,
    #[serde(default)]
    pub stdout_contains: Vec<String>,
    #[serde(default)]
    pub stderr_not_contains: Vec<String>,
    #[serde(default = "default_exit_codes")]
    pub success_exit_codes: BTreeSet<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpOracleConfig {
    #[serde(default = "default_get")]
    pub method: String,
    pub url: Url,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_http_statuses")]
    pub success_statuses: BTreeSet<u16>,
    #[serde(default)]
    pub body_contains: Vec<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaywrightOracleConfig {
    /// A Playwright test file or project command is executed through the same safe argv runner.
    pub argv: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationConfig {
    #[serde(default = "default_budget")]
    pub budget: usize,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default)]
    pub include: BTreeSet<MutationKind>,
    #[serde(default)]
    pub exclude: BTreeSet<MutationKind>,
}

impl Default for MutationConfig {
    fn default() -> Self {
        Self {
            budget: default_budget(),
            concurrency: default_concurrency(),
            seed: default_seed(),
            include: BTreeSet::new(),
            exclude: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read configuration: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse YAML configuration: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("configuration is invalid:\n{0}")]
    Invalid(String),
}

impl WireAssumeConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&text)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut errors = Vec::new();
        if self.project.name.trim().is_empty() {
            errors.push("project.name must not be empty".to_string());
        }
        if self.provider.name.trim().is_empty() {
            errors.push("provider.name must not be empty".to_string());
        }
        if self.scenarios.is_empty() {
            errors.push("at least one scenario is required".to_string());
        }
        if !self.proxy.allow_non_loopback && !is_loopback(self.proxy.listen.ip()) {
            errors.push(format!(
                "proxy.listen={} is not loopback; set proxy.allow_non_loopback=true only in a trusted environment",
                self.proxy.listen
            ));
        }
        if self.proxy.max_payload_bytes == 0 {
            errors.push("proxy.max_payload_bytes must be greater than zero".to_string());
        }

        let mut names = BTreeSet::new();
        for scenario in &self.scenarios {
            if scenario.name.trim().is_empty() {
                errors.push("scenario.name must not be empty".to_string());
            } else if !names.insert(scenario.name.as_str()) {
                errors.push(format!("scenario name {:?} is duplicated", scenario.name));
            }
            if scenario.mutation.budget == 0 {
                errors.push(format!(
                    "scenario {:?} mutation.budget must be greater than zero",
                    scenario.name
                ));
            }
            if scenario.mutation.concurrency == 0 {
                errors.push(format!(
                    "scenario {:?} mutation.concurrency must be greater than zero",
                    scenario.name
                ));
            }
            if scenario.traffic.endpoint.path.trim().is_empty() {
                errors.push(format!(
                    "scenario {:?} traffic.endpoint.path must not be empty",
                    scenario.name
                ));
            }
            validate_oracle(&scenario.name, &scenario.oracle, &mut errors);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Invalid(errors.join("\n")))
        }
    }
}

fn validate_oracle(name: &str, oracle: &OracleConfig, errors: &mut Vec<String>) {
    match oracle {
        OracleConfig::Command(config) | OracleConfig::Custom(config) => {
            if config.argv.is_empty() {
                errors.push(format!(
                    "scenario {name:?} command/custom oracle argv must not be empty"
                ));
            }
            if config.timeout_ms == 0 {
                errors.push(format!(
                    "scenario {name:?} oracle timeout_ms must be greater than zero"
                ));
            }
        }
        OracleConfig::Playwright(config) => {
            if config.argv.is_empty() {
                errors.push(format!(
                    "scenario {name:?} playwright oracle argv must not be empty"
                ));
            }
            if config.timeout_ms == 0 {
                errors.push(format!(
                    "scenario {name:?} oracle timeout_ms must be greater than zero"
                ));
            }
        }
        OracleConfig::Http(config) => {
            if config.timeout_ms == 0 {
                errors.push(format!(
                    "scenario {name:?} oracle timeout_ms must be greater than zero"
                ));
            }
            if config.success_statuses.is_empty() {
                errors.push(format!(
                    "scenario {name:?} HTTP oracle success_statuses must not be empty"
                ));
            }
        }
    }
}

fn is_loopback(ip: IpAddr) -> bool {
    ip.is_loopback()
}

fn default_listen() -> SocketAddr {
    "127.0.0.1:9090".parse().unwrap()
}
fn default_max_payload() -> usize {
    2 * 1024 * 1024
}
fn default_timeout() -> u64 {
    30_000
}
fn default_budget() -> usize {
    500
}
fn default_concurrency() -> usize {
    4
}
fn default_seed() -> u64 {
    42
}
fn default_get() -> String {
    "GET".into()
}
fn default_exit_codes() -> BTreeSet<i32> {
    [0].into_iter().collect()
}
fn default_http_statuses() -> BTreeSet<u16> {
    (200..300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reference_configuration() {
        let yaml = r#"
project:
  name: orbitdesk
provider:
  name: peoplecrm
proxy:
  listen: 127.0.0.1:9090
  upstream: https://api.example.test
scenarios:
  - name: customer-profile
    traffic:
      endpoint:
        method: GET
        path: /customers/*
    oracle:
      type: http
      url: http://127.0.0.1:3000/health/customer/123
    mutation:
      budget: 200
      include: [remove-field, null-field, reverse-array]
redaction:
  headers: [authorization, cookie]
  jsonpaths: [$.token]
"#;
        let config: WireAssumeConfig = serde_yaml::from_str(yaml).unwrap();
        config.validate().unwrap();
        assert_eq!(config.scenarios[0].mutation.seed, 42);
    }

    #[test]
    fn non_loopback_bind_requires_explicit_opt_in() {
        let yaml = r#"
project: { name: demo }
provider: { name: provider }
proxy: { listen: 0.0.0.0:9090 }
scenarios:
  - name: demo
    traffic: { endpoint: { method: GET, path: / } }
    oracle: { type: http, url: http://127.0.0.1:3000/ }
"#;
        let config: WireAssumeConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }
}
