use crate::{Assumption, ConsumptionLock, Requirement};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractDiff {
    pub base_revision: String,
    pub head_revision: String,
    pub added_assumptions: Vec<AssumptionDelta>,
    pub removed_assumptions: Vec<AssumptionDelta>,
    pub requirement_changes: Vec<RequirementDelta>,
    pub provider_comparison_changes: Vec<ProviderComparisonDelta>,
    pub breaking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssumptionDelta {
    pub id: String,
    pub assumption_type: String,
    pub target: String,
    pub behavior: String,
    pub severity: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequirementDelta {
    pub target: String,
    pub base: Option<RequirementShape>,
    pub head: Option<RequirementShape>,
    pub breaking: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequirementShape {
    pub presence: Option<String>,
    pub types: Vec<String>,
    pub nullable: Option<bool>,
    pub accepted_empty: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderComparisonDelta {
    pub assumption_id: String,
    pub target: String,
    pub base: String,
    pub head: String,
}

pub fn diff_contracts(base: &ConsumptionLock, head: &ConsumptionLock) -> ContractDiff {
    let base_assumptions: BTreeMap<_, _> = base
        .assumptions
        .iter()
        .map(|assumption| (assumption.id.as_str(), assumption))
        .collect();
    let head_assumptions: BTreeMap<_, _> = head
        .assumptions
        .iter()
        .map(|assumption| (assumption.id.as_str(), assumption))
        .collect();

    let mut added_assumptions = head_assumptions
        .iter()
        .filter(|(id, _)| !base_assumptions.contains_key(*id))
        .map(|(_, assumption)| assumption_delta(assumption))
        .collect::<Vec<_>>();
    let mut removed_assumptions = base_assumptions
        .iter()
        .filter(|(id, _)| !head_assumptions.contains_key(*id))
        .map(|(_, assumption)| assumption_delta(assumption))
        .collect::<Vec<_>>();
    added_assumptions.sort_by(delta_order);
    removed_assumptions.sort_by(delta_order);

    let base_requirements: BTreeMap<_, _> = base
        .requirements
        .iter()
        .map(|requirement| (requirement.target.path.as_str(), requirement))
        .collect();
    let head_requirements: BTreeMap<_, _> = head
        .requirements
        .iter()
        .map(|requirement| (requirement.target.path.as_str(), requirement))
        .collect();
    let targets: BTreeSet<_> = base_requirements
        .keys()
        .chain(head_requirements.keys())
        .copied()
        .collect();

    let mut requirement_changes = Vec::new();
    for target in targets {
        let base_requirement = base_requirements.get(target).copied();
        let head_requirement = head_requirements.get(target).copied();
        let base_shape = base_requirement.map(requirement_shape);
        let head_shape = head_requirement.map(requirement_shape);
        if base_shape == head_shape {
            continue;
        }
        let (breaking, reason) = classify_requirement_change(base_requirement, head_requirement);
        requirement_changes.push(RequirementDelta {
            target: target.to_string(),
            base: base_shape,
            head: head_shape,
            breaking,
            reason,
        });
    }

    let mut provider_comparison_changes = Vec::new();
    for (id, base_assumption) in &base_assumptions {
        let Some(head_assumption) = head_assumptions.get(id) else {
            continue;
        };
        if base_assumption.provider_comparison != head_assumption.provider_comparison {
            provider_comparison_changes.push(ProviderComparisonDelta {
                assumption_id: (*id).to_string(),
                target: base_assumption.target.path.clone(),
                base: base_assumption.provider_comparison.clone(),
                head: head_assumption.provider_comparison.clone(),
            });
        }
    }
    provider_comparison_changes.sort_by(|a, b| {
        a.target
            .cmp(&b.target)
            .then(a.assumption_id.cmp(&b.assumption_id))
    });

    let breaking = !added_assumptions.is_empty()
        || requirement_changes.iter().any(|change| change.breaking);

    ContractDiff {
        base_revision: base.metadata.source_revision.value.clone(),
        head_revision: head.metadata.source_revision.value.clone(),
        added_assumptions,
        removed_assumptions,
        requirement_changes,
        provider_comparison_changes,
        breaking,
    }
}

fn assumption_delta(assumption: &Assumption) -> AssumptionDelta {
    AssumptionDelta {
        id: assumption.id.clone(),
        assumption_type: assumption.assumption_type.clone(),
        target: assumption.target.path.clone(),
        behavior: assumption.behavior.clone(),
        severity: assumption.severity.clone(),
    }
}

fn delta_order(a: &AssumptionDelta, b: &AssumptionDelta) -> std::cmp::Ordering {
    a.target
        .cmp(&b.target)
        .then(a.assumption_type.cmp(&b.assumption_type))
        .then(a.id.cmp(&b.id))
}

fn requirement_shape(requirement: &Requirement) -> RequirementShape {
    let mut types = requirement.types.clone();
    types.sort();
    types.dedup();
    RequirementShape {
        presence: requirement.presence.clone(),
        types,
        nullable: requirement.nullable,
        accepted_empty: requirement.accepted_empty,
    }
}

fn classify_requirement_change(
    base: Option<&Requirement>,
    head: Option<&Requirement>,
) -> (bool, String) {
    match (base, head) {
        (None, Some(_)) => (true, "new consumer requirement".into()),
        (Some(_), None) => (false, "consumer requirement removed".into()),
        (Some(base), Some(head)) => {
            let mut stricter = Vec::new();
            if base.presence.as_deref() != Some("required")
                && head.presence.as_deref() == Some("required")
            {
                stricter.push("presence became required");
            }
            if base.nullable != Some(false) && head.nullable == Some(false) {
                stricter.push("null became rejected");
            }
            if base.accepted_empty != Some(false) && head.accepted_empty == Some(false) {
                stricter.push("empty value became rejected");
            }
            let base_types: BTreeSet<_> = base.types.iter().collect();
            let head_types: BTreeSet<_> = head.types.iter().collect();
            if !head_types.is_empty() && base_types != head_types {
                stricter.push("accepted/observed type requirement changed");
            }
            if stricter.is_empty() {
                (false, "consumer requirement changed without becoming stricter".into())
            } else {
                (true, stricter.join("; "))
            }
        }
        (None, None) => (false, "no requirement".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Assumption, ContractMetadata, EndpointRef, Party, RevisionRef, ScenarioRef, TargetRef,
    };

    fn lock(revision: &str) -> ConsumptionLock {
        ConsumptionLock {
            schema: crate::CONSUMPTION_SCHEMA_V1.into(),
            metadata: ContractMetadata {
                tool_version: "0.1.0".into(),
                generated_at: "2026-09-10T00:00:00Z".into(),
                seed: 42,
                source_revision: RevisionRef {
                    kind: "git".into(),
                    value: revision.into(),
                },
            },
            provider: Party {
                id: "peoplecrm".into(),
                name: "PeopleCRM".into(),
            },
            consumer: Party {
                id: "orbitdesk".into(),
                name: "OrbitDesk".into(),
            },
            scenario: ScenarioRef {
                id: "profile".into(),
                name: "profile".into(),
                oracle: "command".into(),
            },
            endpoint: EndpointRef {
                method: "GET".into(),
                path: "/customers/*".into(),
            },
            requirements: vec![],
            assumptions: vec![],
            evidence: vec![],
            history: vec![],
        }
    }

    #[test]
    fn new_assumption_is_breaking() {
        let base = lock("base");
        let mut head = lock("head");
        head.assumptions.push(Assumption {
            id: "asm-email".into(),
            assumption_type: "presence".into(),
            target: TargetRef {
                kind: "response-body".into(),
                path: "/email".into(),
            },
            behavior: "value must be present".into(),
            confidence: "observed".into(),
            severity: "high".into(),
            provider_comparison: "undocumented".into(),
            evidence_refs: vec!["ev".into()],
        });
        let diff = diff_contracts(&base, &head);
        assert!(diff.breaking);
        assert_eq!(diff.added_assumptions.len(), 1);
    }

    #[test]
    fn removed_assumption_is_not_breaking() {
        let mut base = lock("base");
        let head = lock("head");
        base.assumptions.push(Assumption {
            id: "asm-email".into(),
            assumption_type: "presence".into(),
            target: TargetRef {
                kind: "response-body".into(),
                path: "/email".into(),
            },
            behavior: "value must be present".into(),
            confidence: "observed".into(),
            severity: "high".into(),
            provider_comparison: "undocumented".into(),
            evidence_refs: vec!["ev".into()],
        });
        let diff = diff_contracts(&base, &head);
        assert!(!diff.breaking);
        assert_eq!(diff.removed_assumptions.len(), 1);
    }

    #[test]
    fn stricter_nullability_is_breaking() {
        let mut base = lock("base");
        let mut head = lock("head");
        let requirement = |nullable| Requirement {
            id: "req-email".into(),
            target: TargetRef {
                kind: "response-body".into(),
                path: "/email".into(),
            },
            presence: Some("required".into()),
            types: vec!["string".into()],
            nullable,
            accepted_empty: Some(true),
            confidence: "observed".into(),
            severity: "high".into(),
            evidence_refs: vec!["ev".into()],
        };
        base.requirements.push(requirement(Some(true)));
        head.requirements.push(requirement(Some(false)));
        let diff = diff_contracts(&base, &head);
        assert!(diff.breaking);
        assert!(diff.requirement_changes[0].breaking);
    }
}
