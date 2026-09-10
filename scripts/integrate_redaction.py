#!/usr/bin/env python3
from pathlib import Path


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text()
    if old in text:
        path.write_text(text.replace(old, new, 1))
        return
    if new in text:
        return
    raise SystemExit(f"{label} marker missing in {path}")


root_cargo = Path("Cargo.toml")
replace_once(
    root_cargo,
    'anyhow = "1.0"\n',
    'anyhow = "1.0"\nregex = "1.11"\n',
    "workspace regex dependency",
)

model_cargo = Path("crates/model/Cargo.toml")
replace_once(
    model_cargo,
    'url.workspace = true\n',
    'url.workspace = true\nregex.workspace = true\n',
    "model regex dependency",
)

Path("crates/model/src/redaction.rs").write_text(r'''use crate::{Body, Header, Interaction};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use url::Url;

pub const REDACTED: &str = "[REDACTED]";

/// Redaction happens before captured traffic or textual oracle evidence is persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionPolicy {
    #[serde(default = "default_headers")]
    pub headers: BTreeSet<String>,
    #[serde(default = "default_query_parameters")]
    pub query_parameters: BTreeSet<String>,
    /// Root/object-key JSON paths, e.g. `$.token` or `$.user.secret`.
    #[serde(default)]
    pub jsonpaths: BTreeSet<String>,
    /// Regular expressions applied to textual evidence and every JSON string value.
    #[serde(default)]
    pub regexes: BTreeSet<String>,
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self {
            headers: default_headers(),
            query_parameters: default_query_parameters(),
            jsonpaths: BTreeSet::new(),
            regexes: BTreeSet::new(),
        }
    }
}

impl RedactionPolicy {
    pub fn apply(&self, interaction: &mut Interaction) {
        redact_headers(&self.headers, &mut interaction.request.headers);
        redact_headers(&self.headers, &mut interaction.response.headers);
        for header in &mut interaction.request.headers {
            header.value = self.redact_text(&header.value);
        }
        for header in &mut interaction.response.headers {
            header.value = self.redact_text(&header.value);
        }
        redact_uri(&self.query_parameters, &mut interaction.request.uri);
        interaction.request.uri = self.redact_text(&interaction.request.uri);
        redact_body(self, &mut interaction.request.body);
        redact_body(self, &mut interaction.response.body);
    }

    /// Redact configured regex matches from text before it becomes persistent evidence.
    /// Invalid programmatic regexes fail closed by replacing the whole string; configuration
    /// loading rejects them earlier via `regex_errors`.
    pub fn redact_text(&self, value: &str) -> String {
        let mut redacted = value.to_string();
        for pattern in &self.regexes {
            let Ok(regex) = Regex::new(pattern) else {
                return REDACTED.to_string();
            };
            redacted = regex.replace_all(&redacted, REDACTED).into_owned();
        }
        redacted
    }

    pub fn regex_errors(&self) -> Vec<String> {
        self.regexes
            .iter()
            .filter_map(|pattern| {
                Regex::new(pattern)
                    .err()
                    .map(|error| format!("{pattern:?}: {error}"))
            })
            .collect()
    }
}

fn default_headers() -> BTreeSet<String> {
    [
        "authorization",
        "cookie",
        "set-cookie",
        "x-api-key",
        "x-auth-token",
        "proxy-authorization",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_query_parameters() -> BTreeSet<String> {
    ["api_key", "apikey", "access_token", "token", "key"]
        .into_iter()
        .map(String::from)
        .collect()
}

fn redact_headers(names: &BTreeSet<String>, headers: &mut [Header]) {
    for header in headers {
        if names.contains(&header.name.to_ascii_lowercase()) {
            header.value = REDACTED.to_string();
        }
    }
}

fn redact_uri(names: &BTreeSet<String>, uri: &mut String) {
    let Ok(mut parsed) = Url::parse(uri) else {
        return;
    };
    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(key, value)| {
            let value = if names.contains(&key.to_ascii_lowercase()) {
                REDACTED.to_string()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect();
    if !pairs.is_empty() {
        parsed.query_pairs_mut().clear().extend_pairs(pairs);
        *uri = parsed.to_string();
    }
}

fn redact_body(policy: &RedactionPolicy, body: &mut Body) {
    match body {
        Body::Json(value) => {
            for path in &policy.jsonpaths {
                if let Some(keys) = simple_jsonpath(path) {
                    redact_json_path(value, &keys);
                }
            }
            redact_json_strings(policy, value);
        }
        Body::Text(text) => *text = policy.redact_text(text),
        Body::Empty | Body::Bytes(_) => {}
    }
}

fn redact_json_strings(policy: &RedactionPolicy, value: &mut Value) {
    match value {
        Value::String(text) => *text = policy.redact_text(text),
        Value::Array(items) => {
            for item in items {
                redact_json_strings(policy, item);
            }
        }
        Value::Object(map) => {
            for child in map.values_mut() {
                redact_json_strings(policy, child);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn simple_jsonpath(path: &str) -> Option<Vec<&str>> {
    let remainder = path.strip_prefix("$.")?;
    if remainder.is_empty() || remainder.chars().any(|ch| matches!(ch, '[' | ']' | '*')) {
        return None;
    }
    Some(remainder.split('.').collect())
}

fn redact_json_path(value: &mut Value, keys: &[&str]) {
    let Some((first, rest)) = keys.split_first() else {
        return;
    };
    let Value::Object(map) = value else {
        return;
    };
    if rest.is_empty() {
        if map.contains_key(*first) {
            map.insert((*first).to_string(), Value::String(REDACTED.to_string()));
        }
    } else if let Some(child) = map.get_mut(*first) {
        redact_json_path(child, rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InteractionMetadata, RequestRecord, ResponseRecord};
    use serde_json::json;

    #[test]
    fn defaults_redact_credentials_and_configured_json_paths() {
        let mut policy = RedactionPolicy::default();
        policy.jsonpaths.insert("$.user.token".into());
        let mut interaction = Interaction {
            request: RequestRecord {
                method: "GET".into(),
                uri: "https://api.example.test/users?token=secret&page=1".into(),
                headers: vec![Header {
                    name: "Authorization".into(),
                    value: "Bearer secret".into(),
                }],
                body: Body::Empty,
            },
            response: ResponseRecord {
                status: 200,
                headers: vec![],
                body: Body::Json(json!({"user": {"token": "secret", "name": "Alice"}})),
            },
            metadata: InteractionMetadata::fixture("peoplecrm", "customer-profile"),
        };
        policy.apply(&mut interaction);
        assert_eq!(interaction.request.headers[0].value, REDACTED);
        assert!(interaction.request.uri.contains("token=%5BREDACTED%5D"));
        assert_eq!(
            interaction.response.body,
            Body::Json(json!({"user": {"token": REDACTED, "name": "Alice"}}))
        );
    }

    #[test]
    fn regexes_redact_text_and_json_strings() {
        let mut policy = RedactionPolicy::default();
        policy.regexes.insert(r"demo-secret-[0-9]+".into());
        assert_eq!(
            policy.redact_text("token=demo-secret-123"),
            "token=[REDACTED]"
        );

        let mut interaction = Interaction {
            request: RequestRecord {
                method: "POST".into(),
                uri: "https://api.example.test/users".into(),
                headers: vec![],
                body: Body::Text("demo-secret-123".into()),
            },
            response: ResponseRecord {
                status: 200,
                headers: vec![],
                body: Body::Json(json!({"nested": ["demo-secret-456"]})),
            },
            metadata: InteractionMetadata::fixture("peoplecrm", "redaction"),
        };
        policy.apply(&mut interaction);
        assert_eq!(interaction.request.body, Body::Text(REDACTED.into()));
        assert_eq!(
            interaction.response.body,
            Body::Json(json!({"nested": [REDACTED]}))
        );
    }

    #[test]
    fn invalid_regexes_are_reported_and_fail_closed_for_direct_use() {
        let mut policy = RedactionPolicy::default();
        policy.regexes.insert("[".into());
        assert_eq!(policy.regex_errors().len(), 1);
        assert_eq!(policy.redact_text("sensitive"), REDACTED);
    }
}
''')

config = Path("crates/config/src/lib.rs")
replace_once(
    config,
    '''        if self.proxy.max_payload_bytes == 0 {
            errors.push("proxy.max_payload_bytes must be greater than zero".to_string());
        }

        let mut names = BTreeSet::new();''',
    '''        if self.proxy.max_payload_bytes == 0 {
            errors.push("proxy.max_payload_bytes must be greater than zero".to_string());
        }
        for error in self.redaction.regex_errors() {
            errors.push(format!("redaction.regexes contains invalid pattern {error}"));
        }

        let mut names = BTreeSet::new();''',
    "redaction regex validation",
)

experiment = Path("crates/experiment/src/lib.rs")
replace_once(
    experiment,
    'use wireassume_model::{canonical_json, stable_id, Body, CorpusError, CorpusStore};',
    'use wireassume_model::{\n    canonical_json, stable_id, Body, CorpusError, CorpusStore, RedactionPolicy,\n};',
    "experiment redaction import",
)
replace_once(
    experiment,
    '''pub struct ExperimentRunner {
    workspace: PathBuf,
    controller: ExperimentReplayController,
    oracle: Arc<dyn Oracle>,
}''',
    '''pub struct ExperimentRunner {
    workspace: PathBuf,
    controller: ExperimentReplayController,
    oracle: Arc<dyn Oracle>,
    redaction: RedactionPolicy,
}''',
    "experiment runner redaction field",
)
replace_once(
    experiment,
    '''        Self {
            workspace: workspace.into(),
            controller,
            oracle,
        }
    }

    pub fn plan''',
    '''        Self {
            workspace: workspace.into(),
            controller,
            oracle,
            redaction: RedactionPolicy::default(),
        }
    }

    pub fn with_redaction(mut self, redaction: RedactionPolicy) -> Self {
        self.redaction = redaction;
        self
    }

    pub fn plan''',
    "experiment runner redaction builder",
)
experiment_text = experiment.read_text()
experiment_text = experiment_text.replace(
    'oracle_artifact(self.oracle.kind(), baseline_evaluated)',
    'oracle_artifact(self.oracle.kind(), baseline_evaluated, &self.redaction)',
)
experiment_text = experiment_text.replace(
    'oracle_artifact(self.oracle.kind(), evaluated)',
    'oracle_artifact(self.oracle.kind(), evaluated, &self.redaction)',
)
replace_target = '''                self.controller.clone(),
                self.oracle.clone(),
            )'''
replace_value = '''                self.controller.clone(),
                self.oracle.clone(),
                self.redaction.clone(),
            )'''
if replace_target in experiment_text:
    experiment_text = experiment_text.replace(replace_target, replace_value, 1)
elif replace_value not in experiment_text:
    raise SystemExit("minimization redaction argument marker missing")
old_artifact = '''fn oracle_artifact(kind: &str, result: Result<OracleResult, OracleError>) -> OracleArtifact {
    match result {
        Ok(result) => OracleArtifact {
            oracle_kind: kind.to_string(),
            outcome: match result.status {
                OracleStatus::Pass => TrialOutcome::Pass,
                OracleStatus::Fail => TrialOutcome::Fail,
                OracleStatus::Inconclusive => TrialOutcome::Inconclusive,
            },
            duration_ms: result.duration_ms,
            summary: result.summary,
            stdout: result.stdout,
            stderr: result.stderr,
        },
        Err(error) => OracleArtifact {
            oracle_kind: kind.to_string(),
            outcome: TrialOutcome::Inconclusive,
            duration_ms: 0,
            summary: error.to_string(),
            stdout: String::new(),
            stderr: error.to_string(),
        },
    }
}'''
new_artifact = '''fn oracle_artifact(
    kind: &str,
    result: Result<OracleResult, OracleError>,
    redaction: &RedactionPolicy,
) -> OracleArtifact {
    match result {
        Ok(result) => OracleArtifact {
            oracle_kind: kind.to_string(),
            outcome: match result.status {
                OracleStatus::Pass => TrialOutcome::Pass,
                OracleStatus::Fail => TrialOutcome::Fail,
                OracleStatus::Inconclusive => TrialOutcome::Inconclusive,
            },
            duration_ms: result.duration_ms,
            summary: redaction.redact_text(&result.summary),
            stdout: redaction.redact_text(&result.stdout),
            stderr: redaction.redact_text(&result.stderr),
        },
        Err(error) => {
            let error = redaction.redact_text(&error.to_string());
            OracleArtifact {
                oracle_kind: kind.to_string(),
                outcome: TrialOutcome::Inconclusive,
                duration_ms: 0,
                summary: error.clone(),
                stdout: String::new(),
                stderr: error,
            }
        }
    }
}'''
if old_artifact in experiment_text:
    experiment_text = experiment_text.replace(old_artifact, new_artifact, 1)
elif new_artifact not in experiment_text:
    raise SystemExit("oracle artifact redaction marker missing")
experiment.write_text(experiment_text)

minimize = Path("crates/experiment/src/minimize.rs")
replace_once(
    minimize,
    'use wireassume_model::{stable_id, Body, ResponseRecord};',
    'use wireassume_model::{stable_id, Body, RedactionPolicy, ResponseRecord};',
    "minimization redaction import",
)
replace_once(
    minimize,
    '''    controller: ExperimentReplayController,
    oracle: Arc<dyn Oracle>,
) -> Option<(ResponseMinimization, MinimizationArtifact)> {''',
    '''    controller: ExperimentReplayController,
    oracle: Arc<dyn Oracle>,
    redaction: RedactionPolicy,
) -> Option<(ResponseMinimization, MinimizationArtifact)> {''',
    "minimization redaction parameter",
)
replace_once(
    minimize,
    '''    let trials_for_test = trials.clone();

    let minimized = minimize_success_async''',
    '''    let trials_for_test = trials.clone();
    let redaction_for_test = redaction;

    let minimized = minimize_success_async''',
    "minimization redaction capture",
)
replace_once(
    minimize,
    '''        let trials = trials_for_test.clone();

        async move {''',
    '''        let trials = trials_for_test.clone();
        let redaction = redaction_for_test.clone();

        async move {''',
    "minimization redaction clone",
)
replace_once(
    minimize,
    '''            trials.lock().await.push(MinimizationTrial {
                candidate_fields,
                outcome: label.into(),
                duration_ms,
                summary,
            });''',
    '''            trials.lock().await.push(MinimizationTrial {
                candidate_fields,
                outcome: label.into(),
                duration_ms,
                summary: redaction.redact_text(&summary),
            });''',
    "minimization summary redaction",
)

cli = Path("crates/cli/src/main.rs")
replace_once(
    cli,
    '    let runner = ExperimentRunner::new(&workspace, replay.controller.clone(), oracle);',
    '''    let runner = ExperimentRunner::new(&workspace, replay.controller.clone(), oracle)
        .with_redaction(config.redaction.clone());''',
    "CLI experiment redaction policy",
)

orbitdesk = Path("demo/orbitdesk.py")
replace_once(
    orbitdesk,
    '    print(f"OrbitDesk customer profile OK: {normalized_email}")\n',
    '    print("OrbitDesk debug token: demo-secret-123")\n    print(f"OrbitDesk customer profile OK: {normalized_email}")\n',
    "demo fake secret output",
)

demo_config = Path("demo/wireassume.yml")
replace_once(
    demo_config,
    '''proxy:
  listen: 127.0.0.1:0
  max_payload_bytes: 2097152

scenarios:''',
    '''proxy:
  listen: 127.0.0.1:0
  max_payload_bytes: 2097152

redaction:
  regexes:
    - 'demo-secret-[0-9]+'

scenarios:''',
    "demo regex redaction",
)

verify = Path("demo/verify_demo.py")
replace_once(
    verify,
    '''    if report["baseline"]["summary"].find("passed") < 0:
        return fail("baseline oracle did not pass")''',
    '''    persisted_json = glob.glob(".wireassume/evidence/*/*.json") + glob.glob(
        ".wireassume/runs/run_*/*.json"
    )
    persisted_text = "\\n".join(
        open(path, encoding="utf-8").read() for path in persisted_json
    )
    if "demo-secret-123" in persisted_text:
        return fail("configured fake secret leaked into persisted oracle evidence")
    if "[REDACTED]" not in persisted_text:
        return fail("oracle evidence did not contain the expected redaction marker")

    if report["baseline"]["summary"].find("passed") < 0:
        return fail("baseline oracle did not pass")''',
    "demo persisted evidence redaction check",
)
replace_once(
    verify,
    '''        "the successful response to {email}; OpenAPI marks the dependency optional + nullable"
''',
    '''        "the successful response to {email}; OpenAPI marks the dependency optional + nullable; "
        "configured regex redaction removes the fake oracle secret before persistence"
''',
    "demo verification summary",
)

config_doc = Path("docs/configuration.md")
text = config_doc.read_text()
text = text.replace(
    '''  jsonpaths:
    - $.user.secret
    - $.credentials.password
```''',
    '''  jsonpaths:
    - $.user.secret
    - $.credentials.password
  regexes:
    - 'Bearer\\s+[A-Za-z0-9._-]+'
    - 'demo-secret-[0-9]+'
```''',
)
text = text.replace(
    '''Traffic redaction happens before persistence. The current JSON path implementation supports root/object-key paths such as `$.user.secret`; wildcard/array/full JSONPath and regex redaction are tracked as pre-release limitations.''',
    '''Traffic redaction happens before persistence. Regexes are validated at configuration load and are applied to textual captured bodies, JSON string values, header values, request URIs, and persisted oracle/minimization summaries. The current JSON path implementation supports root/object-key paths such as `$.user.secret`; wildcard/array/full JSONPath remains a pre-release limitation.''',
)
config_doc.write_text(text)

status = Path("docs/project-status.md")
text = status.read_text().replace(
    '''| Redaction | Implemented, partial rule surface | Default credential headers/query params and simple object-key JSON paths are redacted before traffic persistence. Full JSONPath/regex rules remain. |''',
    '''| Redaction | Implemented, partial JSONPath surface | Default credential headers/query params, simple object-key JSON paths, and validated regex rules are redacted before persistence. The same regex policy scrubs persisted oracle text. Full wildcard/array JSONPath remains. |''',
)
status.write_text(text)

readme = Path("README.md")
text = readme.read_text().replace(
    '''- Configured JSON body paths can be redacted before persistence.
''',
    '''- Configured JSON body paths and validated regex rules can be redacted before persistence; regex rules also scrub persisted oracle stdout/stderr/summaries.
''',
).replace(
    '''The richer browser evidence DSL, multi-worker experiment concurrency, full JSONPath/regex redaction, backend service, and interactive Next.js dashboard are not represented here as completed features.''',
    '''The richer browser evidence DSL, multi-worker experiment concurrency, full wildcard/array JSONPath redaction, backend service, and interactive Next.js dashboard are not represented here as completed features.''',
)
readme.write_text(text)

changelog = Path("CHANGELOG.md")
text = changelog.read_text().replace(
    '''- Default redaction for credential-like headers and query parameters before traffic persistence.
''',
    '''- Default redaction for credential-like headers and query parameters before traffic persistence.
- Validated regex redaction for captured textual/JSON values and persisted oracle/minimization evidence.
''',
).replace(
    '''- Redaction supports configured simple object-key JSON paths; full JSONPath and regex redaction remain to be completed.
''',
    '''- Redaction supports configured simple object-key JSON paths and regexes; wildcard/array/full JSONPath remains to be completed.
''',
)
changelog.write_text(text)
