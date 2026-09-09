use crate::mutation::{escape_pointer_segment, mutate_at_pointer, remove_at_pointer};
use crate::{Mutation, MutationKind, Mutator};
use serde_json::{json, Number, Value};
use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Default)]
pub struct JsonMutator;

impl Mutator for JsonMutator {
    fn name(&self) -> &'static str {
        "json"
    }

    fn supported_kinds(&self) -> BTreeSet<MutationKind> {
        use MutationKind::*;
        [
            RemoveField,
            NullField,
            EmptyString,
            WhitespaceString,
            WrongPrimitiveType,
            EmptyArray,
            EmptyObject,
            UnknownEnum,
            NumericZero,
            NegativeNumber,
            VeryLargeNumber,
            FloatInsteadOfInteger,
            UnicodeString,
            LongString,
            AdditionalUnknownProperty,
        ]
        .into_iter()
        .collect()
    }

    fn generate(&self, baseline: &Value, seed: u64) -> Vec<Mutation> {
        let mut generated = Vec::new();
        walk(baseline, baseline, "", seed, &mut generated);
        dedupe_noops(baseline, generated)
    }
}

fn walk(root: &Value, current: &Value, pointer: &str, seed: u64, out: &mut Vec<Mutation>) {
    if !pointer.is_empty() {
        if let Some(mutated) = remove_at_pointer(root, pointer) {
            push(out, MutationKind::RemoveField, pointer, "remove value", seed, "remove", mutated);
        }
        if !current.is_null() {
            if let Some(mutated) = replace(root, pointer, Value::Null) {
                push(out, MutationKind::NullField, pointer, "replace with null", seed, "null", mutated);
            }
        }
    }

    match current {
        Value::String(_) => {
            scalar(out, root, pointer, seed, MutationKind::EmptyString, Value::String(String::new()), "empty string", "empty");
            scalar(out, root, pointer, seed, MutationKind::WhitespaceString, Value::String(" \t\n".into()), "whitespace string", "whitespace");
            scalar(out, root, pointer, seed, MutationKind::WrongPrimitiveType, json!(0), "replace string with number", "number");
            scalar(out, root, pointer, seed, MutationKind::UnknownEnum, Value::String("__wireassume_unknown__".into()), "unknown enum-like string", "unknown-enum");
            scalar(out, root, pointer, seed, MutationKind::UnicodeString, Value::String("東京-🧪-é".into()), "unicode string", "unicode");
            scalar(out, root, pointer, seed, MutationKind::LongString, Value::String("w".repeat(4096)), "4096-byte-ish long string", "long-4096");
        }
        Value::Number(number) => {
            scalar(out, root, pointer, seed, MutationKind::NumericZero, json!(0), "numeric zero", "zero");
            scalar(out, root, pointer, seed, MutationKind::NegativeNumber, json!(-1), "negative number", "negative-one");
            scalar(out, root, pointer, seed, MutationKind::VeryLargeNumber, json!(9_007_199_254_740_991_i64), "very large integer", "max-safe-integer");
            scalar(out, root, pointer, seed, MutationKind::WrongPrimitiveType, Value::String(number.to_string()), "replace number with string", "string");
            if number.is_i64() || number.is_u64() {
                scalar(out, root, pointer, seed, MutationKind::FloatInsteadOfInteger, Value::Number(Number::from_f64(1.5).unwrap()), "floating point instead of integer", "float-1.5");
            }
        }
        Value::Bool(value) => {
            scalar(out, root, pointer, seed, MutationKind::WrongPrimitiveType, Value::String(value.to_string()), "replace boolean with string", "string");
        }
        Value::Array(items) => {
            if !items.is_empty() {
                scalar(out, root, pointer, seed, MutationKind::EmptyArray, Value::Array(vec![]), "empty array", "empty-array");
            }
            scalar(out, root, pointer, seed, MutationKind::WrongPrimitiveType, Value::Object(Default::default()), "replace array with object", "object");
            for (index, child) in items.iter().enumerate() {
                let child_pointer = format!("{pointer}/{index}");
                walk(root, child, &child_pointer, seed, out);
            }
        }
        Value::Object(map) => {
            if !map.is_empty() {
                scalar(out, root, pointer, seed, MutationKind::EmptyObject, Value::Object(Default::default()), "empty object", "empty-object");
            }
            scalar(out, root, pointer, seed, MutationKind::WrongPrimitiveType, Value::Array(vec![]), "replace object with array", "array");
            if let Some(mutated) = mutate_at_pointer(root, pointer, |value| {
                if let Value::Object(map) = value {
                    map.insert("__wireassume_unknown__".into(), json!(true));
                }
            }) {
                push(out, MutationKind::AdditionalUnknownProperty, pointer, "add unknown property", seed, "unknown-property", mutated);
            }
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            for key in keys {
                let child_pointer = format!("{pointer}/{}", escape_pointer_segment(key));
                walk(root, &map[key], &child_pointer, seed, out);
            }
        }
        Value::Null => {
            scalar(out, root, pointer, seed, MutationKind::WrongPrimitiveType, Value::String("not-null".into()), "replace null with string", "string");
        }
    }
}

fn scalar(out: &mut Vec<Mutation>, root: &Value, pointer: &str, seed: u64, kind: MutationKind, replacement: Value, description: &str, variant: &str) {
    if let Some(mutated) = replace(root, pointer, replacement) {
        push(out, kind, pointer, description, seed, variant, mutated);
    }
}

fn replace(root: &Value, pointer: &str, replacement: Value) -> Option<Value> {
    mutate_at_pointer(root, pointer, |target| *target = replacement)
}

fn push(out: &mut Vec<Mutation>, kind: MutationKind, pointer: &str, description: &str, seed: u64, variant: &str, mutated: Value) {
    out.push(Mutation::new(kind, pointer, description, seed, variant, mutated));
}

fn dedupe_noops(baseline: &Value, mutations: Vec<Mutation>) -> Vec<Mutation> {
    let mut seen = HashSet::new();
    mutations
        .into_iter()
        .filter(|mutation| mutation.mutated != *baseline)
        .filter(|mutation| seen.insert(mutation.id.clone()))
        .collect()
}
