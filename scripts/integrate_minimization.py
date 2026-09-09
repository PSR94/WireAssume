#!/usr/bin/env python3
from pathlib import Path


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text()
    if old in text:
        path.write_text(text.replace(old, new, 1))
        return
    if new in text:
        return
    raise SystemExit(f"{label} marker missing in {path}")


cargo = Path("crates/experiment/Cargo.toml")
replace_once(
    cargo,
    'wireassume-model = { path = "../model" }\n',
    'wireassume-model = { path = "../model" }\nwireassume-delta-debugger = { path = "../delta-debugger" }\n',
    "experiment delta-debugger dependency",
)

minimize = Path("crates/experiment/src/minimize.rs")
replace_once(
    minimize,
    '    let id = stable_id("min", &fingerprint).ok()?;\n',
    '    let id = stable_id("min", &fingerprint)\n        .expect("minimization fingerprint contains only serializable values");\n',
    "minimization stable id",
)

experiment = Path("crates/experiment/src/lib.rs")
text = experiment.read_text()
module_marker = 'use wireassume_proxy::{ExperimentReplayController, ResponseOverride};\n'
module_new = module_marker + '\nmod minimize;\npub use minimize::{MinimizationArtifact, MinimizationTrial, ResponseMinimization};\n'
if 'mod minimize;' not in text:
    if module_marker not in text:
        raise SystemExit("experiment minimize module marker missing")
    text = text.replace(module_marker, module_new, 1)
report_old = '''    pub inconclusive: usize,
    pub results: Vec<TrialResult>,
}'''
report_new = '''    pub inconclusive: usize,
    pub results: Vec<TrialResult>,
    #[serde(default)]
    pub minimizations: Vec<ResponseMinimization>,
}'''
if report_old in text:
    text = text.replace(report_old, report_new, 1)
elif report_new not in text:
    raise SystemExit("experiment report marker missing")

counts_old = '''        let passes = results
            .iter()
            .filter(|result| result.outcome == TrialOutcome::Pass)
            .count();'''
counts_new = '''        let store = CorpusStore::new(&self.workspace);
        let interaction_ids: BTreeSet<_> = cases
            .iter()
            .map(|case| case.interaction_id.clone())
            .collect();
        let mut minimizations = Vec::new();
        for interaction_id in interaction_ids {
            let interaction = store.load(&interaction_id)?;
            if let Some((mut summary, artifact)) = minimize::minimize_response_fields(
                &interaction_id,
                &interaction.response,
                self.controller.clone(),
                self.oracle.clone(),
            )
            .await
            {
                let artifact_name = format!("minimization-{}.json", summary.id);
                let artifact_ref = persist_run_artifact(
                    &self.workspace,
                    &run_id,
                    &artifact_name,
                    &artifact,
                )?;
                summary.artifact_ref = artifact_ref;
                minimizations.push(summary);
            }
        }
        minimizations.sort_by(|a, b| a.interaction_id.cmp(&b.interaction_id));

        let passes = results
            .iter()
            .filter(|result| result.outcome == TrialOutcome::Pass)
            .count();'''
if counts_old in text:
    text = text.replace(counts_old, counts_new, 1)
elif counts_new not in text:
    raise SystemExit("experiment minimization execution marker missing")

report_build_old = '''            inconclusive,
            results,
        };'''
report_build_new = '''            inconclusive,
            results,
            minimizations,
        };'''
if report_build_old in text:
    text = text.replace(report_build_old, report_build_new, 1)
elif report_build_new not in text:
    raise SystemExit("experiment report construction marker missing")
experiment.write_text(text)

contract = Path("crates/contract-engine/src/lib.rs")
replace_once(
    contract,
    '''            inconclusive: results.len() - passes - failures,
            results,
        }''',
    '''            inconclusive: results.len() - passes - failures,
            results,
            minimizations: vec![],
        }''',
    "contract test report minimizations",
)

cli = Path("crates/cli/src/main.rs")
replace_once(
    cli,
    '''        "\\nNot yet claimed by this command: integrated delta minimization, browser step DSL, backend API, or dashboard services."
    );''',
    '''        "\\nNot yet claimed by this command: browser step DSL, backend API, or dashboard services."
    );''',
    "doctor minimization capability",
)
replace_once(
    cli,
    '''    println!("Mutations executed: {}", report.executed_mutations);
    println!("Consumer failures: {}", report.failures);''',
    '''    println!("Mutations executed: {}", report.executed_mutations);
    println!("Response minimizations: {}", report.minimizations.len());
    for minimization in &report.minimizations {
        let fields = if minimization.minimal_fields.is_empty() {
            "<none>".to_string()
        } else {
            minimization.minimal_fields.join(", ")
        };
        println!(
            "  {}: {} → {} fields; minimal [{}]; tests {}; established={}",
            minimization.interaction_id,
            minimization.original_fields.len(),
            minimization.minimal_fields.len(),
            fields,
            minimization.tests_executed,
            minimization.established
        );
    }
    println!("Consumer failures: {}", report.failures);''',
    "CLI minimization output",
)

verify = Path("demo/verify_demo.py")
replace_once(
    verify,
    'import json\nimport sys\n',
    'import json\nimport os\nimport sys\n',
    "demo os import",
)
replace_once(
    verify,
    '''    if report["passes"] < 1:
        return fail("expected at least one harmless mutation to pass")

    by_target = {item["target"]["path"]: item for item in lock["requirements"]}''',
    '''    if report["passes"] < 1:
        return fail("expected at least one harmless mutation to pass")

    minimizations = [item for item in report.get("minimizations", []) if item.get("established")]
    if len(minimizations) != 1:
        return fail(f"expected one established response minimization, found {len(minimizations)}")
    minimized = minimizations[0]
    if minimized.get("minimal_fields") != ["email"]:
        return fail(
            f"minimal successful response fields were {minimized.get('minimal_fields')!r}, "
            "expected ['email']"
        )
    if minimized.get("tests_executed", 0) < 2:
        return fail("response minimization did not execute enough real oracle trials")
    artifact_ref = minimized.get("artifact_ref")
    if not artifact_ref or not os.path.exists(os.path.join(".wireassume", artifact_ref)):
        return fail("response minimization evidence artifact was not persisted")

    by_target = {item["target"]["path"]: item for item in lock["requirements"]}''',
    "demo minimization verification",
)
replace_once(
    verify,
    '''        "presence + non-nullability while tolerating empty strings; OpenAPI marks "
        "the dependency optional + nullable"''',
    '''        "presence + non-nullability while tolerating empty strings; async ddmin reduces "
        "the successful response to {email}; OpenAPI marks the dependency optional + nullable"''',
    "demo minimization success message",
)

readme = Path("demo/README.md")
replace_once(
    readme,
    '''- inferred email requirement: present, non-null, empty accepted

`wireassume analyze` also compares''',
    '''- inferred email requirement: present, non-null, empty accepted
- async ddmin minimal successful response field set: `email`

`wireassume analyze` also compares''',
    "demo README minimization",
)

status = Path("docs/project-status.md")
replace_once(
    status,
    '| Delta debugger | Implemented component | ddmin-family synchronous and asynchronous reducers exist and are unit tested. Full integration into `analyze` is still partial. |',
    '| Delta debugger | Integrated | ddmin-family synchronous/asynchronous reducers exist; `analyze` now performs async success-preserving response-field minimization through the real consumer oracle and persists the candidate-trial evidence. |',
    "project status delta debugger",
)
replace_once(
    status,
    '| `wireassume analyze` | Partial integration | Runs evidence-backed controlled mutation experiments and emits contract/report outputs. Integrated ddmin and provider-spec comparison are still required for the target vertical slice. |',
    '| `wireassume analyze` | Implemented vertical slice | Runs evidence-backed controlled mutations, async response-field minimization, deterministic inference, configured OpenAPI guarantee comparison, and lock/report output for the OrbitDesk/PeopleCRM scenario. Broader protocol coverage remains roadmap. |',
    "project status analyze",
)
