use base64::{engine::general_purpose::STANDARD, Engine};
use bytes::Bytes;
use wireassume_model::{Body, Header};

pub fn headers_to_model(headers: &axum::http::HeaderMap) -> Vec<Header> {
    headers
        .iter()
        .filter_map(|(name, value)| value.to_str().ok().map(|value| Header {
            name: name.as_str().to_string(),
            value: value.to_string(),
        }))
        .collect()
}

pub fn body_to_model(bytes: &Bytes, content_type: Option<&str>) -> Body {
    if bytes.is_empty() {
        return Body::Empty;
    }
    if content_type.is_some_and(|value| value.to_ascii_lowercase().contains("json")) {
        if let Ok(json) = serde_json::from_slice(bytes) {
            return Body::Json(json);
        }
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Body::Text(text.to_string()),
        Err(_) => Body::Base64(STANDARD.encode(bytes)),
    }
}

pub fn model_to_bytes(body: &Body) -> Result<Bytes, String> {
    match body {
        Body::Empty => Ok(Bytes::new()),
        Body::Json(value) => serde_json::to_vec(value).map(Bytes::from).map_err(|error| error.to_string()),
        Body::Text(text) => Ok(Bytes::copy_from_slice(text.as_bytes())),
        Body::Base64(encoded) => STANDARD.decode(encoded).map(Bytes::from).map_err(|error| error.to_string()),
    }
}

pub fn is_hop_by_hop_header(name: &axum::http::HeaderName) -> bool {
    matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "connection" | "keep-alive" | "proxy-authenticate" | "proxy-authorization" | "te" | "trailer" | "transfer-encoding" | "upgrade" | "host" | "content-length"
    )
}
