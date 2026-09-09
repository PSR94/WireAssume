use crate::{Body, Header, Interaction};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use url::Url;

pub const REDACTED: &str = "[REDACTED]";

/// Redaction happens before an interaction is persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionPolicy {
    #[serde(default = "default_headers")]
    pub headers: BTreeSet<String>,
    #[serde(default = "default_query_parameters")]
    pub query_parameters: BTreeSet<String>,
    /// MVP JSONPath support: root paths composed of object keys, e.g. `$.token` or `$.user.secret`.
    #[serde(default)]
    pub jsonpaths: BTreeSet<String>,
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self {
            headers: default_headers(),
            query_parameters: default_query_parameters(),
            jsonpaths: BTreeSet::new(),
        }
    }
}

impl RedactionPolicy {
    pub fn apply(&self, interaction: &mut Interaction) {
        redact_headers(&self.headers, &mut interaction.request.headers);
        redact_headers(&self.headers, &mut interaction.response.headers);
        redact_uri(&self.query_parameters, &mut interaction.request.uri);
        redact_body(&self.jsonpaths, &mut interaction.request.body);
        redact_body(&self.jsonpaths, &mut interaction.response.body);
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
        *uri = parsed.into();
    }
}

fn redact_body(paths: &BTreeSet<String>, body: &mut Body) {
    let Body::Json(value) = body else {
        return;
    };
    for path in paths {
        if let Some(keys) = simple_jsonpath(path) {
            redact_json_path(value, &keys);
        }
    }
}

fn simple_jsonpath(path: &str) -> Option<Vec<&str>> {
    let remainder = path.strip_prefix("$.")?;
    if remainder.is_empty() || remainder.contains(['[', ']', '*']) {
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
                headers: vec![Header { name: "Authorization".into(), value: "Bearer secret".into() }],
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
        assert_eq!(interaction.response.body, Body::Json(json!({"user": {"token": REDACTED, "name": "Alice"}})));
    }
}
