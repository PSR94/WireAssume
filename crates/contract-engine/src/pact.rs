use crate::{ConsumptionLock, Requirement};
use serde_json::{json, Map, Value};

/// Export the experimentally observed structural contract as a Pact v3-compatible document.
///
/// This is intentionally a compatibility bridge, not WireAssume's source of truth. Behavioral
/// assumptions that Pact cannot represent remain under `metadata.wireassume.assumptions`.
pub fn to_pact_json(lock: &ConsumptionLock) -> Result<String, serde_json::Error> {
    let mut body = Value::Object(Map::new());
    let mut matching_rules = Map::new();

    let mut requirements = lock.requirements.clone();
    requirements.sort_by(|a, b| a.target.path.cmp(&b.target.path));
    for requirement in &requirements {
        if requirement.target.kind != "response-body" {
            continue;
        }
        insert_placeholder(&mut body, &requirement.target.path, placeholder(requirement));
        let path = pact_path(&requirement.target.path);
        if let Some(matcher) = matcher(requirement) {
            matching_rules.insert(path, matcher);
        }
    }

    let concrete_path = concrete_request_path(&lock.endpoint.path);
    let document = json!({
        "consumer": {"name": lock.consumer.name},
        "provider": {"name": lock.provider.name},
        "interactions": [{
            "description": format!("WireAssume observed scenario: {}", lock.scenario.name),
            "providerStates": [],
            "request": {
                "method": lock.endpoint.method,
                "path": concrete_path,
            },
            "response": {
                "status": 200,
                "headers": {"Content-Type": "application/json"},
                "body": body,
                "matchingRules": {
                    "body": matching_rules,
                }
            }
        }],
        "metadata": {
            "pactSpecification": {"version": "3.0.0"},
            "wireassume": {
                "schema": lock.schema,
                "sourceRevision": lock.metadata.source_revision,
                "scenario": lock.scenario,
                "originalEndpointPath": lock.endpoint.path,
                "assumptions": lock.assumptions,
                "note": "Generated compatibility export. WireAssume evidence remains authoritative."
            }
        }
    });
    serde_json::to_string_pretty(&document)
}

fn placeholder(requirement: &Requirement) -> Value {
    match requirement.types.first().map(String::as_str) {
        Some("boolean") => Value::Bool(true),
        Some("integer") | Some("number") => json!(1),
        Some("array") => Value::Array(Vec::new()),
        Some("object") => Value::Object(Map::new()),
        _ => Value::String("wireassume-example".into()),
    }
}

fn matcher(requirement: &Requirement) -> Option<Value> {
    if requirement.types.is_empty() {
        return None;
    }
    Some(json!({
        "combine": "AND",
        "matchers": [{"match": "type"}]
    }))
}

fn insert_placeholder(root: &mut Value, pointer: &str, value: Value) {
    let segments = pointer_segments(pointer);
    insert_segments(root, &segments, &value);
}

fn insert_segments(current: &mut Value, segments: &[String], value: &Value) {
    let Some((segment, rest)) = segments.split_first() else {
        *current = value.clone();
        return;
    };

    if !current.is_object() {
        *current = Value::Object(Map::new());
    }
    let object = current
        .as_object_mut()
        .expect("value was converted to an object above");
    if rest.is_empty() {
        object.insert(segment.clone(), value.clone());
        return;
    }
    let child = object
        .entry(segment.clone())
        .or_insert_with(|| Value::Object(Map::new()));
    insert_segments(child, rest, value);
}

fn pointer_segments(pointer: &str) -> Vec<String> {
    pointer
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

fn pact_path(pointer: &str) -> String {
    let segments = pointer_segments(pointer);
    if segments.is_empty() {
        return "$".into();
    }
    let mut path = String::from("$");
    for segment in segments {
        if segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            path.push('.');
            path.push_str(&segment);
        } else {
            path.push_str("['");
            path.push_str(&segment.replace('\\', "\\\\").replace('\'', "\\'"));
            path.push_str("']");
        }
    }
    path
}

fn concrete_request_path(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if segment == "*" || (segment.starts_with('{') && segment.ends_with('}')) {
                "wireassume-sample"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Assumption, ContractMetadata, EndpointRef, Party, RevisionRef, ScenarioRef, TargetRef,
        CONSUMPTION_SCHEMA_V1,
    };

    fn lock() -> ConsumptionLock {
        ConsumptionLock {
            schema: CONSUMPTION_SCHEMA_V1.into(),
            metadata: ContractMetadata {
                tool_version: "0.1.0".into(),
                generated_at: "2026-09-10T00:00:00Z".into(),
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
                id: "profile".into(),
                name: "profile".into(),
                oracle: "command".into(),
            },
            endpoint: EndpointRef {
                method: "GET".into(),
                path: "/customers/*".into(),
            },
            requirements: vec![Requirement {
                id: "req".into(),
                target: TargetRef {
                    kind: "response-body".into(),
                    path: "/email".into(),
                },
                presence: Some("required".into()),
                types: vec!["string".into()],
                nullable: Some(false),
                accepted_empty: Some(true),
                confidence: "observed".into(),
                severity: "high".into(),
                evidence_refs: vec!["ev".into()],
            }],
            assumptions: vec![Assumption {
                id: "asm".into(),
                assumption_type: "presence".into(),
                target: TargetRef {
                    kind: "response-body".into(),
                    path: "/email".into(),
                },
                behavior: "value must be present".into(),
                confidence: "observed".into(),
                severity: "high".into(),
                provider_comparison: "undocumented".into(),
                evidence_refs: vec!["ev".into()],
            }],
            evidence: vec![],
            history: vec![],
        }
    }

    #[test]
    fn exports_pact_v3_with_structural_matcher_and_wireassume_metadata() {
        let exported = to_pact_json(&lock()).unwrap();
        let value: Value = serde_json::from_str(&exported).unwrap();
        assert_eq!(value["consumer"]["name"], "OrbitDesk");
        assert_eq!(
            value["interactions"][0]["request"]["path"],
            "/customers/wireassume-sample"
        );
        assert_eq!(
            value["interactions"][0]["response"]["body"]["email"],
            "wireassume-example"
        );
        assert_eq!(
            value["metadata"]["wireassume"]["originalEndpointPath"],
            "/customers/*"
        );
    }
}
