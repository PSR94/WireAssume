use crate::wire::{is_hop_by_hop_header, model_to_bytes};
use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
    routing::any,
    Router,
};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use wireassume_replay::ReplayIndex;

#[derive(Debug, Clone)]
pub struct ReplayServerConfig {
    pub listen: SocketAddr,
    pub workspace: PathBuf,
}

pub async fn serve_replay(config: ReplayServerConfig) -> Result<(), String> {
    let index = ReplayIndex::load(&config.workspace).map_err(|error| error.to_string())?;
    if index.is_empty() {
        return Err(format!(
            "no recorded interactions found under {}",
            config.workspace.display()
        ));
    }
    let state = Arc::new(index);
    let app = Router::new().fallback(any(handler)).with_state(state);
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .map_err(|error| format!("failed to bind replay server at {}: {error}", config.listen))?;
    tracing::info!(listen = %config.listen, "WireAssume replay server listening");
    axum::serve(listener, app)
        .await
        .map_err(|error| format!("replay server failed: {error}"))
}

async fn handler(
    State(index): State<Arc<ReplayIndex>>,
    request: Request<Body>,
) -> impl IntoResponse {
    let method = request.method().to_string();
    let uri = request.uri().to_string();
    let Some(entry) = index.find(&method, &uri) else {
        return (
            StatusCode::NOT_FOUND,
            format!("no recorded interaction matches {method} {uri}"),
        )
            .into_response();
    };
    let bytes = match model_to_bytes(&entry.interaction.response.body) {
        Ok(bytes) => bytes,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "recorded body {} could not be decoded: {error}",
                    entry.interaction_id
                ),
            )
                .into_response()
        }
    };
    let status = match StatusCode::from_u16(entry.interaction.response.status) {
        Ok(status) => status,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("recorded status is invalid: {error}"),
            )
                .into_response()
        }
    };
    let mut builder = Response::builder().status(status);
    for header in &entry.interaction.response.headers {
        let Ok(name) = axum::http::HeaderName::try_from(header.name.as_str()) else {
            continue;
        };
        let Ok(value) = axum::http::HeaderValue::try_from(header.value.as_str()) else {
            continue;
        };
        if !is_hop_by_hop_header(&name) {
            builder = builder.header(name, value);
        }
    }
    match builder.body(Body::from(bytes)) {
        Ok(response) => response.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to construct replay response: {error}"),
        )
            .into_response(),
    }
}
