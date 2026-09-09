use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use wireassume_model::stable_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MutationKind {
    RemoveField,
    NullField,
    EmptyString,
    WhitespaceString,
    WrongPrimitiveType,
    EmptyArray,
    EmptyObject,
    DuplicateValue,
    UnknownEnum,
    NumericZero,
    NegativeNumber,
    VeryLargeNumber,
    FloatInsteadOfInteger,
    UnicodeString,
    LongString,
    AdditionalUnknownProperty,
    ReverseArray,
    ShuffleArray,
    DuplicateArrayItem,
    RemoveFirstArrayItem,
    RemoveLastArrayItem,
    OneArrayItem,
    RepeatedArrayItems,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationFingerprint {
    pub kind: MutationKind,
    pub path: String,
    pub seed: u64,
    pub variant: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mutation {
    pub id: String,
    pub kind: MutationKind,
    /// RFC 6901 JSON Pointer. The empty string refers to the root.
    pub path: String,
    pub description: String,
    pub seed: u64,
    pub mutated: Value,
}

impl Mutation {
    pub fn new(
        kind: MutationKind,
        path: impl Into<String>,
        description: impl Into<String>,
        seed: u64,
        variant: impl Into<String>,
        mutated: Value,
    ) -> Self {
        let path = path.into();
        let fingerprint = MutationFingerprint {
            kind,
            path: path.clone(),
            seed,
            variant: variant.into(),
        };
        let id = stable_id("mut", &fingerprint).expect("mutation fingerprint is serializable");
        Self {
            id,
            kind,
            path,
            description: description.into(),
            seed,
            mutated,
        }
    }
}

pub trait Mutator: Send + Sync {
    fn name(&self) -> &'static str;
    fn supported_kinds(&self) -> BTreeSet<MutationKind>;
    fn generate(&self, baseline: &Value, seed: u64) -> Vec<Mutation>;
}

pub(crate) fn escape_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

pub(crate) fn mutate_at_pointer<F>(baseline: &Value, pointer: &str, f: F) -> Option<Value>
where
    F: FnOnce(&mut Value),
{
    let mut candidate = baseline.clone();
    if pointer.is_empty() {
        f(&mut candidate);
        return Some(candidate);
    }
    let target = candidate.pointer_mut(pointer)?;
    f(target);
    Some(candidate)
}

pub(crate) fn remove_at_pointer(baseline: &Value, pointer: &str) -> Option<Value> {
    let (parent_pointer, key) = pointer.rsplit_once('/')?;
    let key = key.replace("~1", "/").replace("~0", "~");
    let mut candidate = baseline.clone();
    let parent = if parent_pointer.is_empty() {
        &mut candidate
    } else {
        candidate.pointer_mut(parent_pointer)?
    };
    match parent {
        Value::Object(map) => {
            map.remove(&key)?;
            Some(candidate)
        }
        Value::Array(items) => {
            let index: usize = key.parse().ok()?;
            if index >= items.len() {
                None
            } else {
                items.remove(index);
                Some(candidate)
            }
        }
        _ => None,
    }
}
