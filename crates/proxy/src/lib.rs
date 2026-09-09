//! Reverse-proxy recording and deterministic local replay.

mod record;
mod replay_server;
mod wire;

pub use record::{serve_record, RecordProxyConfig};
pub use replay_server::{serve_replay, ReplayServerConfig};
