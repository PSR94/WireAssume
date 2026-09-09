//! Deterministic inference of observed consumer requirements from experiment evidence.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
use wireassume_experiment::{ExperimentReport, TrialOutcome, TrialResult};
use wireassume_model::{stable_id, Body, ResponseRecord};
use wireassume_mutation_engine::MutationKind;

mod openapi;
pub use openapi::{apply_openapi_comparison, OpenApiComparison, ProviderMismatch};

pub const CONSUMPTION_SCHEMA_V1: &str = "wireassume.consumption/v1";

#[derive(Debug, Clone)]
pub struct ContractContext {
    pub tool_version: String,
    pub generated_at: String,
    pub provider_id: String,
    pub provider_name: String,
    pub consumer_id: String,
    pub consumer_name: String,
    pub scenario_id: String,
    pub scenario_name: String,
    pub oracle: String,
    pub method: String,
    pub path: String,
    pub source_revision_kind: String,
    pub source_revision: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsumptionLock {
    pub schema: String,
    pub metadata: ContractMetadata,
    pub provider: Party,
    pub consumer: Party,
    pub scenario: ScenarioRef,
    pub endpoint: EndpointRef,
    pub requirements: Vec<Requirement>,
    pub assumptions: Vec<Assumption>,
    pub evidence: Vec<EvidenceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractMetadata {
    pub tool_version: String,
    pub generated_at: String,
    pub seed: u64,
    pub source_revision: RevisionRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevisionRef {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Party {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioRef {
    pub id: String,
    pub name: String,
    pub oracle: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointRef {
    pub method: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetRef {
    pub kind: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Requirement {
    pub id: String,
    pub target: TargetRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nullable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_empty: Option<bool>,
    pub confidence: String,
    pub severity: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assumption {
    pub id: String,
    #[serde(rename = "type")]
    pub assumption_type: String,
    pub target: TargetRef,
    pub behavior: String,
    pub confidence: String,
    pub severity: String,
    pub provider_comparison: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub id: String,
    pub interaction_id: String,
    pub mutation_id: String,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_summary: Option<String>,
    pub artifact_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub revision: RevisionRef,
    pub assumption_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToleranceMap {
    pub target: String,
    pub cells: Vec<ToleranceCell>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToleranceCell {
    pub mutation: MutationKind,
    pub outcome: TrialOutcome,
    pub evidence_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractAnalysis {
    /// 100 means the consumer survived every weighted decisive mutation in this run.
    pub dependency_resilience_score: u8,
    pub decisive_trials: usize,
    pub failing_trials: usize,
    pub tolerance: Vec<ToleranceMap>,
}

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("missing baseline response for interaction {0}")]
    MissingBaseline(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
}

pub fn build_consumption_lock(
    report: &ExperimentReport,
    baselines: &BTreeMap<String, ResponseRecord>,
    context: &ContractContext,
) -> Result<ConsumptionLock, ContractError> {
    let mut requirement_builders: BTreeMap<String, RequirementBuilder> = BTreeMap::new();
    let mut assumption_builders: BTreeMap<String, AssumptionBuilder> = BTreeMap::new();

    for result in &report.results {
        let baseline = baselines
            .get(&result.interaction_id)
            .ok_or_else(|| ContractError::MissingBaseline(result.interaction_id.clone()))?;
        update_requirement(&mut requirement_builders, result, baseline);

        if result.outcome == TrialOutcome::Fail {
            if let Some((assumption_type, behavior, severity)) = assumption_from_failure(result) {
                let key = format!("{assumption_type}\u{0}{}\u{0}{behavior}", result.target);
                let builder = assumption_builders
                    .entry(key)
                    .or_insert_with(|| AssumptionBuilder {
                        assumption_type: assumption_type.to_string(),
                        target: target_ref(&result.target),
                        behavior: behavior.to_string(),
                        severity: severity.to_string(),
                        evidence_refs: BTreeSet::new(),
                    });
                builder.evidence_refs.insert(result.evidence_id.clone());
            }
        }
    }

    let requirements = requirement_builders
        .into_values()
        .map(RequirementBuilder::finish)
        .collect::<Result<Vec<_>, _>>()?;
    let assumptions = assumption_builders
        .into_values()
        .map(AssumptionBuilder::finish)
        .collect::<Result<Vec<_>, _>>()?;
    let evidence = report.results.iter().map(evidence_ref).collect();

    Ok(ConsumptionLock {
        schema: CONSUMPTION_SCHEMA_V1.into(),
        metadata: ContractMetadata {
            tool_version: context.tool_version.clone(),
            generated_at: context.generated_at.clone(),
            seed: report.seed,
            source_revision: RevisionRef {
                kind: context.source_revision_kind.clone(),
                value: context.source_revision.clone(),
            },
        },
        provider: Party {
            id: context.provider_id.clone(),
            name: context.provider_name.clone(),
        },
        consumer: Party {
            id: context.consumer_id.clone(),
            name: context.consumer_name.clone(),
        },
        scenario: ScenarioRef {
            id: context.scenario_id.clone(),
            name: context.scenario_name.clone(),
            oracle: context.oracle.clone(),
        },
        endpoint: EndpointRef {
            method: context.method.to_ascii_uppercase(),
            path: context.path.clone(),
        },
        requirements,
        assumptions,
        evidence,
        history: Vec::new(),
    })
}

pub fn analyze_contract(report: &ExperimentReport) -> ContractAnalysis {
    let mut by_target: BTreeMap<String, Vec<ToleranceCell>> = BTreeMap::new();
    let mut passed_weight = 0u64;
    let mut total_weight = 0u64;

    for result in &report.results {
        by_target
            .entry(result.target.clone())
            .or_default()
            .push(ToleranceCell {
                mutation: result.kind,
                outcome: result.outcome,
                evidence_id: result.evidence_id.clone(),
            });

        if result.outcome != TrialOutcome::Inconclusive {
            let weight = mutation_weight(result.kind);
            total_weight += weight;
            if result.outcome == TrialOutcome::Pass {
                passed_weight += weight;
            }
        }
    }

    for cells in by_target.values_mut() {
        cells.sort_by(|a, b| {
            a.mutation
                .cmp(&b.mutation)
                .then(a.evidence_id.cmp(&b.evidence_id))
        });
    }

    let score = (passed_weight * 100 + total_weight / 2)
        .checked_div(total_weight)
        .unwrap_or(0)
        .min(100) as u8;

    ContractAnalysis {
        dependency_resilience_score: score,
        decisive_trials: report.passes + report.failures,
        failing_trials: report.failures,
        tolerance: by_target
            .into_iter()
            .map(|(target, cells)| ToleranceMap { target, cells })
            .collect(),
    }
}

pub fn to_json(lock: &ConsumptionLock) -> Result<String, ContractError> {
    Ok(format!("{}\n", serde_json::to_string_pretty(lock)?))
}

pub fn to_yaml(lock: &ConsumptionLock) -> Result<String, ContractError> {
    Ok(serde_yaml::to_string(lock)?)
}

pub fn to_markdown(lock: &ConsumptionLock, analysis: &ContractAnalysis) -> String {
    let mut out = String::new();
    out.push_str("# WireAssume Consumer Contract Analysis\n\n");
    out.push_str(&format!("- Provider: **{}**\n", lock.provider.name));
    out.push_str(&format!("- Consumer: **{}**\n", lock.consumer.name));
    out.push_str(&format!("- Scenario: **{}**\n", lock.scenario.name));
    out.push_str(&format!(
        "- Endpoint: `{} {}`\n",
        lock.endpoint.method, lock.endpoint.path
    ));
    out.push_str(&format!(
        "- Dependency Resilience Score: **{} / 100** (100 = survived all weighted decisive mutations)\n",
        analysis.dependency_resilience_score
    ));
    out.push_str(&format!(
        "- Observed assumptions: **{}**\n\n",
        lock.assumptions.len()
    ));
    out.push_str("## Assumptions\n\n");

    if lock.assumptions.is_empty() {
        out.push_str(
            "No failing counterfactual mutation produced an observed assumption in this run.\n",
        );
    } else {
        for assumption in &lock.assumptions {
            out.push_str(&format!(
                "### {} — `{}`\n\n",
                assumption.assumption_type, assumption.target.path
            ));
            out.push_str(&format!("{}\n\n", assumption.behavior));
            out.push_str(&format!(
                "Severity: **{}** · Confidence: **{}** · Provider comparison: **{}**\n\n",
                assumption.severity, assumption.confidence, assumption.provider_comparison
            ));
        }
    }
    out
}

#[derive(Default)]
struct RequirementBuilder {
    target: Option<TargetRef>,
    presence: Option<String>,
    types: BTreeSet<String>,
    nullable: Option<bool>,
    accepted_empty: Option<bool>,
    severity: String,
    evidence_refs: BTreeSet<String>,
}

impl RequirementBuilder {
    fn finish(self) -> Result<Requirement, serde_json::Error> {
        let target = self
            .target
            .expect("requirement builder is created only with a target");
        let fingerprint = (
            &target.kind,
            &target.path,
            &self.presence,
            &self.types,
            &self.nullable,
            &self.accepted_empty,
        );
        Ok(Requirement {
            id: stable_id("req", &fingerprint)?,
            target,
            presence: self.presence,
            types: self.types.into_iter().collect(),
            nullable: self.nullable,
            accepted_empty: self.accepted_empty,
            confidence: "observed".into(),
            severity: if self.severity.is_empty() {
                "low".into()
            } else {
                self.severity
            },
            evidence_refs: self.evidence_refs.into_iter().collect(),
        })
    }
}

struct AssumptionBuilder {
    assumption_type: String,
    target: TargetRef,
    behavior: String,
    severity: String,
    evidence_refs: BTreeSet<String>,
}

impl AssumptionBuilder {
    fn finish(self) -> Result<Assumption, serde_json::Error> {
        let fingerprint = (&self.assumption_type, &self.target.path, &self.behavior);
        Ok(Assumption {
            id: stable_id("asm", &fingerprint)?,
            assumption_type: self.assumption_type,
            target: self.target,
            behavior: self.behavior,
            confidence: "observed".into(),
            severity: self.severity,
            provider_comparison: "not-compared".into(),
            evidence_refs: self.evidence_refs.into_iter().collect(),
        })
    }
}

fn update_requirement(
    builders: &mut BTreeMap<String, RequirementBuilder>,
    result: &TrialResult,
    baseline: &ResponseRecord,
) {
    if result.outcome == TrialOutcome::Inconclusive || !result.target.starts_with("response.body") {
        return;
    }

    let failed = result.outcome == TrialOutcome::Fail;
    let relevant = matches!(
        result.kind,
        MutationKind::RemoveField
            | MutationKind::NullField
            | MutationKind::EmptyString
            | MutationKind::WrongPrimitiveType
    );
    if !relevant {
        return;
    }

    let builder = builders.entry(result.target.clone()).or_default();
    builder.target = Some(target_ref(&result.target));
    builder.evidence_refs.insert(result.evidence_id.clone());

    match result.kind {
        MutationKind::RemoveField => {
            builder.presence = Some(if failed { "required" } else { "optional" }.into());
        }
        MutationKind::NullField => builder.nullable = Some(!failed),
        MutationKind::EmptyString => builder.accepted_empty = Some(!failed),
        MutationKind::WrongPrimitiveType if failed => {
            if let Some(value_type) = baseline_type_at_target(baseline, &result.target) {
                builder.types.insert(value_type.to_string());
            }
        }
        _ => {}
    }

    if failed {
        builder.severity = max_severity(&builder.severity, severity_for(result.kind)).to_string();
    }
}

fn baseline_type_at_target(baseline: &ResponseRecord, target: &str) -> Option<&'static str> {
    let pointer = target.strip_prefix("response.body")?;
    let Body::Json(value) = &baseline.body else {
        return None;
    };
    let value = if pointer.is_empty() {
        value
    } else {
        value.pointer(pointer)?
    };
    Some(value_type(value))
}

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn evidence_ref(result: &TrialResult) -> EvidenceRef {
    EvidenceRef {
        id: result.evidence_id.clone(),
        interaction_id: result.interaction_id.clone(),
        mutation_id: result.mutation_id.clone(),
        outcome: match result.outcome {
            TrialOutcome::Pass => "pass",
            TrialOutcome::Fail => "fail",
            TrialOutcome::Inconclusive => "inconclusive",
        }
        .into(),
        failure_summary: (result.outcome == TrialOutcome::Fail).then(|| result.summary.clone()),
        artifact_refs: result.artifact_refs.clone(),
    }
}

fn target_ref(target: &str) -> TargetRef {
    let (kind, path) = if let Some(path) = target.strip_prefix("response.body") {
        ("response-body", if path.is_empty() { "/" } else { path })
    } else if target == "response.status" {
        ("response-status", "status")
    } else if let Some(path) = target.strip_prefix("response.header.") {
        ("response-header", path)
    } else if target.starts_with("timing.") {
        ("timing", target)
    } else if target.contains("pagination") || target.contains("cursor") {
        ("pagination", target)
    } else {
        ("response-body", target)
    };
    TargetRef {
        kind: kind.into(),
        path: path.into(),
    }
}

fn assumption_from_failure(
    result: &TrialResult,
) -> Option<(&'static str, &'static str, &'static str)> {
    use MutationKind::*;
    Some(match result.kind {
        RemoveField => ("presence", "value must be present", "high"),
        NullField => ("nullability", "null is not tolerated", "high"),
        WrongPrimitiveType => ("type", "observed value type is significant", "high"),
        UnknownEnum => ("enum", "unknown enum-like value is not tolerated", "high"),
        NumericZero | NegativeNumber | VeryLargeNumber | FloatInsteadOfInteger => (
            "numeric-range",
            "numeric edge case is not tolerated",
            "medium",
        ),
        EmptyString | WhitespaceString | UnicodeString | LongString => (
            "string-format",
            "string edge case is not tolerated",
            "medium",
        ),
        ReverseArray | ShuffleArray => {
            ("array-order", "array ordering affects the workflow", "high")
        }
        EmptyArray | RemoveFirstArrayItem | RemoveLastArrayItem | OneArrayItem
        | RepeatedArrayItems => (
            "cardinality",
            "array cardinality/content change affects the workflow",
            "medium",
        ),
        DuplicateArrayItem | DuplicateValue => (
            "duplicate-handling",
            "duplicate value is not tolerated",
            "medium",
        ),
        AdditionalUnknownProperty => (
            "unknown-field-tolerance",
            "additional unknown field is not tolerated",
            "medium",
        ),
        RemoveHeader => (
            "header",
            "response header is required by the workflow",
            "high",
        ),
        ChangeContentType => (
            "content-type",
            "Content-Type semantics affect the workflow",
            "high",
        ),
        StatusCode => (
            "status-code",
            "HTTP status variation affects the workflow",
            "high",
        ),
        Redirect => (
            "redirect-behavior",
            "redirect response is not tolerated",
            "medium",
        ),
        ErrorCodeMissing | ErrorMessageMissing | EmptyErrorObject | NonJsonErrorBody
        | HtmlErrorPage | UnknownErrorCode => (
            "error-shape",
            "error response shape affects the workflow",
            "high",
        ),
        PaginationMissingCursor | PaginationNullCursor | PaginationMissingMetadata => (
            "pagination",
            "pagination metadata behavior affects the workflow",
            "high",
        ),
        DelayResponse => (
            "timing",
            "increased response latency affects the workflow",
            "medium",
        ),
        EmptyResponse | MalformedBody => (
            "error-shape",
            "empty or malformed response is not tolerated",
            "medium",
        ),
        EmptyObject => ("presence", "object contents are required", "medium"),
        AdditionalHeader => return None,
    })
}

fn severity_for(kind: MutationKind) -> &'static str {
    assumption_from_kind(kind).map_or("low", |(_, severity)| severity)
}

fn assumption_from_kind(kind: MutationKind) -> Option<(&'static str, &'static str)> {
    use MutationKind::*;
    Some(match kind {
        RemoveField
        | NullField
        | WrongPrimitiveType
        | UnknownEnum
        | ReverseArray
        | ShuffleArray
        | RemoveHeader
        | ChangeContentType
        | StatusCode
        | ErrorCodeMissing
        | ErrorMessageMissing
        | EmptyErrorObject
        | NonJsonErrorBody
        | HtmlErrorPage
        | UnknownErrorCode
        | PaginationMissingCursor
        | PaginationNullCursor
        | PaginationMissingMetadata => ("structural", "high"),
        NumericZero
        | NegativeNumber
        | VeryLargeNumber
        | FloatInsteadOfInteger
        | EmptyString
        | WhitespaceString
        | UnicodeString
        | LongString
        | EmptyArray
        | EmptyObject
        | DuplicateArrayItem
        | DuplicateValue
        | RemoveFirstArrayItem
        | RemoveLastArrayItem
        | OneArrayItem
        | RepeatedArrayItems
        | AdditionalUnknownProperty
        | Redirect
        | EmptyResponse
        | MalformedBody
        | DelayResponse => ("edge", "medium"),
        AdditionalHeader => return None,
    })
}

fn mutation_weight(kind: MutationKind) -> u64 {
    match severity_for(kind) {
        "critical" => 8,
        "high" => 4,
        "medium" => 2,
        _ => 1,
    }
}

fn max_severity<'a>(current: &'a str, candidate: &'a str) -> &'a str {
    if severity_rank(candidate) > severity_rank(current) {
        candidate
    } else {
        current
    }
}

fn severity_rank(value: &str) -> u8 {
    match value {
        "critical" => 4,
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wireassume_experiment::{BaselineResult, ExperimentReport};
    use wireassume_model::Header;

    fn report(results: Vec<TrialResult>) -> ExperimentReport {
        let passes = results
            .iter()
            .filter(|result| result.outcome == TrialOutcome::Pass)
            .count();
        let failures = results
            .iter()
            .filter(|result| result.outcome == TrialOutcome::Fail)
            .count();
        ExperimentReport {
            run_id: "run_test".into(),
            generated_at: "2026-09-09T00:00:00Z".into(),
            scenario_id: "customer-profile".into(),
            source_revision: Some("abc123".into()),
            seed: 42,
            budget: 10,
            baseline: BaselineResult {
                duration_ms: 1,
                summary: "pass".into(),
                artifact_refs: vec!["runs/run_test/baseline-oracle.json".into()],
            },
            planned_mutations: results.len(),
            executed_mutations: results.len(),
            passes,
            failures,
            inconclusive: results.len() - passes - failures,
            results,
            minimizations: vec![],
        }
    }

    fn trial(kind: MutationKind, outcome: TrialOutcome, target: &str, id: &str) -> TrialResult {
        TrialResult {
            evidence_id: format!("ev_{id}"),
            interaction_id: "int_1".into(),
            mutation_id: format!("mut_{id}"),
            kind,
            target: target.into(),
            description: "test".into(),
            outcome,
            duration_ms: 1,
            summary: if outcome == TrialOutcome::Fail {
                "consumer failed"
            } else {
                "consumer passed"
            }
            .into(),
            artifact_refs: vec![format!("evidence/ev_{id}/oracle.json")],
        }
    }

    #[test]
    fn failing_missing_and_null_trials_infer_observed_requirement() {
        let report = report(vec![
            trial(
                MutationKind::RemoveField,
                TrialOutcome::Fail,
                "response.body/email",
                "missing",
            ),
            trial(
                MutationKind::NullField,
                TrialOutcome::Fail,
                "response.body/email",
                "null",
            ),
            trial(
                MutationKind::EmptyString,
                TrialOutcome::Pass,
                "response.body/email",
                "empty",
            ),
        ]);
        let mut baselines = BTreeMap::new();
        baselines.insert(
            "int_1".into(),
            ResponseRecord {
                status: 200,
                headers: vec![Header {
                    name: "content-type".into(),
                    value: "application/json".into(),
                }],
                body: Body::Json(serde_json::json!({"email": "alice@example.com"})),
            },
        );
        let context = ContractContext {
            tool_version: "0.1.0".into(),
            generated_at: "2026-09-09T00:00:00Z".into(),
            provider_id: "peoplecrm".into(),
            provider_name: "PeopleCRM".into(),
            consumer_id: "orbitdesk".into(),
            consumer_name: "OrbitDesk".into(),
            scenario_id: "customer-profile".into(),
            scenario_name: "Customer profile".into(),
            oracle: "http".into(),
            method: "GET".into(),
            path: "/customers/{id}".into(),
            source_revision_kind: "git".into(),
            source_revision: "abc123".into(),
        };
        let lock = build_consumption_lock(&report, &baselines, &context).unwrap();
        let requirement = lock
            .requirements
            .iter()
            .find(|requirement| requirement.target.path == "/email")
            .unwrap();
        assert_eq!(requirement.presence.as_deref(), Some("required"));
        assert_eq!(requirement.nullable, Some(false));
        assert_eq!(requirement.accepted_empty, Some(true));
        assert_eq!(lock.assumptions.len(), 2);
        assert!(!lock.evidence[0].artifact_refs.is_empty());
    }

    #[test]
    fn inconclusive_trials_do_not_become_tolerance_claims() {
        let report = report(vec![trial(
            MutationKind::RemoveField,
            TrialOutcome::Inconclusive,
            "response.body/email",
            "infra",
        )]);
        let mut baselines = BTreeMap::new();
        baselines.insert(
            "int_1".into(),
            ResponseRecord {
                status: 200,
                headers: vec![],
                body: Body::Json(serde_json::json!({"email": "a@example.com"})),
            },
        );
        let context = ContractContext {
            tool_version: "0.1.0".into(),
            generated_at: "2026-09-09T00:00:00Z".into(),
            provider_id: "p".into(),
            provider_name: "P".into(),
            consumer_id: "c".into(),
            consumer_name: "C".into(),
            scenario_id: "s".into(),
            scenario_name: "S".into(),
            oracle: "command".into(),
            method: "GET".into(),
            path: "/x".into(),
            source_revision_kind: "workspace".into(),
            source_revision: "working-tree".into(),
        };
        let lock = build_consumption_lock(&report, &baselines, &context).unwrap();
        assert!(lock.requirements.is_empty());
        assert!(lock.assumptions.is_empty());
    }

    #[test]
    fn resilience_score_is_weighted_survival_not_an_llm_judgment() {
        let report = report(vec![
            trial(
                MutationKind::RemoveField,
                TrialOutcome::Fail,
                "response.body/email",
                "a",
            ),
            trial(
                MutationKind::UnicodeString,
                TrialOutcome::Pass,
                "response.body/email",
                "b",
            ),
        ]);
        let analysis = analyze_contract(&report);
        assert_eq!(analysis.decisive_trials, 2);
        assert!(analysis.dependency_resilience_score < 50);
    }
}
