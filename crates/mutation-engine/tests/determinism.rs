use serde_json::json;
use std::collections::HashSet;
use wireassume_mutation_engine::{
    ArrayMutator, JsonMutator, MutationKind, MutationPlanner, PlannerConfig,
};

fn planner() -> MutationPlanner {
    MutationPlanner::new(vec![Box::new(JsonMutator), Box::new(ArrayMutator)])
}

#[test]
fn same_seed_produces_same_mutation_ids_and_payloads() {
    let baseline = json!({
        "email": "alice@example.com",
        "age": 42,
        "customers": [{"id": "a"}, {"id": "b"}, {"id": "c"}]
    });
    let config = PlannerConfig {
        budget: 200,
        seed: 42,
        ..PlannerConfig::default()
    };
    let first = planner().plan(&baseline, &config, &HashSet::new());
    let second = planner().plan(&baseline, &config, &HashSet::new());
    assert_eq!(first.selected, second.selected);
}

#[test]
fn removal_nullability_type_and_order_mutations_are_real_payload_changes() {
    let baseline = json!({"email": "alice@example.com", "items": [1, 2, 3]});
    let config = PlannerConfig {
        budget: 200,
        seed: 7,
        ..PlannerConfig::default()
    };
    let plan = planner().plan(&baseline, &config, &HashSet::new());

    for kind in [
        MutationKind::RemoveField,
        MutationKind::NullField,
        MutationKind::WrongPrimitiveType,
        MutationKind::ReverseArray,
    ] {
        let mutation = plan
            .selected
            .iter()
            .find(|mutation| mutation.kind == kind)
            .expect("required mutation family");
        assert_ne!(mutation.mutated, baseline);
    }
}

#[test]
fn planner_budget_and_seen_ids_prevent_reexecution() {
    let baseline = json!({"a": "x", "b": "y"});
    let config = PlannerConfig {
        budget: 3,
        ..PlannerConfig::default()
    };
    let first = planner().plan(&baseline, &config, &HashSet::new());
    assert_eq!(first.selected.len(), 3);
    let seen = first.selected.iter().map(|m| m.id.clone()).collect();
    let second = planner().plan(&baseline, &config, &seen);
    assert!(second
        .selected
        .iter()
        .all(|m| !first.selected.iter().any(|old| old.id == m.id)));
}
