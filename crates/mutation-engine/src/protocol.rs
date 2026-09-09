use crate::{MutationFingerprint, MutationKind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use wireassume_model::{stable_id, Body, Header, ResponseRecord};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMutatorConfig {
    #[serde(default = "default_status_codes")]
    pub status_codes: Vec<u16>,
    #[serde(default = "default_delay_ms")]
    pub delay_ms: u64,
    #[serde(default)]
    pub malformed_body: bool,
}

impl Default for ProtocolMutatorConfig {
    fn default() -> Self {
        Self {
            status_codes: default_status_codes(),
            delay_ms: default_delay_ms(),
            malformed_body: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolMutation {
    pub id: String,
    pub kind: MutationKind,
    pub target: String,
    pub description: String,
    pub seed: u64,
    pub response: ResponseRecord,
    #[serde(default)]
    pub delay_ms: u64,
}

impl ProtocolMutation {
    fn new(
        kind: MutationKind,
        target: impl Into<String>,
        description: impl Into<String>,
        seed: u64,
        variant: impl Into<String>,
        response: ResponseRecord,
        delay_ms: u64,
    ) -> Self {
        let target = target.into();
        let fingerprint = MutationFingerprint {
            kind,
            path: target.clone(),
            seed,
            variant: variant.into(),
        };
        let id = stable_id("mut", &fingerprint).expect("protocol fingerprint is serializable");
        Self {
            id,
            kind,
            target,
            description: description.into(),
            seed,
            response,
            delay_ms,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProtocolMutator {
    pub config: ProtocolMutatorConfig,
}

impl ProtocolMutator {
    pub fn generate(&self, baseline: &ResponseRecord, seed: u64) -> Vec<ProtocolMutation> {
        let mut out = Vec::new();
        self.status_mutations(baseline, seed, &mut out);
        self.header_mutations(baseline, seed, &mut out);
        self.body_mutations(baseline, seed, &mut out);
        self.error_mutations(baseline, seed, &mut out);
        self.pagination_mutations(baseline, seed, &mut out);

        if self.config.delay_ms > 0 {
            out.push(ProtocolMutation::new(
                MutationKind::DelayResponse,
                "timing.response",
                format!("delay response by {} ms", self.config.delay_ms),
                seed,
                format!("delay-{}", self.config.delay_ms),
                baseline.clone(),
                self.config.delay_ms,
            ));
        }

        let mut seen = HashSet::new();
        out.into_iter()
            .filter(|mutation| mutation.response != *baseline || mutation.delay_ms > 0)
            .filter(|mutation| seen.insert(mutation.id.clone()))
            .collect()
    }

    fn status_mutations(
        &self,
        baseline: &ResponseRecord,
        seed: u64,
        out: &mut Vec<ProtocolMutation>,
    ) {
        for status in &self.config.status_codes {
            if *status == baseline.status {
                continue;
            }
            let mut response = baseline.clone();
            response.status = *status;
            out.push(ProtocolMutation::new(
                MutationKind::StatusCode,
                "response.status",
                format!("change HTTP status to {status}"),
                seed,
                format!("status-{status}"),
                response,
                0,
            ));
        }

        let mut redirect = baseline.clone();
        redirect.status = 302;
        set_header(&mut redirect.headers, "location", "/__wireassume_redirect__");
        out.push(ProtocolMutation::new(
            MutationKind::Redirect,
            "response.status",
            "return a 302 redirect",
            seed,
            "redirect-302",
            redirect,
            0,
        ));
    }

    fn header_mutations(
        &self,
        baseline: &ResponseRecord,
        seed: u64,
        out: &mut Vec<ProtocolMutation>,
    ) {
        let mut headers = baseline.headers.clone();
        headers.sort_by_key(|header| header.name.to_ascii_lowercase());

        for header in headers {
            let mut response = baseline.clone();
            response
                .headers
                .retain(|candidate| !candidate.name.eq_ignore_ascii_case(&header.name));
            out.push(ProtocolMutation::new(
                MutationKind::RemoveHeader,
                format!("response.header.{}", header.name.to_ascii_lowercase()),
                format!("remove response header {}", header.name),
                seed,
                format!("remove-header-{}", header.name.to_ascii_lowercase()),
                response,
                0,
            ));
        }

        let mut content_type = baseline.clone();
        set_header(
            &mut content_type.headers,
            "content-type",
            "text/plain; charset=utf-8",
        );
        out.push(ProtocolMutation::new(
            MutationKind::ChangeContentType,
            "response.header.content-type",
            "change Content-Type to text/plain",
            seed,
            "content-type-text",
            content_type,
            0,
        ));

        let mut additional = baseline.clone();
        set_header(&mut additional.headers, "x-wireassume-unknown", "1");
        out.push(ProtocolMutation::new(
            MutationKind::AdditionalHeader,
            "response.headers",
            "add an unknown response header",
            seed,
            "additional-header",
            additional,
            0,
        ));
    }

    fn body_mutations(
        &self,
        baseline: &ResponseRecord,
        seed: u64,
        out: &mut Vec<ProtocolMutation>,
    ) {
        let mut empty = baseline.clone();
        empty.body = Body::Empty;
        out.push(ProtocolMutation::new(
            MutationKind::EmptyResponse,
            "response.body",
            "return an empty response body",
            seed,
            "empty-response",
            empty,
            0,
        ));

        if self.config.malformed_body && matches!(baseline.body, Body::Json(_)) {
            let mut malformed = baseline.clone();
            malformed.body = Body::Text("{\"wireassume\":".into());
            out.push(ProtocolMutation::new(
                MutationKind::MalformedBody,
                "response.body",
                "return malformed JSON",
                seed,
                "malformed-json",
                malformed,
                0,
            ));
        }
    }

    fn error_mutations(
        &self,
        baseline: &ResponseRecord,
        seed: u64,
        out: &mut Vec<ProtocolMutation>,
    ) {
        if baseline.status < 400 {
            return;
        }
        let Body::Json(Value::Object(object)) = &baseline.body else {
            return;
        };

        if object.contains_key("code") {
            push_json_object_change(
                baseline,
                seed,
                out,
                ObjectChangeSpec::new(
                    MutationKind::ErrorCodeMissing,
                    "response.body.code",
                    "remove error code",
                    "error-remove-code",
                ),
                |map| {
                    map.remove("code");
                },
            );
            push_json_object_change(
                baseline,
                seed,
                out,
                ObjectChangeSpec::new(
                    MutationKind::UnknownErrorCode,
                    "response.body.code",
                    "replace error code with an unknown value",
                    "error-unknown-code",
                ),
                |map| {
                    map.insert("code".into(), json!("__WIREASSUME_UNKNOWN__"));
                },
            );
        }

        if object.contains_key("message") {
            push_json_object_change(
                baseline,
                seed,
                out,
                ObjectChangeSpec::new(
                    MutationKind::ErrorMessageMissing,
                    "response.body.message",
                    "remove error message",
                    "error-remove-message",
                ),
                |map| {
                    map.remove("message");
                },
            );
        }

        let mut empty = baseline.clone();
        empty.body = Body::Json(json!({}));
        out.push(ProtocolMutation::new(
            MutationKind::EmptyErrorObject,
            "response.body",
            "replace error body with an empty object",
            seed,
            "error-empty-object",
            empty,
            0,
        ));

        let mut non_json = baseline.clone();
        non_json.body = Body::Text("upstream error".into());
        set_header(
            &mut non_json.headers,
            "content-type",
            "text/plain; charset=utf-8",
        );
        out.push(ProtocolMutation::new(
            MutationKind::NonJsonErrorBody,
            "response.body",
            "replace JSON error with plain text",
            seed,
            "error-text",
            non_json,
            0,
        ));

        let mut html = baseline.clone();
        html.body = Body::Text("<!doctype html><title>Upstream error</title>".into());
        set_header(
            &mut html.headers,
            "content-type",
            "text/html; charset=utf-8",
        );
        out.push(ProtocolMutation::new(
            MutationKind::HtmlErrorPage,
            "response.body",
            "replace error with an HTML page",
            seed,
            "error-html",
            html,
            0,
        ));
    }

    fn pagination_mutations(
        &self,
        baseline: &ResponseRecord,
        seed: u64,
        out: &mut Vec<ProtocolMutation>,
    ) {
        let Body::Json(value) = &baseline.body else {
            return;
        };
        let mut pointers = Vec::new();
        find_pagination_fields(value, "", &mut pointers);

        for (pointer, field_kind) in pointers {
            if let Some(mutated) = remove_json_pointer(value, &pointer) {
                let mut response = baseline.clone();
                response.body = Body::Json(mutated);
                let kind = if field_kind == PaginationField::Metadata {
                    MutationKind::PaginationMissingMetadata
                } else {
                    MutationKind::PaginationMissingCursor
                };
                out.push(ProtocolMutation::new(
                    kind,
                    format!("response.body{pointer}"),
                    format!("remove pagination field {pointer}"),
                    seed,
                    format!("pagination-remove-{pointer}"),
                    response,
                    0,
                ));
            }

            if field_kind == PaginationField::Cursor {
                if let Some(mutated) = replace_json_pointer(value, &pointer, Value::Null) {
                    let mut response = baseline.clone();
                    response.body = Body::Json(mutated);
                    out.push(ProtocolMutation::new(
                        MutationKind::PaginationNullCursor,
                        format!("response.body{pointer}"),
                        format!("set pagination cursor {pointer} to null"),
                        seed,
                        format!("pagination-null-{pointer}"),
                        response,
                        0,
                    ));
                }
            }
        }
    }
}

struct ObjectChangeSpec<'a> {
    kind: MutationKind,
    target: &'a str,
    description: &'a str,
    variant: &'a str,
}

impl<'a> ObjectChangeSpec<'a> {
    fn new(
        kind: MutationKind,
        target: &'a str,
        description: &'a str,
        variant: &'a str,
    ) -> Self {
        Self {
            kind,
            target,
            description,
            variant,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaginationField {
    Cursor,
    Metadata,
}

fn find_pagination_fields(
    value: &Value,
    pointer: &str,
    out: &mut Vec<(String, PaginationField)>,
) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            for key in keys {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                let child_pointer = format!("{pointer}/{escaped}");
                let normalized: String = key
                    .to_ascii_lowercase()
                    .chars()
                    .filter(|ch| *ch != '-' && *ch != '_')
                    .collect();
                if matches!(
                    normalized.as_str(),
                    "cursor" | "nextcursor" | "nextpagecursor" | "nexttoken" | "pagetoken"
                ) {
                    out.push((child_pointer.clone(), PaginationField::Cursor));
                } else if matches!(
                    normalized.as_str(),
                    "pagination" | "pagemeta" | "pagemetadata"
                ) {
                    out.push((child_pointer.clone(), PaginationField::Metadata));
                }
                find_pagination_fields(&map[key], &child_pointer, out);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                find_pagination_fields(item, &format!("{pointer}/{index}"), out);
            }
        }
        _ => {}
    }
}

fn remove_json_pointer(value: &Value, pointer: &str) -> Option<Value> {
    let (parent_pointer, key) = pointer.rsplit_once('/')?;
    let key = key.replace("~1", "/").replace("~0", "~");
    let mut candidate = value.clone();
    let parent = if parent_pointer.is_empty() {
        &mut candidate
    } else {
        candidate.pointer_mut(parent_pointer)?
    };
    match parent {
        Value::Object(map) => {
            map.remove(&key)?;
        }
        Value::Array(items) => {
            let index: usize = key.parse().ok()?;
            if index >= items.len() {
                return None;
            }
            items.remove(index);
        }
        _ => return None,
    }
    Some(candidate)
}

fn replace_json_pointer(value: &Value, pointer: &str, replacement: Value) -> Option<Value> {
    let mut candidate = value.clone();
    let target = candidate.pointer_mut(pointer)?;
    *target = replacement;
    Some(candidate)
}

fn set_header(headers: &mut Vec<Header>, name: &str, value: &str) {
    headers.retain(|header| !header.name.eq_ignore_ascii_case(name));
    headers.push(Header {
        name: name.into(),
        value: value.into(),
    });
    headers.sort_by_key(|header| header.name.to_ascii_lowercase());
}

fn push_json_object_change<F>(
    baseline: &ResponseRecord,
    seed: u64,
    out: &mut Vec<ProtocolMutation>,
    spec: ObjectChangeSpec<'_>,
    change: F,
) where
    F: FnOnce(&mut serde_json::Map<String, Value>),
{
    let Body::Json(Value::Object(mut object)) = baseline.body.clone() else {
        return;
    };
    change(&mut object);
    let mut response = baseline.clone();
    response.body = Body::Json(Value::Object(object));
    out.push(ProtocolMutation::new(
        spec.kind,
        spec.target,
        spec.description,
        seed,
        spec.variant,
        response,
        0,
    ));
}

fn default_status_codes() -> Vec<u16> {
    vec![400, 422, 429, 500, 502, 503, 504]
}

fn default_delay_ms() -> u64 {
    1_000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16, body: Value) -> ResponseRecord {
        ResponseRecord {
            status,
            headers: vec![Header {
                name: "Content-Type".into(),
                value: "application/json".into(),
            }],
            body: Body::Json(body),
        }
    }

    #[test]
    fn protocol_generation_is_deterministic() {
        let baseline = response(200, json!({"items": [1, 2], "next_cursor": "abc"}));
        let mutator = ProtocolMutator::default();
        assert_eq!(
            mutator.generate(&baseline, 42),
            mutator.generate(&baseline, 42)
        );
    }

    #[test]
    fn pagination_and_status_mutations_change_real_response_data() {
        let baseline = response(200, json!({"items": [1, 2], "next_cursor": "abc"}));
        let mutations = ProtocolMutator::default().generate(&baseline, 42);
        assert!(mutations.iter().any(|mutation| {
            mutation.kind == MutationKind::PaginationMissingCursor
                && mutation.response != baseline
        }));
        assert!(mutations.iter().any(|mutation| {
            mutation.kind == MutationKind::StatusCode && mutation.response.status == 429
        }));
    }

    #[test]
    fn error_contract_mutations_are_only_generated_for_error_responses() {
        let baseline = response(422, json!({"code": "invalid", "message": "bad input"}));
        let mutations = ProtocolMutator::default().generate(&baseline, 9);
        assert!(mutations
            .iter()
            .any(|mutation| mutation.kind == MutationKind::ErrorCodeMissing));
        assert!(mutations
            .iter()
            .any(|mutation| mutation.kind == MutationKind::HtmlErrorPage));
    }
}
