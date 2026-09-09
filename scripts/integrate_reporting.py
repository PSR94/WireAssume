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


contract = Path("crates/contract-engine/src/lib.rs")
replace_once(
    contract,
    '''mod openapi;
pub use openapi::{apply_openapi_comparison, OpenApiComparison, ProviderMismatch};''',
    '''mod html;
mod openapi;
pub use html::to_html;
pub use openapi::{apply_openapi_comparison, OpenApiComparison, ProviderMismatch};''',
    "contract HTML module",
)

cli = Path("crates/cli/src/main.rs")
replace_once(
    cli,
    '''use wireassume_contract_engine::{
    analyze_contract, apply_openapi_comparison, build_consumption_lock, to_json, to_markdown,
    to_yaml, ContractContext,
};''',
    '''use wireassume_contract_engine::{
    analyze_contract, apply_openapi_comparison, build_consumption_lock, to_html, to_json,
    to_markdown, to_yaml, ContractContext,
};''',
    "CLI HTML import",
)
replace_once(
    cli,
    '''    let markdown = to_markdown(&lock, &analysis);
    fs::write(run_directory.join("consumption.lock.yml"), &yaml)?;
    fs::write(run_directory.join("consumption.lock.json"), &json)?;
    fs::write(run_directory.join("report.md"), &markdown)?;
    fs::write("consumption.lock.yml", &yaml)?;''',
    '''    let markdown = to_markdown(&lock, &analysis);
    let html = to_html(&lock, &analysis);
    fs::write(run_directory.join("consumption.lock.yml"), &yaml)?;
    fs::write(run_directory.join("consumption.lock.json"), &json)?;
    fs::write(run_directory.join("report.md"), &markdown)?;
    fs::write(run_directory.join("report.html"), &html)?;
    fs::write("consumption.lock.yml", &yaml)?;''',
    "CLI HTML report write",
)
replace_once(
    cli,
    '''    println!("Report: {}/report.md", run_directory.display());''',
    '''    println!("Reports: {}/report.md and report.html", run_directory.display());''',
    "CLI report output",
)

verify = Path("demo/verify_demo.py")
replace_once(
    verify,
    '''    with open(locks[0], encoding="utf-8") as handle:
        lock = json.load(handle)

    if report["baseline"]["summary"].find("passed") < 0:''',
    '''    with open(locks[0], encoding="utf-8") as handle:
        lock = json.load(handle)
    html_report = os.path.join(os.path.dirname(reports[0]), "report.html")
    if not os.path.exists(html_report):
        return fail("static HTML report was not generated")
    with open(html_report, encoding="utf-8") as handle:
        html = handle.read()
    for expected in ("WireAssume Consumer Contract Analysis", "UNDOCUMENTED", "CONTRADICTED"):
        if expected not in html:
            return fail(f"static HTML report is missing {expected!r}")

    if report["baseline"]["summary"].find("passed") < 0:''',
    "demo HTML report verification",
)

readme = Path("demo/README.md")
replace_once(
    readme,
    '''`wireassume analyze` also compares those already-observed assumptions with `peoplecrm.openapi.yaml`: email presence is an undocumented consumer dependency because the provider marks it optional, and non-nullability is contradicted because the provider explicitly allows `null`. The OpenAPI document never creates consumer assumptions by itself.
''',
    '''`wireassume analyze` also compares those already-observed assumptions with `peoplecrm.openapi.yaml`: email presence is an undocumented consumer dependency because the provider marks it optional, and non-nullability is contradicted because the provider explicitly allows `null`. The OpenAPI document never creates consumer assumptions by itself.

Each run writes deterministic machine-readable lock output plus Markdown and a self-contained static HTML report at `.wireassume/runs/<run-id>/report.html`.
''',
    "demo HTML documentation",
)

status = Path("docs/project-status.md")
replace_once(
    status,
    '''| Tolerance/resilience analysis | Implemented | Deterministic tolerance map and weighted resilience score are derived from decisive experiment outcomes. |
| `wireassume analyze` | Implemented vertical slice |''',
    '''| Tolerance/resilience analysis | Implemented | Deterministic tolerance map and weighted resilience score are derived from decisive experiment outcomes. |
| Static HTML reporting | Implemented | Each analysis run emits a self-contained HTML report alongside Markdown and JSON/YAML artifacts, including provider comparison and evidence references. |
| `wireassume analyze` | Implemented vertical slice |''',
    "project status HTML report",
)
