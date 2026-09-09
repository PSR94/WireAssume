use crate::{Mutation, MutationKind, Mutator};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerConfig {
    pub budget: usize,
    pub concurrency: usize,
    pub seed: u64,
    pub retry_limit: usize,
    #[serde(default)]
    pub enabled: BTreeSet<MutationKind>,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            budget: 500,
            concurrency: 4,
            seed: 42,
            retry_limit: 1,
            enabled: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationPlan {
    pub seed: u64,
    pub budget: usize,
    pub generated: usize,
    pub selected: Vec<Mutation>,
}

pub struct MutationPlanner {
    mutators: Vec<Box<dyn Mutator>>,
}

impl MutationPlanner {
    pub fn new(mutators: Vec<Box<dyn Mutator>>) -> Self {
        Self { mutators }
    }

    pub fn plan(&self, baseline: &Value, config: &PlannerConfig, already_tested: &HashSet<String>) -> MutationPlan {
        let mut all: Vec<Mutation> = self
            .mutators
            .iter()
            .flat_map(|mutator| mutator.generate(baseline, config.seed))
            .filter(|mutation| config.enabled.is_empty() || config.enabled.contains(&mutation.kind))
            .filter(|mutation| !already_tested.contains(&mutation.id))
            .collect();

        all.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.kind.cmp(&b.kind))
                .then(a.id.cmp(&b.id))
        });
        all.dedup_by(|a, b| a.id == b.id);

        let generated = all.len();
        all.truncate(config.budget);
        MutationPlan {
            seed: config.seed,
            budget: config.budget,
            generated,
            selected: all,
        }
    }
}
