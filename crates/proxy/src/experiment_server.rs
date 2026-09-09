use crate::wire::{is_hop_by_hop_header, model_to_bytes};
use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
    routing::any,
    Router,
};
use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    sync::{oneshot, RwLock},
    task::JoinHandle,
};
use wireassume_model::ResponseRecord;
use wireassume_replay::ReplayIndex;

#[derive(Debug, Clone)]
pub struct ExperimentReplayConfig {
    pub listen: SocketAddr,
    pub workspace: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ResponseOverride {
    pub interaction_id: String,
    pub response: ResponseRecord,
    pub delay_ms: u64,
}

#[derive(Clone)]
pub struct ExperimentReplayController {
    active: Arc<RwLock<Option<ResponseOverride>>>,
}

impl ExperimentReplayController {
    pub async fn apply(&self, response_override: ResponseOverride) {
        *self.active.write().await = Some(response_override);
    }

    pub async fn reset(&self) {
        *self.active.write().await = None;
    }

    pub async fn active(&self) -> Option<ResponseOverride> {
        self.active.read().await.clone()
    }
}

struct ExperimentState {
    index: ReplayIndex,
    active: Arc<RwLock<Option<ResponseOverride>>>,
}

pub struct ExperimentReplayHandle {
    pub listen: SocketAddr,
    pub controller: ExperimentReplayController,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<Result<(), String>>,
}

impl ExperimentReplayHandle {
    pub async fn shutdown(mut self) -> Result<(), String> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.task
            .await
            .map_err(|error| format!("experiment replay task failed to join: {error}"))?
    }
}

pub async fn start_experiment_replay(
    config: ExperimentReplayConfig,
) -> Result<ExperimentReplayHandle, String> {
    let index = ReplayIndex::load(&config.workspace).map_err(|error| error.to_string())?;
    if index.is_empty() {
        return Err(format!(
            "no recorded interactions found under {}",
            config.workspace.display()
        ));
    }

    let active = Arc::new(RwLock::new(None));
    let controller = ExperimentReplayController {
        active: Arc::clone(&active),
    };
    let state = Arc::new(ExperimentState { index, active });
    let app = Router::new().fallback(any(handler)).with_state(state);
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .map_err(|error| {
            format!(
                "failed to bind experiment replay at {}: {error}",
                config.listen
            )
        })?;
    let listen = listener
        .local_addr()
        .map_err(|error| format!("failed to resolve experiment replay address: {error}"))?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .map_err(|error| format!("experiment replay server failed: {error}"))
    });

    Ok(ExperimentReplayHandle {
        listen,
        controller,
        shutdown: Some(shutdown_tx),
        task,
    })
}

async fn handler(
    State(state): State<Arc<ExperimentState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    let method = request.method().to_string();
    let uri = request.uri().to_string();
    let Some(entry) = state.index.find(&method, &uri) else {
        return (
            StatusCode::NOT_FOUND,
            format!("no recorded interaction matches {method} {uri}"),
        )
            .into_response();
    };

    let active = state.active.read().await.clone();
    let (response_record, delay_ms) = match active {
        Some(active) if active.interaction_id == entry.interaction_id => {
            (active.response, active.delay_ms)
        }
        _ => (entry.interaction.response.clone(), 0),
    };

    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }

    response_from_record(&entry.interaction_id, &response_record)
}

fn response_from_record(interaction_id: &str, record: &ResponseRecord) -> axum::response::Response {
    let bytes = match model_to_bytes(&record.body) {
        Ok(bytes) => bytes,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("recorded body {interaction_id} could not be decoded: {error}"),
            )
                .into_response();
        }
    };
    let status = match StatusCode::from_u16(record.status) {
        Ok(status) => status,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("recorded status is invalid: {error}"),
            )
                .into_response();
        }
    };
    let mut builder = Response::builder().status(status);
    for header in &record.headers {
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
            format!("failed to construct experiment response: {error}"),
        )
            .into_response(),
    }
}
