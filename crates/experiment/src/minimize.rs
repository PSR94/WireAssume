use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use wireassume_delta_debugger::{minimize_success_async, TestOutcome};
use wireassume_model::{stable_id, Body, RedactionPolicy, ResponseRecord};
use wireassume_oracles::{Oracle, OracleStatus};
use wireassume_proxy::{ExperimentReplayController, ResponseOverride};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMinimization {
    pub id: String,
    pub interaction_id: String,
    pub original_fields: Vec<String>,
    pub minimal_fields: Vec<String>,
    pub tests_executed: usize,
    pub established: bool,
    pub artifact_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimizationArtifact {
    pub interaction_id: String,
    pub original_fields: Vec<String>,
    pub minimal_fields: Vec<String>,
    pub tests_executed: usize,
    pub established: bool,
    pub trials: Vec<MinimizationTrial>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimizationTrial {
    pub candidate_fields: Vec<String>,
    pub outcome: String,
    pub duration_ms: u64,
    pub summary: String,
}

#[derive(Serialize)]
struct MinimizationFingerprint<'a> {
    interaction_id: &'a str,
    original_fields: &'a [String],
    minimal_fields: &'a [String],
    established: bool,
    trial_outcomes: Vec<(&'a [String], &'a str)>,
}

pub(crate) async fn minimize_response_fields(
    interaction_id: &str,
    baseline: &ResponseRecord,
    controller: ExperimentReplayController,
    oracle: Arc<dyn Oracle>,
    redaction: RedactionPolicy,
) -> Option<(ResponseMinimization, MinimizationArtifact)> {
    let Body::Json(Value::Object(object)) = &baseline.body else {
        return None;
    };
    if object.len() < 2 {
        return None;
    }

    let mut original_fields: Vec<_> = object.keys().cloned().collect();
    original_fields.sort();
    let values = Arc::new(object.clone());
    let baseline = baseline.clone();
    let interaction_id_owned = interaction_id.to_string();
    let trials = Arc::new(Mutex::new(Vec::<MinimizationTrial>::new()));

    let controller_for_test = controller.clone();
    let oracle_for_test = oracle.clone();
    let values_for_test = values.clone();
    let baseline_for_test = baseline.clone();
    let interaction_for_test = interaction_id_owned.clone();
    let trials_for_test = trials.clone();
    let redaction_for_test = redaction;

    let minimized = minimize_success_async(original_fields.clone(), move |mut candidate_fields| {
        candidate_fields.sort();
        let controller = controller_for_test.clone();
        let oracle = oracle_for_test.clone();
        let values = values_for_test.clone();
        let baseline = baseline_for_test.clone();
        let interaction_id = interaction_for_test.clone();
        let trials = trials_for_test.clone();
        let redaction = redaction_for_test.clone();

        async move {
            let mut candidate_object = Map::new();
            for field in &candidate_fields {
                if let Some(value) = values.get(field) {
                    candidate_object.insert(field.clone(), value.clone());
                }
            }
            let mut response = baseline;
            response.body = Body::Json(Value::Object(candidate_object));
            controller
                .apply(ResponseOverride {
                    interaction_id,
                    response,
                    delay_ms: 0,
                })
                .await;

            let evaluated = oracle.evaluate().await;
            controller.reset().await;
            let (outcome, label, duration_ms, summary) = match evaluated {
                Ok(result) => match result.status {
                    OracleStatus::Pass => (
                        TestOutcome::Pass,
                        "pass",
                        result.duration_ms,
                        result.summary,
                    ),
                    OracleStatus::Fail => (
                        TestOutcome::Fail,
                        "fail",
                        result.duration_ms,
                        result.summary,
                    ),
                    OracleStatus::Inconclusive => (
                        TestOutcome::Unresolved,
                        "unresolved",
                        result.duration_ms,
                        result.summary,
                    ),
                },
                Err(error) => (TestOutcome::Unresolved, "unresolved", 0, error.to_string()),
            };
            trials.lock().await.push(MinimizationTrial {
                candidate_fields,
                outcome: label.into(),
                duration_ms,
                summary: redaction.redact_text(&summary),
            });
            outcome
        }
    })
    .await;

    controller.reset().await;
    let recorded_trials = trials.lock().await.clone();
    let (minimal_fields, tests_executed, established) = match minimized {
        Ok(result) => (result.items, result.tests_executed, true),
        Err(_) => (original_fields.clone(), recorded_trials.len(), false),
    };

    let artifact = MinimizationArtifact {
        interaction_id: interaction_id_owned.clone(),
        original_fields: original_fields.clone(),
        minimal_fields: minimal_fields.clone(),
        tests_executed,
        established,
        trials: recorded_trials,
    };
    let fingerprint = MinimizationFingerprint {
        interaction_id: &interaction_id_owned,
        original_fields: &original_fields,
        minimal_fields: &minimal_fields,
        established,
        trial_outcomes: artifact
            .trials
            .iter()
            .map(|trial| (trial.candidate_fields.as_slice(), trial.outcome.as_str()))
            .collect(),
    };
    let id = stable_id("min", &fingerprint)
        .expect("minimization fingerprint contains only serializable values");
    Some((
        ResponseMinimization {
            id,
            interaction_id: interaction_id_owned,
            original_fields,
            minimal_fields,
            tests_executed,
            established,
            artifact_ref: String::new(),
        },
        artifact,
    ))
}
