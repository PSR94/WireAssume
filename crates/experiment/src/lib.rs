//! Evidence-producing mutation experiment runner.

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use thiserror::Error;
use url::Url;
use wireassume_model::{canonical_json, stable_id, Body, CorpusError, CorpusStore, Interaction};
use wireassume_mutation_engine::{
    ArrayMutator, JsonMutator, MutationKind, MutationPlanner, PlannerConfig, ProtocolMutator,
    ProtocolMutatorConfig,
};
use wireassume_oracles::{Oracle, OracleResult, OracleStatus};
use wireassume_proxy::{ExperimentReplayController, ResponseOverride};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentConfig {
    pub scenario_id: String,
    pub method: String,
    pub path_pattern: String,
    pub seed: u64,
    pub budget: usize,
    #[serde(default)]
    pub include: BTreeSet<MutationKind>,
    #[serde(default)]
    pub exclude: BTreeSet<MutationKind>,
    #[serde(default)]
    pub protocol: ProtocolMutatorConfig,
    #[serde(default)]
    pub source_revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrialMutation {
    pub id: String,
    pub kind: MutationKind,
    pub target: String,
    pub description: String,
    pub response: wireassume_model::ResponseRecord,
    pub delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExperimentCase {
    pub interaction_id: String,
    pub mutation: TrialMutation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrialOutcome {
    Pass,
    Fail,
    Inconclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialResult {
    pub evidence_id: String,
    pub interaction_id: String,
    pub mutation_id: String,
    pub kind: MutationKind,
    pub target: String,
    pub description: String,
    pub outcome: TrialOutcome,
    pub duration_ms: u64,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentReport {
    pub run_id: String,
    pub generated_at: String,
    pub scenario_id: String,
    pub source_revision: Option<String>,
    pub seed: u64,
    pub budget: usize,
    pub baseline: BaselineResult,
    pub planned_mutations: usize,
    pub executed_mutations: usize,
    pub passes: usize,
    pub failures: usize,
    pub inconclusive: usize,
    pub results: Vec<TrialResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineResult {
    pub duration_ms: u64,
    pub summary: String,
}

#[derive(Debug, Error)]
pub enum ExperimentError {
    #[error(transparent)]
    Corpus(#[from] CorpusError),
    #[error("experiment configuration is invalid: {0}")]
    Invalid(String),
    #[error("consumer baseline did not pass: {0}")]
    BaselineFailed(String),
    #[error("failed to persist experiment evidence: {0}")]
    Persist(#[from] std::io::Error),
    #[error("failed to serialize experiment evidence: {0}")]
    Serialize(#[from] serde_json::Error),
}

pub struct ExperimentRunner {
    workspace: PathBuf,
    controller: ExperimentReplayController,
    oracle: Arc<dyn Oracle>,
}

impl ExperimentRunner {
    pub fn new(
        workspace: impl Into<PathBuf>,
        controller: ExperimentReplayController,
        oracle: Arc<dyn Oracle>,
    ) -> Self {
        Self {
            workspace: workspace.into(),
            controller,
            oracle,
        }
    }

    pub fn plan(&self, config: &ExperimentConfig) -> Result<Vec<ExperimentCase>, ExperimentError> {
        validate_config(config)?;
        let store = CorpusStore::new(&self.workspace);
        let mut interactions = Vec::new();
        for id in store.ids()? {
            let interaction = store.load(&id)?;
            if interaction.metadata.scenario_id == config.scenario_id
                && interaction.request.method.eq_ignore_ascii_case(&config.method)
                && path_matches(&config.path_pattern, request_path(&interaction.request.uri))
            {
                interactions.push((id, interaction));
            }
        }
        interactions.sort_by(|a, b| a.0.cmp(&b.0));
        if interactions.is_empty() {
            return Err(ExperimentError::Invalid(format!(
                "no recorded interactions match scenario {:?}, method {}, path {:?}",
                config.scenario_id, config.method, config.path_pattern
            )));
        }

        let planner = MutationPlanner::new(vec![Box::new(JsonMutator), Box::new(ArrayMutator)]);
        let protocol = ProtocolMutator {
            config: config.protocol.clone(),
        };
        let mut cases = Vec::new();
        for (interaction_id, interaction) in interactions {
            if let Body::Json(body) = &interaction.response.body {
                let body_plan = planner.plan(
                    body,
                    &PlannerConfig {
                        budget: usize::MAX,
                        concurrency: 1,
                        seed: config.seed,
                        retry_limit: 0,
                        enabled: config.include.clone(),
                    },
                    &HashSet::new(),
                );
                for mutation in body_plan.selected {
                    if config.exclude.contains(&mutation.kind) {
                        continue;
                    }
                    let mut response = interaction.response.clone();
                    response.body = Body::Json(mutation.mutated);
                    cases.push(ExperimentCase {
                        interaction_id: interaction_id.clone(),
                        mutation: TrialMutation {
                            id: mutation.id,
                            kind: mutation.kind,
                            target: format!("response.body{}", mutation.path),
                            description: mutation.description,
                            response,
                            delay_ms: 0,
                        },
                    });
                }
            }

            for mutation in protocol.generate(&interaction.response, config.seed) {
                if (!config.include.is_empty() && !config.include.contains(&mutation.kind))
                    || config.exclude.contains(&mutation.kind)
                {
                    continue;
                }
                cases.push(ExperimentCase {
                    interaction_id: interaction_id.clone(),
                    mutation: TrialMutation {
                        id: mutation.id,
                        kind: mutation.kind,
                        target: mutation.target,
                        description: mutation.description,
                        response: mutation.response,
                        delay_ms: mutation.delay_ms,
                    },
                });
            }
        }

        cases.sort_by(|a, b| {
            a.interaction_id
                .cmp(&b.interaction_id)
                .then(a.mutation.target.cmp(&b.mutation.target))
                .then(a.mutation.kind.cmp(&b.mutation.kind))
                .then(a.mutation.id.cmp(&b.mutation.id))
        });
        cases.dedup_by(|a, b| {
            a.interaction_id == b.interaction_id && a.mutation.id == b.mutation.id
        });
        cases.truncate(config.budget);
        Ok(cases)
    }

    pub async fn run(&self, config: &ExperimentConfig) -> Result<ExperimentReport, ExperimentError> {
        let cases = self.plan(config)?;
        self.controller.reset().await;
        let baseline = self
            .oracle
            .evaluate()
            .await
            .map_err(|error| ExperimentError::BaselineFailed(error.to_string()))?;
        if baseline.status != OracleStatus::Pass {
            return Err(ExperimentError::BaselineFailed(baseline.summary));
        }

        let run_fingerprint = RunFingerprint {
            scenario_id: &config.scenario_id,
            source_revision: config.source_revision.as_deref(),
            seed: config.seed,
            mutation_ids: cases.iter().map(|case| case.mutation.id.as_str()).collect(),
        };
        let run_id = stable_id("run", &run_fingerprint)?;
        let mut results = Vec::with_capacity(cases.len());

        for case in &cases {
            self.controller
                .apply(ResponseOverride {
                    interaction_id: case.interaction_id.clone(),
                    response: case.mutation.response.clone(),
                    delay_ms: case.mutation.delay_ms,
                })
                .await;

            let evaluated = self.oracle.evaluate().await;
            self.controller.reset().await;
            let (outcome, duration_ms, summary) = trial_outcome(evaluated);
            let evidence_fingerprint = EvidenceFingerprint {
                run_id: &run_id,
                interaction_id: &case.interaction_id,
                mutation_id: &case.mutation.id,
                outcome,
            };
            let evidence_id = stable_id("ev", &evidence_fingerprint)?;
            results.push(TrialResult {
                evidence_id,
                interaction_id: case.interaction_id.clone(),
                mutation_id: case.mutation.id.clone(),
                kind: case.mutation.kind,
                target: case.mutation.target.clone(),
                description: case.mutation.description.clone(),
                outcome,
                duration_ms,
                summary,
            });
        }

        let passes = results
            .iter()
            .filter(|result| result.outcome == TrialOutcome::Pass)
            .count();
        let failures = results
            .iter()
            .filter(|result| result.outcome == TrialOutcome::Fail)
            .count();
        let inconclusive = results.len() - passes - failures;
        let report = ExperimentReport {
            run_id: run_id.clone(),
            generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            scenario_id: config.scenario_id.clone(),
            source_revision: config.source_revision.clone(),
            seed: config.seed,
            budget: config.budget,
            baseline: BaselineResult {
                duration_ms: baseline.duration_ms,
                summary: baseline.summary,
            },
            planned_mutations: cases.len(),
            executed_mutations: results.len(),
            passes,
            failures,
            inconclusive,
            results,
        };
        persist_report(&self.workspace, &report)?;
        Ok(report)
    }
}

#[derive(Serialize)]
struct RunFingerprint<'a> {
    scenario_id: &'a str,
    source_revision: Option<&'a str>,
    seed: u64,
    mutation_ids: Vec<&'a str>,
}

#[derive(Serialize)]
struct EvidenceFingerprint<'a> {
    run_id: &'a str,
    interaction_id: &'a str,
    mutation_id: &'a str,
    outcome: TrialOutcome,
}

fn trial_outcome(result: Result<OracleResult, wireassume_oracles::OracleError>) -> (TrialOutcome, u64, String) {
    match result {
        Ok(result) => {
            let outcome = match result.status {
                OracleStatus::Pass => TrialOutcome::Pass,
                OracleStatus::Fail => TrialOutcome::Fail,
                OracleStatus::Inconclusive => TrialOutcome::Inconclusive,
            };
            (outcome, result.duration_ms, result.summary)
        }
        Err(error) => (TrialOutcome::Inconclusive, 0, error.to_string()),
    }
}

fn persist_report(workspace: &Path, report: &ExperimentReport) -> Result<(), ExperimentError> {
    let directory = workspace.join("runs").join(&report.run_id);
    fs::create_dir_all(&directory)?;
    let path = directory.join("report.json");
    let tmp = directory.join("report.json.tmp");
    fs::write(&tmp, canonical_json(report)?)?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

fn validate_config(config: &ExperimentConfig) -> Result<(), ExperimentError> {
    if config.scenario_id.trim().is_empty() {
        return Err(ExperimentError::Invalid("scenario_id must not be empty".into()));
    }
    if config.method.trim().is_empty() {
        return Err(ExperimentError::Invalid("method must not be empty".into()));
    }
    if config.path_pattern.trim().is_empty() {
        return Err(ExperimentError::Invalid("path_pattern must not be empty".into()));
    }
    if config.budget == 0 {
        return Err(ExperimentError::Invalid("budget must be greater than zero".into()));
    }
    Ok(())
}

fn request_path(uri: &str) -> &str {
    if let Ok(url) = Url::parse(uri) {
        let path = url.path().to_string();
        return Box::leak(path.into_boxed_str());
    }
    uri.split('?').next().unwrap_or(uri)
}

fn path_matches(pattern: &str, path: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == path;
    }
    let mut rest = path;
    let mut first = true;
    for part in pattern.split('*') {
        if part.is_empty() {
            continue;
        }
        if first && !pattern.starts_with('*') {
            let Some(after) = rest.strip_prefix(part) else {
                return false;
            };
            rest = after;
        } else if let Some(index) = rest.find(part) {
            rest = &rest[index + part.len()..];
        } else {
            return false;
        }
        first = false;
    }
    pattern.ends_with('*') || rest.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_path_matching_is_ordered_and_anchored() {
        assert!(path_matches("/customers/*", "/customers/123"));
        assert!(path_matches("/v1/*/items/*", "/v1/acme/items/42"));
        assert!(!path_matches("/customers/*", "/orders/123"));
        assert!(!path_matches("/customers/*/detail", "/customers/1/other"));
    }
}
