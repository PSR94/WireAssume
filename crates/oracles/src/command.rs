use crate::{Oracle, OracleError, OracleResult, OracleStatus};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::{process::Command, time::timeout};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOracleSpec {
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
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

pub struct CommandOracle {
    spec: CommandOracleSpec,
}

impl CommandOracle {
    pub fn new(spec: CommandOracleSpec) -> Result<Self, OracleError> {
        if spec.argv.is_empty() {
            return Err(OracleError::Invalid(
                "argv must contain an executable".into(),
            ));
        }
        if spec.timeout_ms == 0 {
            return Err(OracleError::Invalid(
                "timeout_ms must be greater than zero".into(),
            ));
        }
        Ok(Self { spec })
    }
}

#[async_trait]
impl Oracle for CommandOracle {
    fn kind(&self) -> &'static str {
        "command"
    }

    async fn evaluate(&self) -> Result<OracleResult, OracleError> {
        let start = Instant::now();
        let mut command = Command::new(&self.spec.argv[0]);
        command.args(&self.spec.argv[1..]);
        command.kill_on_drop(true);
        if let Some(cwd) = &self.spec.cwd {
            command.current_dir(cwd);
        }
        if !self.spec.inherit_env {
            command.env_clear();
        }
        command.envs(&self.spec.env);

        let output = timeout(
            Duration::from_millis(self.spec.timeout_ms),
            command.output(),
        )
        .await
        .map_err(|_| {
            OracleError::Execution(format!(
                "command exceeded {} ms timeout",
                self.spec.timeout_ms
            ))
        })?
        .map_err(|error| OracleError::Execution(format!("failed to spawn command: {error}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let exit_code = output.status.code();

        let mut failures = Vec::new();
        match exit_code {
            Some(code) if self.spec.success_exit_codes.contains(&code) => {}
            Some(code) => failures.push(format!("exit code {code} was not accepted")),
            None => failures.push("process terminated without an exit code".into()),
        }
        for expected in &self.spec.stdout_contains {
            if !stdout.contains(expected) {
                failures.push(format!("stdout did not contain {expected:?}"));
            }
        }
        for forbidden in &self.spec.stderr_not_contains {
            if stderr.contains(forbidden) {
                failures.push(format!("stderr contained forbidden text {forbidden:?}"));
            }
        }

        let status = if failures.is_empty() {
            OracleStatus::Pass
        } else {
            OracleStatus::Fail
        };
        let summary = if failures.is_empty() {
            format!(
                "command oracle passed with exit code {}",
                exit_code.unwrap_or_default()
            )
        } else {
            failures.join("; ")
        };
        Ok(OracleResult {
            status,
            duration_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            summary,
            stdout,
            stderr,
        })
    }
}

fn default_exit_codes() -> BTreeSet<i32> {
    [0].into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn command_oracle_observes_real_exit_and_stdout() {
        let oracle = CommandOracle::new(CommandOracleSpec {
            argv: vec!["/bin/sh".into(), "-c".into(), "printf healthy".into()],
            cwd: None,
            timeout_ms: 2_000,
            env: BTreeMap::new(),
            inherit_env: false,
            stdout_contains: vec!["healthy".into()],
            stderr_not_contains: vec![],
            success_exit_codes: [0].into_iter().collect(),
        })
        .unwrap();
        let result = oracle.evaluate().await.unwrap();
        assert_eq!(result.status, OracleStatus::Pass);
    }
}
