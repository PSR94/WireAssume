//! Reverse-proxy recording, deterministic replay, and controlled experiment replay.

mod experiment_server;
mod record;
mod replay_server;
mod wire;

pub use experiment_server::{
    start_experiment_replay, ExperimentReplayConfig, ExperimentReplayController,
    ExperimentReplayHandle, ResponseOverride,
};
pub use record::{serve_record, RecordProxyConfig};
pub use replay_server::{serve_replay, ReplayServerConfig};
