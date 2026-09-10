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


lib = Path("crates/contract-engine/src/lib.rs")
replace_once(
    lib,
    '''mod html;
mod openapi;
pub use html::to_html;
pub use openapi::{apply_openapi_comparison, OpenApiComparison, ProviderMismatch};''',
    '''mod diff;
mod html;
mod openapi;
mod pact;
pub use diff::{
    diff_contracts, AssumptionDelta, ContractDiff, ProviderComparisonDelta, RequirementDelta,
    RequirementShape,
};
pub use html::to_html;
pub use openapi::{apply_openapi_comparison, OpenApiComparison, ProviderMismatch};
pub use pact::to_pact_json;''',
    "contract engine exports",
)

cli_cargo = Path("crates/cli/Cargo.toml")
replace_once(
    cli_cargo,
    'serde_json.workspace = true\n',
    'serde_json.workspace = true\nserde_yaml.workspace = true\n',
    "CLI serde_yaml dependency",
)

cli = Path("crates/cli/src/main.rs")
replace_once(
    cli,
    '''use wireassume_contract_engine::{
    analyze_contract, apply_openapi_comparison, build_consumption_lock, to_html, to_json,
    to_markdown, to_yaml, ContractContext,
};''',
    '''use wireassume_contract_engine::{
    analyze_contract, apply_openapi_comparison, build_consumption_lock, diff_contracts, to_html,
    to_json, to_markdown, to_pact_json, to_yaml, ConsumptionLock, ContractContext,
};''',
    "CLI contract imports",
)
replace_once(
    cli,
    '''    /// Replay recorded traffic, execute real consumer oracles, and infer consumption.lock.
    Analyze {''',
    '''    /// Compare two consumption.lock files and optionally fail on stricter consumer behavior.
    Diff {
        base: PathBuf,
        head: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        fail_on_breaking: bool,
    },
    /// Export a consumption.lock as a Pact v3 compatibility document.
    ExportPact {
        lock: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// List run-scoped contract history, optionally filtered to one assumption ID.
    History {
        #[arg(long, default_value = ".wireassume")]
        workspace: PathBuf,
        #[arg(long)]
        assumption: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Replay recorded traffic, execute real consumer oracles, and infer consumption.lock.
    Analyze {''',
    "CLI command variants",
)
replace_once(
    cli,
    '''        Command::Analyze {
            scenario,
            source_revision,
        } => analyze(&cli.config, &scenario, source_revision.as_deref()).await,
''',
    '''        Command::Diff {
            base,
            head,
            json,
            fail_on_breaking,
        } => diff_locks(&base, &head, json, fail_on_breaking),
        Command::ExportPact { lock, output } => export_pact(&lock, output.as_deref()),
        Command::History {
            workspace,
            assumption,
            json,
        } => history(&workspace, assumption.as_deref(), json),
        Command::Analyze {
            scenario,
            source_revision,
        } => analyze(&cli.config, &scenario, source_revision.as_deref()).await,
''',
    "CLI command dispatch",
)

history_functions = r'''
fn load_lock(path: &Path) -> Result<ConsumptionLock> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    if path.extension().and_then(|value| value.to_str()) == Some("json") {
        serde_json::from_str(&text).with_context(|| format!("parse JSON lock {}", path.display()))
    } else {
        serde_yaml::from_str(&text).with_context(|| format!("parse YAML lock {}", path.display()))
    }
}

fn diff_locks(base_path: &Path, head_path: &Path, json: bool, fail_on_breaking: bool) -> Result<()> {
    let base = load_lock(base_path)?;
    let head = load_lock(head_path)?;
    let diff = diff_contracts(&base, &head);

    if json {
        println!("{}", serde_json::to_string_pretty(&diff)?);
    } else {
        println!("WireAssume contract diff");
        println!("Base revision: {}", diff.base_revision);
        println!("Head revision: {}", diff.head_revision);
        println!("Breaking: {}", diff.breaking);
        println!("Added assumptions: {}", diff.added_assumptions.len());
        for assumption in &diff.added_assumptions {
            println!(
                "  + {} {} — {} ({})",
                assumption.target, assumption.assumption_type, assumption.behavior, assumption.severity
            );
        }
        println!("Removed assumptions: {}", diff.removed_assumptions.len());
        for assumption in &diff.removed_assumptions {
            println!("  - {} {}", assumption.target, assumption.assumption_type);
        }
        println!("Requirement changes: {}", diff.requirement_changes.len());
        for change in &diff.requirement_changes {
            println!(
                "  {} {} — {}",
                if change.breaking { "!" } else { "~" },
                change.target,
                change.reason
            );
        }
        println!(
            "Provider comparison changes: {}",
            diff.provider_comparison_changes.len()
        );
    }

    if fail_on_breaking && diff.breaking {
        bail!("breaking consumer-contract change detected");
    }
    Ok(())
}

fn export_pact(lock_path: &Path, output: Option<&Path>) -> Result<()> {
    let lock = load_lock(lock_path)?;
    let pact = to_pact_json(&lock)?;
    if let Some(output) = output {
        if let Some(parent) = output.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::write(output, pact)?;
        println!("Wrote Pact compatibility export to {}", output.display());
    } else {
        println!("{pact}");
    }
    Ok(())
}

fn history(workspace: &Path, assumption_filter: Option<&str>, json: bool) -> Result<()> {
    let runs_dir = workspace.join("runs");
    let mut snapshots = Vec::new();
    if runs_dir.exists() {
        for entry in fs::read_dir(&runs_dir)
            .with_context(|| format!("read run directory {}", runs_dir.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path().join("consumption.lock.json");
            if !path.exists() {
                continue;
            }
            let lock = load_lock(&path)?;
            if assumption_filter.is_some_and(|filter| {
                !lock.assumptions.iter().any(|assumption| assumption.id == filter)
            }) {
                continue;
            }
            let mut assumption_ids = lock
                .assumptions
                .iter()
                .map(|assumption| assumption.id.clone())
                .collect::<Vec<_>>();
            assumption_ids.sort();
            snapshots.push(serde_json::json!({
                "run_id": entry.file_name().to_string_lossy(),
                "generated_at": lock.metadata.generated_at,
                "revision": lock.metadata.source_revision,
                "assumption_ids": assumption_ids,
            }));
        }
    }
    snapshots.sort_by(|a, b| {
        a["generated_at"]
            .as_str()
            .cmp(&b["generated_at"].as_str())
            .then(a["run_id"].as_str().cmp(&b["run_id"].as_str()))
    });

    if json {
        println!("{}", serde_json::to_string_pretty(&snapshots)?);
    } else if snapshots.is_empty() {
        println!("No matching run-scoped contract history found.");
    } else {
        for snapshot in &snapshots {
            println!(
                "{}  {}  {} assumptions  {}",
                snapshot["generated_at"].as_str().unwrap_or("unknown-time"),
                snapshot["revision"]["value"].as_str().unwrap_or("unknown-revision"),
                snapshot["assumption_ids"].as_array().map_or(0, Vec::len),
                snapshot["run_id"].as_str().unwrap_or("unknown-run")
            );
        }
        if let Some(assumption) = assumption_filter {
            if let Some(first) = snapshots.first() {
                println!(
                    "First recorded revision containing {assumption}: {}",
                    first["revision"]["value"].as_str().unwrap_or("unknown-revision")
                );
            }
        }
    }
    Ok(())
}

'''
text = cli.read_text()
marker = 'fn scenario_by_name<\'a>(\n'
if history_functions not in text:
    if marker not in text:
        raise SystemExit("CLI helper insertion marker missing")
    text = text.replace(marker, history_functions + marker, 1)
cli.write_text(text)

# Extend the composite action with optional baseline regression enforcement.
action = Path("action.yml")
text = action.read_text()
input_marker = '''  toolchain:
    description: Rust toolchain used to build the WireAssume action binary.
    required: false
    default: 1.98.0
'''
input_replacement = input_marker + '''  baseline-lock:
    description: Optional baseline consumption.lock path relative to working-directory.
    required: false
    default: ""
  fail-on-breaking:
    description: Fail when the generated contract is stricter than baseline-lock.
    required: false
    default: "true"
'''
if "baseline-lock:" not in text:
    if input_marker not in text:
        raise SystemExit("action input marker missing")
    text = text.replace(input_marker, input_replacement, 1)
output_marker = '''  html-report:
    description: Absolute path to the generated self-contained HTML report.
    value: ${{ steps.analyze.outputs.html-report }}
'''
output_replacement = output_marker + '''  diff-report:
    description: Absolute path to generated contract diff JSON when baseline-lock is supplied.
    value: ${{ steps.analyze.outputs.diff-report }}
'''
if "  diff-report:" not in text:
    if output_marker not in text:
        raise SystemExit("action output marker missing")
    text = text.replace(output_marker, output_replacement, 1)
env_marker = '''        WA_SOURCE_REVISION: ${{ inputs.source-revision }}
'''
env_replacement = env_marker + '''        WA_BASELINE_LOCK: ${{ inputs.baseline-lock }}
        WA_FAIL_ON_BREAKING: ${{ inputs.fail-on-breaking }}
'''
if "WA_BASELINE_LOCK" not in text:
    if env_marker not in text:
        raise SystemExit("action env marker missing")
    text = text.replace(env_marker, env_replacement, 1)
artifact_marker = '''        for artifact in "$lockfile" "$markdown_report" "$html_report"; do
          if [[ ! -f "$artifact" ]]; then
            echo "Expected WireAssume artifact was not generated: $artifact" >&2
            exit 4
          fi
        done

        {
          echo "run-id=$run_id"
          echo "report-directory=$report_directory"
          echo "lockfile=$lockfile"
          echo "markdown-report=$markdown_report"
          echo "html-report=$html_report"
        } >> "$GITHUB_OUTPUT"
'''
artifact_replacement = '''        for artifact in "$lockfile" "$markdown_report" "$html_report"; do
          if [[ ! -f "$artifact" ]]; then
            echo "Expected WireAssume artifact was not generated: $artifact" >&2
            exit 4
          fi
        done

        diff_report=""
        if [[ -n "$WA_BASELINE_LOCK" ]]; then
          baseline="$workdir/$WA_BASELINE_LOCK"
          if [[ ! -f "$baseline" ]]; then
            echo "WireAssume baseline-lock does not exist: $baseline" >&2
            exit 5
          fi
          diff_report="$report_directory/diff.json"
          diff_args=(diff "$baseline" "$report_directory/consumption.lock.json" --json)
          if [[ "$WA_FAIL_ON_BREAKING" == "true" ]]; then
            diff_args+=(--fail-on-breaking)
          fi
          set +e
          "$binary" "${diff_args[@]}" > "$diff_report"
          diff_status=$?
          set -e
          cat "$diff_report"
          if [[ $diff_status -ne 0 ]]; then
            exit $diff_status
          fi
        fi

        {
          echo "run-id=$run_id"
          echo "report-directory=$report_directory"
          echo "lockfile=$lockfile"
          echo "markdown-report=$markdown_report"
          echo "html-report=$html_report"
          echo "diff-report=$diff_report"
        } >> "$GITHUB_OUTPUT"
'''
if "diff_args=(diff" not in text:
    if artifact_marker not in text:
        raise SystemExit("action artifact marker missing")
    text = text.replace(artifact_marker, artifact_replacement, 1)
action.write_text(text)

readme = Path("README.md")
text = readme.read_text()
text = text.replace(
    '''wireassume analyze --scenario <name> [--source-revision <revision>]
```''',
    '''wireassume analyze --scenario <name> [--source-revision <revision>]
wireassume diff <base.lock> <head.lock> [--json] [--fail-on-breaking]
wireassume export-pact <consumption.lock> [--output pact.json]
wireassume history [--workspace .wireassume] [--assumption <id>] [--json]
```''',
)
text = text.replace(
    '''The action builds the locked WireAssume workspace with Rust 1.98 by default, runs `analyze`, and exposes `run-id`, `report-directory`, `lockfile`, `markdown-report`, and `html-report` outputs.''',
    '''The action builds the locked WireAssume workspace with Rust 1.98 by default, runs `analyze`, and exposes `run-id`, `report-directory`, `lockfile`, `markdown-report`, and `html-report` outputs. Supply `baseline-lock` to run a deterministic contract diff; `fail-on-breaking` defaults to `true` so newly introduced consumer assumptions or stricter requirements fail the job.''',
)
readme.write_text(text)

action_doc = Path("docs/github-action.md")
text = action_doc.read_text()
text = text.replace(
    '''| `toolchain` | no | `1.98.0` | Rust toolchain used to build the action's WireAssume binary. |''',
    '''| `toolchain` | no | `1.98.0` | Rust toolchain used to build the action's WireAssume binary. |
| `baseline-lock` | no | empty | Previous lock path, relative to `working-directory`, for regression comparison. |
| `fail-on-breaking` | no | `true` | Fail when the generated contract adds assumptions or becomes structurally stricter. |''',
)
text = text.replace(
    '''| `html-report` | Absolute path to the self-contained run HTML report. |''',
    '''| `html-report` | Absolute path to the self-contained run HTML report. |
| `diff-report` | Absolute path to JSON contract diff when `baseline-lock` is provided; empty otherwise. |''',
)
action_doc.write_text(text)

changelog = Path("CHANGELOG.md")
text = changelog.read_text().replace(
    '''- Reusable composite GitHub Action for running `wireassume analyze` from consumer repositories.
''',
    '''- Reusable composite GitHub Action for running `wireassume analyze` from consumer repositories, with optional breaking contract regression enforcement.
- Deterministic `diff`/`history` CLI surfaces plus Pact v3 compatibility export.
''',
)
changelog.write_text(text)

status = Path("docs/project-status.md")
text = status.read_text().replace(
    '''| Reusable GitHub Action | Implemented initial slice | Root `action.yml` builds the locked CLI, runs `analyze`, validates generated artifacts, and exposes run/lock/report outputs. The OrbitDesk CI workflow self-tests `uses: ./`. |''',
    '''| Reusable GitHub Action | Implemented | Root `action.yml` builds the locked CLI, runs `analyze`, validates artifacts, exposes run/lock/report outputs, and can fail on breaking differences from a supplied baseline lock. OrbitDesk CI self-tests `uses: ./`. |
| Contract diff/history | Implemented initial slice | `wireassume diff` deterministically classifies added/removed assumptions and stricter requirements; `history` lists run-scoped revision/assumption history and can identify the first recorded revision containing an assumption. |
| Pact export | Implemented compatibility bridge | `wireassume export-pact` renders experimentally observed structural response requirements as Pact v3 matchers while retaining non-Pact behavioral assumptions in WireAssume metadata. |''',
)
status.write_text(text)
