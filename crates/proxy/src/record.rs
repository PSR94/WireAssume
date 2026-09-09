use crate::wire::{body_to_model, headers_to_model, is_hop_by_hop_header};
use axum::{body::{to_bytes, Body as AxumBody}, extract::State, http::{Request, Response, StatusCode}, response::IntoResponse, routing::any, Router};
use bytes::Bytes;
use chrono::{SecondsFormat, Utc};
use reqwest::{redirect::Policy, Client};
use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Instant};
use url::Url;
use wireassume_model::{CorpusStore, Interaction, InteractionMetadata, RedactionPolicy, RequestRecord, ResponseRecord};

#[derive(Debug, Clone)]
pub struct RecordProxyConfig {
    pub listen: SocketAddr,
    pub upstream: Url,
    pub workspace: PathBuf,
    pub provider_id: String,
    pub scenario_id: String,
    pub max_payload_bytes: usize,
    pub redaction: RedactionPolicy,
}

struct RecordState {
    config: RecordProxyConfig,
    client: Client,
    store: CorpusStore,
}

pub async fn serve_record(config: RecordProxyConfig) -> Result<(), String> {
    if config.max_payload_bytes == 0 {
        return Err("max_payload_bytes must be greater than zero".into());
    }
    let client = Client::builder()
        .redirect(Policy::none())
        .build()
        .map_err(|error| format!("failed to build upstream client: {error}"))?;
    let state = Arc::new(RecordState {
        store: CorpusStore::new(&config.workspace),
        config,
        client,
    });
    let listen = state.config.listen;
    let app = Router::new().fallback(any(handler)).with_state(state);
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|error| format!("failed to bind reverse proxy at {listen}: {error}"))?;
    tracing::info!(%listen, "WireAssume record proxy listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|error| format!("record proxy failed: {error}"))
}

async fn handler(State(state): State<Arc<RecordState>>, request: Request<AxumBody>) -> impl IntoResponse {
    match forward_and_record(state, request).await {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}

async fn forward_and_record(state: Arc<RecordState>, request: Request<AxumBody>) -> Result<Response<AxumBody>, ProxyFailure> {
    let (parts, body) = request.into_parts();
    let request_bytes = to_bytes(body, state.config.max_payload_bytes)
        .await
        .map_err(|error| ProxyFailure::new(StatusCode::PAYLOAD_TOO_LARGE, format!("request body exceeded configured limit or could not be read: {error}")))?;

    let mut upstream = state.config.upstream.clone();
    upstream.set_path(parts.uri.path());
    upstream.set_query(parts.uri.query());

    let mut upstream_request = state.client.request(parts.method.clone(), upstream.clone());
    for (name, value) in &parts.headers {
        if !is_hop_by_hop_header(name) {
            upstream_request = upstream_request.header(name, value);
        }
    }

    let started = Instant::now();
    let upstream_response = upstream_request
        .body(request_bytes.clone())
        .send()
        .await
        .map_err(|error| ProxyFailure::new(StatusCode::BAD_GATEWAY, format!("upstream request to {upstream} failed: {error}")))?;
    let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let status = upstream_response.status();
    let response_headers = upstream_response.headers().clone();
    let response_bytes = upstream_response
        .bytes()
        .await
        .map_err(|error| ProxyFailure::new(StatusCode::BAD_GATEWAY, format!("failed to read upstream response: {error}")))?;
    if response_bytes.len() > state.config.max_payload_bytes {
        return Err(ProxyFailure::new(StatusCode::PAYLOAD_TOO_LARGE, format!("upstream response body {} bytes exceeds configured limit {}", response_bytes.len(), state.config.max_payload_bytes)));
    }

    let request_content_type = parts.headers.get("content-type").and_then(|v| v.to_str().ok());
    let response_content_type = response_headers.get("content-type").and_then(|v| v.to_str().ok());
    let interaction = Interaction {
        request: RequestRecord {
            method: parts.method.to_string(),
            uri: upstream.to_string(),
            headers: headers_to_model(&parts.headers),
            body: body_to_model(&request_bytes, request_content_type),
        },
        response: ResponseRecord {
            status: status.as_u16(),
            headers: headers_to_model(&response_headers),
            body: body_to_model(&response_bytes, response_content_type),
        },
        metadata: InteractionMetadata {
            captured_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            duration_ms,
            provider_id: state.config.provider_id.clone(),
            scenario_id: state.config.scenario_id.clone(),
            correlation_id: None,
        },
    };
    let persisted = state.store.persist(&interaction, &state.config.redaction)
        .map_err(|error| ProxyFailure::new(StatusCode::INTERNAL_SERVER_ERROR, format!("upstream responded but recording failed; response withheld to avoid missing evidence: {error}")))?;
    tracing::info!(interaction_id = %persisted.id, status = status.as_u16(), duration_ms, "recorded provider interaction");

    let mut builder = Response::builder().status(status);
    for (name, value) in &response_headers {
        if !is_hop_by_hop_header(name) {
            builder = builder.header(name, value);
        }
    }
    builder.body(AxumBody::from(response_bytes))
        .map_err(|error| ProxyFailure::new(StatusCode::INTERNAL_SERVER_ERROR, format!("failed to construct proxy response: {error}")))
}

struct ProxyFailure {
    status: StatusCode,
    message: String,
}

impl ProxyFailure {
    fn new(status: StatusCode, message: String) -> Self { Self { status, message } }
}

impl IntoResponse for ProxyFailure {
    fn into_response(self) -> axum::response::Response {
        (self.status, self.message).into_response()
    }
}

async fn shutdown_signal() {
    let ctrl_c = async { let _ = tokio::signal::ctrl_c().await; };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_url_uses_configured_host_not_incoming_host() {
        let mut upstream = Url::parse("https://allowed.example/base").unwrap();
        let uri: axum::http::Uri = "/customers/1?page=2".parse().unwrap();
        upstream.set_path(uri.path());
        upstream.set_query(uri.query());
        assert_eq!(upstream.as_str(), "https://allowed.example/customers/1?page=2");
    }
}
