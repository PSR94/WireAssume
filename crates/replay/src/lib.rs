//! Deterministic lookup of recorded responses for replay.

use std::path::PathBuf;
use thiserror::Error;
use url::Url;
use wireassume_model::{CorpusError, CorpusStore, Interaction};

#[derive(Debug, Clone)]
pub struct ReplayEntry {
    pub interaction_id: String,
    pub interaction: Interaction,
}

#[derive(Debug, Clone)]
pub struct ReplayIndex {
    entries: Vec<ReplayEntry>,
}

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error(transparent)]
    Corpus(#[from] CorpusError),
}

impl ReplayIndex {
    pub fn load(root: impl Into<PathBuf>) -> Result<Self, ReplayError> {
        let store = CorpusStore::new(root);
        let mut entries = Vec::new();
        for id in store.ids()? {
            entries.push(ReplayEntry {
                interaction_id: id.clone(),
                interaction: store.load(&id)?,
            });
        }
        entries.sort_by(|a, b| a.interaction_id.cmp(&b.interaction_id));
        Ok(Self { entries })
    }

    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    /// v0.1 matching is intentionally deterministic and conservative: exact method plus path/query.
    /// Request bodies are not ignored by a claim of semantic matching; body-aware matchers are a later extension.
    pub fn find(&self, method: &str, uri: &str) -> Option<&ReplayEntry> {
        let target = request_target(uri)?;
        self.entries.iter().find(|entry| {
            entry.interaction.request.method.eq_ignore_ascii_case(method)
                && request_target(&entry.interaction.request.uri).as_deref() == Some(target.as_str())
        })
    }
}

pub fn request_target(uri: &str) -> Option<String> {
    if let Ok(url) = Url::parse(uri) {
        let mut target = url.path().to_string();
        if let Some(query) = url.query() {
            target.push('?');
            target.push_str(query);
        }
        return Some(target);
    }
    if uri.starts_with('/') {
        return Some(uri.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_ignores_host_but_preserves_query() {
        assert_eq!(request_target("https://api.example.test/a?b=1").as_deref(), Some("/a?b=1"));
        assert_eq!(request_target("/a?b=1").as_deref(), Some("/a?b=1"));
    }
}
