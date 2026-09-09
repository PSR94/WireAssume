use crate::mutation::mutate_at_pointer;
use crate::{Mutation, MutationKind, Mutator};
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Default)]
pub struct ArrayMutator;

impl Mutator for ArrayMutator {
    fn name(&self) -> &'static str {
        "arrays"
    }

    fn supported_kinds(&self) -> BTreeSet<MutationKind> {
        use MutationKind::*;
        [ReverseArray, ShuffleArray, DuplicateArrayItem, RemoveFirstArrayItem, RemoveLastArrayItem, EmptyArray, OneArrayItem, RepeatedArrayItems]
            .into_iter()
            .collect()
    }

    fn generate(&self, baseline: &Value, seed: u64) -> Vec<Mutation> {
        let mut out = Vec::new();
        walk(baseline, baseline, "", seed, &mut out);
        let mut seen = HashSet::new();
        out.into_iter()
            .filter(|mutation| mutation.mutated != *baseline)
            .filter(|mutation| seen.insert(mutation.id.clone()))
            .collect()
    }
}

fn walk(root: &Value, current: &Value, pointer: &str, seed: u64, out: &mut Vec<Mutation>) {
    match current {
        Value::Array(items) => {
            if !items.is_empty() {
                add(out, root, pointer, seed, MutationKind::EmptyArray, "empty array", "empty", |_| vec![]);
                add(out, root, pointer, seed, MutationKind::OneArrayItem, "keep one array item", "one", |items| vec![items[0].clone()]);
                add(out, root, pointer, seed, MutationKind::DuplicateArrayItem, "duplicate first array item", "duplicate-first", |items| {
                    let mut next = items.to_vec();
                    next.insert(1.min(next.len()), items[0].clone());
                    next
                });
                add(out, root, pointer, seed, MutationKind::RemoveFirstArrayItem, "remove first array item", "remove-first", |items| items[1..].to_vec());
                add(out, root, pointer, seed, MutationKind::RemoveLastArrayItem, "remove last array item", "remove-last", |items| items[..items.len() - 1].to_vec());
                add(out, root, pointer, seed, MutationKind::RepeatedArrayItems, "repeat first array item", "repeat-first", |items| vec![items[0].clone(); items.len().max(2)]);
            }
            if items.len() > 1 {
                add(out, root, pointer, seed, MutationKind::ReverseArray, "reverse array order", "reverse", |items| {
                    let mut next = items.to_vec();
                    next.reverse();
                    next
                });
                let path_seed = derive_seed(seed, pointer);
                add(out, root, pointer, seed, MutationKind::ShuffleArray, "deterministically shuffle array", &format!("shuffle-{path_seed}"), |items| {
                    let mut next = items.to_vec();
                    let mut rng = ChaCha8Rng::seed_from_u64(path_seed);
                    next.shuffle(&mut rng);
                    next
                });
            }
            for (index, child) in items.iter().enumerate() {
                walk(root, child, &format!("{pointer}/{index}"), seed, out);
            }
        }
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            for key in keys {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                walk(root, &map[key], &format!("{pointer}/{escaped}"), seed, out);
            }
        }
        _ => {}
    }
}

fn add<F>(out: &mut Vec<Mutation>, root: &Value, pointer: &str, seed: u64, kind: MutationKind, description: &str, variant: &str, transform: F)
where
    F: FnOnce(&[Value]) -> Vec<Value>,
{
    if let Some(mutated) = mutate_at_pointer(root, pointer, |target| {
        if let Value::Array(items) = target {
            let next = transform(items);
            *items = next;
        }
    }) {
        out.push(Mutation::new(kind, pointer, description, seed, variant, mutated));
    }
}

fn derive_seed(seed: u64, pointer: &str) -> u64 {
    let mut hash = Sha256::new();
    hash.update(seed.to_be_bytes());
    hash.update(pointer.as_bytes());
    let digest = hash.finalize();
    u64::from_be_bytes(digest[..8].try_into().unwrap())
}
