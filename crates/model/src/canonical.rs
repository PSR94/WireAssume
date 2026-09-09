use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Serialize JSON with recursively sorted object keys and no insignificant whitespace.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let value = serde_json::to_value(value)?;
    serde_json::to_vec(&sort_value(value))
}

/// Return a lowercase SHA-256 digest of canonical JSON.
pub fn stable_hash<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let bytes = canonical_json(value)?;
    let mut digest = Sha256::new();
    digest.update(bytes);
    Ok(hex::encode(digest.finalize()))
}

/// Create a compact deterministic identifier from canonical content.
pub fn stable_id<T: Serialize>(prefix: &str, value: &T) -> Result<String, serde_json::Error> {
    let hash = stable_hash(value)?;
    Ok(format!("{prefix}_{}", &hash[..20]))
}

fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.into_iter().collect();
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            let mut sorted = Map::new();
            for (key, value) in entries {
                sorted.insert(key, sort_value(value));
            }
            Value::Object(sorted)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(sort_value).collect()),
        scalar => scalar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonicalization_is_independent_of_object_insertion_order() {
        let a = json!({"b": 2, "a": {"z": 1, "y": 2}});
        let b = json!({"a": {"y": 2, "z": 1}, "b": 2});
        assert_eq!(canonical_json(&a).unwrap(), canonical_json(&b).unwrap());
        assert_eq!(stable_hash(&a).unwrap(), stable_hash(&b).unwrap());
    }
}
