use crate::{ConsumptionLock, Requirement};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenApiComparison {
    pub matched_path: Option<String>,
    pub compared_assumptions: usize,
    pub mismatches: Vec<ProviderMismatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMismatch {
    pub target: String,
    pub assumption_type: String,
    pub provider_guarantee: String,
    pub classification: String,
}

#[derive(Debug, Clone, Default)]
struct PropertyGuarantee {
    documented: bool,
    required: bool,
    nullable: bool,
    types: BTreeSet<String>,
}

/// Compare already-observed consumer assumptions with provider guarantees.
///
/// This function never creates assumptions. It only annotates assumptions that were
/// produced by real counterfactual experiment failures.
pub fn apply_openapi_comparison(
    lock: &mut ConsumptionLock,
    document: &str,
) -> Result<OpenApiComparison, serde_yaml::Error> {
    let root: Value = serde_yaml::from_str(document)?;
    let Some((matched_path, response_schema)) =
        response_schema(&root, &lock.endpoint.method, &lock.endpoint.path)
    else {
        for assumption in &mut lock.assumptions {
            assumption.provider_comparison = "unknown".into();
        }
        return Ok(OpenApiComparison {
            matched_path: None,
            compared_assumptions: 0,
            mismatches: Vec::new(),
        });
    };

    let requirements: BTreeMap<String, Requirement> = lock
        .requirements
        .iter()
        .cloned()
        .map(|requirement| (requirement.target.path.clone(), requirement))
        .collect();
    let mut compared = 0usize;
    let mut mismatches = Vec::new();

    for assumption in &mut lock.assumptions {
        if assumption.target.kind != "response-body" {
            assumption.provider_comparison = "unknown".into();
            continue;
        }

        let guarantee = property_guarantee(&root, response_schema, &assumption.target.path);
        let requirement = requirements.get(&assumption.target.path);
        let (provider_guarantee, classification) =
            classify(&assumption.assumption_type, &guarantee, requirement);
        assumption.provider_comparison = classification.to_string();
        compared += 1;

        if classification != "guaranteed" && classification != "unknown" {
            mismatches.push(ProviderMismatch {
                target: assumption.target.path.clone(),
                assumption_type: assumption.assumption_type.clone(),
                provider_guarantee: provider_guarantee.to_string(),
                classification: classification.to_string(),
            });
        }
    }

    mismatches.sort_by(|a, b| {
        a.target
            .cmp(&b.target)
            .then(a.assumption_type.cmp(&b.assumption_type))
    });

    Ok(OpenApiComparison {
        matched_path: Some(matched_path.to_string()),
        compared_assumptions: compared,
        mismatches,
    })
}

fn response_schema<'a>(
    root: &'a Value,
    method: &str,
    endpoint: &str,
) -> Option<(&'a str, &'a Value)> {
    let paths = root.get("paths")?.as_object()?;
    let mut candidates: Vec<_> = paths.iter().collect();
    candidates.sort_by(|a, b| a.0.cmp(b.0));
    let (path, path_item) = candidates
        .into_iter()
        .find(|(candidate, _)| path_templates_match(endpoint, candidate))?;
    let operation = path_item.get(method.to_ascii_lowercase())?;
    let responses = operation.get("responses")?.as_object()?;
    let response = responses.get("200").or_else(|| {
        let mut success: Vec<_> = responses
            .iter()
            .filter(|(status, _)| status.starts_with('2'))
            .collect();
        success.sort_by(|a, b| a.0.cmp(b.0));
        success.first().map(|(_, response)| *response)
    })?;
    let response = resolve_ref(root, response);
    let content = response.get("content")?.as_object()?;
    let media = content
        .get("application/json")
        .or_else(|| content.values().next())?;
    let schema = resolve_ref(root, media.get("schema")?);
    Some((path.as_str(), schema))
}

fn property_guarantee(root: &Value, response_schema: &Value, pointer: &str) -> PropertyGuarantee {
    let mut current = resolve_ref(root, response_schema);
    let segments: Vec<_> = pointer
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect();

    if segments.is_empty() {
        return PropertyGuarantee {
            documented: true,
            required: true,
            nullable: is_nullable(current),
            types: schema_types(current),
        };
    }

    for (index, segment) in segments.iter().enumerate() {
        current = resolve_ref(root, current);
        if current.get("type").and_then(Value::as_str) == Some("array") {
            let Some(items) = current.get("items") else {
                return PropertyGuarantee::default();
            };
            current = resolve_ref(root, items);
        }

        let required = current
            .get("required")
            .and_then(Value::as_array)
            .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(segment)));
        let Some(property) = current
            .get("properties")
            .and_then(Value::as_object)
            .and_then(|properties| properties.get(segment))
        else {
            return PropertyGuarantee::default();
        };
        let property = resolve_ref(root, property);

        if index + 1 == segments.len() {
            return PropertyGuarantee {
                documented: true,
                required,
                nullable: is_nullable(property),
                types: schema_types(property),
            };
        }
        current = property;
    }

    PropertyGuarantee::default()
}

fn classify(
    assumption_type: &str,
    guarantee: &PropertyGuarantee,
    requirement: Option<&Requirement>,
) -> (&'static str, &'static str) {
    if !guarantee.documented {
        return ("undocumented", "undocumented");
    }

    match assumption_type {
        "presence" => {
            if guarantee.required {
                ("required", "guaranteed")
            } else {
                ("optional", "undocumented")
            }
        }
        "nullability" => {
            if guarantee.nullable {
                ("nullable", "contradicted")
            } else {
                ("non-null", "guaranteed")
            }
        }
        "type" => {
            let Some(requirement) = requirement else {
                return ("documented", "unknown");
            };
            if requirement.types.is_empty() || guarantee.types.is_empty() {
                return ("documented", "unknown");
            }
            let compatible = requirement
                .types
                .iter()
                .all(|required| guarantee.types.contains(required));
            if compatible {
                ("typed", "guaranteed")
            } else {
                ("different-type", "incompatible")
            }
        }
        _ => ("documented", "unknown"),
    }
}

fn resolve_ref<'a>(root: &'a Value, value: &'a Value) -> &'a Value {
    let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
        return value;
    };
    reference
        .strip_prefix('#')
        .and_then(|pointer| root.pointer(pointer))
        .unwrap_or(value)
}

fn is_nullable(schema: &Value) -> bool {
    if schema.get("nullable").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    match schema.get("type") {
        Some(Value::String(kind)) => kind == "null",
        Some(Value::Array(types)) => types.iter().any(|kind| kind.as_str() == Some("null")),
        _ => schema
            .get("anyOf")
            .or_else(|| schema.get("oneOf"))
            .and_then(Value::as_array)
            .is_some_and(|variants| variants.iter().any(is_nullable)),
    }
}

fn schema_types(schema: &Value) -> BTreeSet<String> {
    match schema.get("type") {
        Some(Value::String(kind)) if kind != "null" => [kind.clone()].into_iter().collect(),
        Some(Value::Array(types)) => types
            .iter()
            .filter_map(Value::as_str)
            .filter(|kind| *kind != "null")
            .map(str::to_string)
            .collect(),
        _ => BTreeSet::new(),
    }
}

fn path_templates_match(consumer: &str, provider: &str) -> bool {
    let consumer: Vec<_> = consumer.trim_matches('/').split('/').collect();
    let provider: Vec<_> = provider.trim_matches('/').split('/').collect();
    consumer.len() == provider.len()
        && consumer.iter().zip(provider).all(|(left, right)| {
            *left == "*" || (right.starts_with('{') && right.ends_with('}')) || *left == right
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Assumption, ContractMetadata, EndpointRef, Party, RevisionRef, ScenarioRef, TargetRef,
    };

    fn lock() -> ConsumptionLock {
        ConsumptionLock {
            schema: "wireassume.consumption/v1".into(),
            metadata: ContractMetadata {
                tool_version: "0.1.0".into(),
                generated_at: "2026-09-09T00:00:00Z".into(),
                seed: 42,
                source_revision: RevisionRef {
                    kind: "git".into(),
                    value: "abc".into(),
                },
            },
            provider: Party {
                id: "peoplecrm".into(),
                name: "PeopleCRM".into(),
            },
            consumer: Party {
                id: "orbitdesk".into(),
                name: "OrbitDesk".into(),
            },
            scenario: ScenarioRef {
                id: "customer-profile".into(),
                name: "customer-profile".into(),
                oracle: "command".into(),
            },
            endpoint: EndpointRef {
                method: "GET".into(),
                path: "/customers/*".into(),
            },
            requirements: vec![Requirement {
                id: "req_email".into(),
                target: TargetRef {
                    kind: "response-body".into(),
                    path: "/email".into(),
                },
                presence: Some("required".into()),
                types: vec![],
                nullable: Some(false),
                accepted_empty: Some(true),
                confidence: "observed".into(),
                severity: "high".into(),
                evidence_refs: vec!["ev_a".into()],
            }],
            assumptions: vec![
                Assumption {
                    id: "asm_presence".into(),
                    assumption_type: "presence".into(),
                    target: TargetRef {
                        kind: "response-body".into(),
                        path: "/email".into(),
                    },
                    behavior: "value must be present".into(),
                    confidence: "observed".into(),
                    severity: "high".into(),
                    provider_comparison: "not-compared".into(),
                    evidence_refs: vec!["ev_a".into()],
                },
                Assumption {
                    id: "asm_null".into(),
                    assumption_type: "nullability".into(),
                    target: TargetRef {
                        kind: "response-body".into(),
                        path: "/email".into(),
                    },
                    behavior: "null is not tolerated".into(),
                    confidence: "observed".into(),
                    severity: "high".into(),
                    provider_comparison: "not-compared".into(),
                    evidence_refs: vec!["ev_b".into()],
                },
            ],
            evidence: vec![],
            history: vec![],
        }
    }

    #[test]
    fn optional_nullable_email_is_reported_as_provider_mismatch() {
        let spec = r#"
openapi: 3.0.3
paths:
  /customers/{id}:
    get:
      responses:
        "200":
          content:
            application/json:
              schema:
                type: object
                properties:
                  email:
                    type: string
                    nullable: true
"#;
        let mut lock = lock();
        let summary = apply_openapi_comparison(&mut lock, spec).unwrap();
        assert_eq!(summary.matched_path.as_deref(), Some("/customers/{id}"));
        assert_eq!(summary.compared_assumptions, 2);
        assert_eq!(summary.mismatches.len(), 2);
        assert_eq!(lock.assumptions[0].provider_comparison, "undocumented");
        assert_eq!(lock.assumptions[1].provider_comparison, "contradicted");
        assert_eq!(summary.mismatches[0].provider_guarantee, "nullable");
        assert_eq!(summary.mismatches[1].provider_guarantee, "optional");
    }

    #[test]
    fn comparison_never_creates_assumptions() {
        let spec = "openapi: 3.0.3\npaths: {}\n";
        let mut lock = lock();
        lock.assumptions.clear();
        let summary = apply_openapi_comparison(&mut lock, spec).unwrap();
        assert!(lock.assumptions.is_empty());
        assert_eq!(summary.compared_assumptions, 0);
    }
}
