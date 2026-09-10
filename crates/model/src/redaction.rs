use crate::{Body, Header, Interaction};
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
        Body::Empty | Body::Base64(_) => {}
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
