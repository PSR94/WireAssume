use crate::{canonical_json, stable_hash, stable_id, RedactionPolicy};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum Body {
    Empty,
    Json(Value),
    Text(String),
    Base64(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestRecord {
    pub method: String,
    pub uri: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseRecord {
    pub status: u16,
    #[serde(default)]
    pub headers: Vec<Header>,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionMetadata {
    pub captured_at: String,
    pub duration_ms: u64,
    pub provider_id: String,
    pub scenario_id: String,
    #[serde(default)]
    pub correlation_id: Option<String>,
}

impl InteractionMetadata {
    /// Stable timestamps are useful in tests and generated demo fixtures.
    pub fn fixture(provider_id: impl Into<String>, scenario_id: impl Into<String>) -> Self {
        Self {
            captured_at: "1970-01-01T00:00:00Z".into(),
            duration_ms: 0,
            provider_id: provider_id.into(),
            scenario_id: scenario_id.into(),
            correlation_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Interaction {
    pub request: RequestRecord,
    pub response: ResponseRecord,
    pub metadata: InteractionMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedInteraction {
    pub id: String,
    pub content_sha256: String,
    pub directory: PathBuf,
}

#[derive(Debug, Error)]
pub enum CorpusError {
    #[error("failed to serialize or deserialize traffic artifact: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to access traffic artifact: {0}")]
    Io(#[from] io::Error),
    #[error("invalid interaction id {0:?}")]
    InvalidId(String),
}

#[derive(Debug, Clone)]
pub struct CorpusStore {
    root: PathBuf,
}

impl CorpusStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn persist(
        &self,
        interaction: &Interaction,
        redaction: &RedactionPolicy,
    ) -> Result<PersistedInteraction, CorpusError> {
        let mut interaction = interaction.clone();
        redaction.apply(&mut interaction);

        let content_sha256 = stable_hash(&interaction)?;
        let id = stable_id("int", &interaction)?;
        let directory = self.root.join("corpus").join(&id);
        fs::create_dir_all(&directory)?;

        write_atomic(
            &directory.join("request.json"),
            &canonical_json(&interaction.request)?,
        )?;
        write_atomic(
            &directory.join("response.json"),
            &canonical_json(&interaction.response)?,
        )?;
        write_atomic(
            &directory.join("metadata.json"),
            &canonical_json(&interaction.metadata)?,
        )?;

        Ok(PersistedInteraction {
            id,
            content_sha256,
            directory,
        })
    }

    pub fn load(&self, id: &str) -> Result<Interaction, CorpusError> {
        validate_id(id)?;
        let directory = self.root.join("corpus").join(id);
        let request = serde_json::from_slice(&fs::read(directory.join("request.json"))?)?;
        let response = serde_json::from_slice(&fs::read(directory.join("response.json"))?)?;
        let metadata = serde_json::from_slice(&fs::read(directory.join("metadata.json"))?)?;
        Ok(Interaction {
            request,
            response,
            metadata,
        })
    }

    pub fn ids(&self) -> Result<Vec<String>, CorpusError> {
        let directory = self.root.join("corpus");
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            if validate_id(&id).is_ok() {
                ids.push(id);
            }
        }
        ids.sort();
        Ok(ids)
    }
}

fn validate_id(id: &str) -> Result<(), CorpusError> {
    if id.starts_with("int_")
        && id.len() <= 128
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        Ok(())
    } else {
        Err(CorpusError::InvalidId(id.to_string()))
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(tmp, path)
}
