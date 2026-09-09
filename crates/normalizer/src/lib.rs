//! Configurable normalization of volatile traffic before analysis.
//!
//! The recorder retains redacted evidence. Normalization produces a separate analysis view so
//! volatile values can be stabilized without destroying the original observed response.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use url::Url;
use wireassume_model::{Body, Header, Interaction};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizationPolicy {
    #[serde(default = "default_headers")]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub query_parameters: BTreeMap<String, String>,
    #[serde(default)]
    pub json_pointers: BTreeMap<String, String>,
}

impl Default for NormalizationPolicy {
    fn default() -> Self {
        Self {
            headers: default_headers(),
            query_parameters: BTreeMap::new(),
            json_pointers: BTreeMap::new(),
        }
    }
}

impl NormalizationPolicy {
    pub fn normalize(&self, interaction: &Interaction) -> Interaction {
        let mut normalized = interaction.clone();
        normalize_headers(&self.headers, &mut normalized.request.headers);
        normalize_headers(&self.headers, &mut normalized.response.headers);
        normalize_uri(&self.query_parameters, &mut normalized.request.uri);
        normalize_body(&self.json_pointers, &mut normalized.request.body);
        normalize_body(&self.json_pointers, &mut normalized.response.body);
        normalized
    }
}

fn default_headers() -> BTreeMap<String, String> {
    [
        ("date", "<normalized-date>"),
        ("x-request-id", "<normalized-request-id>"),
        ("x-correlation-id", "<normalized-correlation-id>"),
        ("traceparent", "<normalized-traceparent>"),
        ("tracestate", "<normalized-tracestate>"),
    ]
    .into_iter()
    .map(|(name, replacement)| (name.to_string(), replacement.to_string()))
    .collect()
}

fn normalize_headers(replacements: &BTreeMap<String, String>, headers: &mut [Header]) {
    for header in headers {
        if let Some(replacement) = replacements.get(&header.name.to_ascii_lowercase()) {
            header.value = replacement.clone();
        }
    }
}

fn normalize_uri(replacements: &BTreeMap<String, String>, uri: &mut String) {
    if replacements.is_empty() {
        return;
    }
    let Ok(mut parsed) = Url::parse(uri) else {
        return;
    };
    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(key, value)| {
            let replacement = replacements
                .get(&key.to_ascii_lowercase())
                .cloned()
                .unwrap_or_else(|| value.into_owned());
            (key.into_owned(), replacement)
        })
        .collect();
    parsed.query_pairs_mut().clear().extend_pairs(pairs);
    *uri = parsed.to_string();
}

fn normalize_body(replacements: &BTreeMap<String, String>, body: &mut Body) {
    let Body::Json(value) = body else {
        return;
    };
    for (pointer, replacement) in replacements {
        if let Some(target) = value.pointer_mut(pointer) {
            *target = Value::String(replacement.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wireassume_model::{InteractionMetadata, RequestRecord, ResponseRecord};

    #[test]
    fn normalization_preserves_original_and_changes_only_configured_values() {
        let original = Interaction {
            request: RequestRecord {
                method: "GET".into(),
                uri: "https://api.test/items?cursor=volatile&page=1".into(),
                headers: vec![Header {
                    name: "X-Request-Id".into(),
                    value: "req-123".into(),
                }],
                body: Body::Empty,
            },
            response: ResponseRecord {
                status: 200,
                headers: vec![],
                body: Body::Json(json!({"created_at": "2026-09-09T21:00:00Z", "name": "Alice"})),
            },
            metadata: InteractionMetadata::fixture("provider", "scenario"),
        };
        let mut policy = NormalizationPolicy::default();
        policy
            .query_parameters
            .insert("cursor".into(), "<cursor>".into());
        policy
            .json_pointers
            .insert("/created_at".into(), "<timestamp>".into());
        let normalized = policy.normalize(&original);

        assert!(original.request.uri.contains("volatile"));
        assert!(normalized.request.uri.contains("%3Ccursor%3E"));
        assert_eq!(
            normalized.request.headers[0].value,
            "<normalized-request-id>"
        );
        assert_eq!(
            normalized.response.body,
            Body::Json(json!({"created_at": "<timestamp>", "name": "Alice"}))
        );
    }
}
