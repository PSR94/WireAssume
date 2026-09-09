//! Stable data model and traffic-corpus persistence for WireAssume.

mod canonical;
mod corpus;
mod redaction;

pub use canonical::{canonical_json, stable_hash, stable_id};
pub use corpus::{
    Body, CorpusError, CorpusStore, Header, Interaction, InteractionMetadata, PersistedInteraction,
    RequestRecord, ResponseRecord,
};
pub use redaction::{RedactionPolicy, REDACTED};
