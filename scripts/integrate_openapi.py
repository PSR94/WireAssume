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
    "use wireassume_mutation_engine::MutationKind;\n",
    "use wireassume_mutation_engine::MutationKind;\n\nmod openapi;\npub use openapi::{apply_openapi_comparison, OpenApiComparison, ProviderMismatch};\n",
    "contract-engine module",
)
replace_once(
    lib,
    '''            out.push_str(&format!(
                "Severity: **{}** · Confidence: **{}**\\n\\n",
                assumption.severity, assumption.confidence
            ));''',
    '''            out.push_str(&format!(
                "Severity: **{}** · Confidence: **{}** · Provider comparison: **{}**\\n\\n",
                assumption.severity, assumption.confidence, assumption.provider_comparison
            ));''',
    "contract markdown",
)

openapi = Path("crates/contract-engine/src/openapi.rs")
replace_once(
    openapi,
    "fn classify<'a>(\n    assumption_type: &str,\n    guarantee: &PropertyGuarantee,\n    requirement: Option<&Requirement>,\n) -> (&'a str, &'a str) {",
    "fn classify(\n    assumption_type: &str,\n    guarantee: &PropertyGuarantee,\n    requirement: Option<&Requirement>,\n) -> (&'static str, &'static str) {",
    "OpenAPI classification lifetime",
)

cli = Path("crates/cli/src/main.rs")
replace_once(
    cli,
    '''use wireassume_contract_engine::{
    analyze_contract, build_consumption_lock, to_json, to_markdown, to_yaml, ContractContext,
};''',
    '''use wireassume_contract_engine::{
    analyze_contract, apply_openapi_comparison, build_consumption_lock, to_json, to_markdown,
    to_yaml, ContractContext,
};''',
    "CLI contract-engine import",
)
replace_once(
    cli,
    '''        "\\nNot yet claimed by this command: integrated delta minimization, OpenAPI comparison, browser step DSL, backend API, or dashboard services."
    );''',
    '''        "\\nNot yet claimed by this command: integrated delta minimization, browser step DSL, backend API, or dashboard services."
    );''',
    "doctor capabilities",
)
replace_once(
    cli,
    '''    let lock = build_consumption_lock(&report, &baselines, &context)?;
    let analysis = analyze_contract(&report);''',
    '''    let mut lock = build_consumption_lock(&report, &baselines, &context)?;
    let provider_comparison = if let Some(openapi) = &config.provider.openapi {
        let configured = Path::new(openapi);
        let spec_path = if configured.is_absolute() {
            configured.to_path_buf()
        } else {
            config_path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .join(configured)
        };
        let document = fs::read_to_string(&spec_path)
            .with_context(|| format!("read provider OpenAPI {}", spec_path.display()))?;
        Some(apply_openapi_comparison(&mut lock, &document).with_context(|| {
            format!("parse provider OpenAPI {}", spec_path.display())
        })?)
    } else {
        None
    };
    let analysis = analyze_contract(&report);''',
    "CLI contract construction",
)
replace_once(
    cli,
    '''    println!(
        "Dependency Resilience Score: {} / 100",
        analysis.dependency_resilience_score
    );
    println!("\\nRun: {}", report.run_id);''',
    '''    println!(
        "Dependency Resilience Score: {} / 100",
        analysis.dependency_resilience_score
    );
    if let Some(comparison) = &provider_comparison {
        println!("Provider spec comparisons: {}", comparison.compared_assumptions);
        println!("Provider guarantee mismatches: {}", comparison.mismatches.len());
        for mismatch in &comparison.mismatches {
            println!(
                "  ⚠ {} {}: provider={} classification={}",
                mismatch.target,
                mismatch.assumption_type,
                mismatch.provider_guarantee,
                mismatch.classification
            );
        }
    }
    println!("\\nRun: {}", report.run_id);''',
    "CLI provider comparison output",
)

verify = Path("demo/verify_demo.py")
replace_once(
    verify,
    '''    if any(not assumption.get("evidence_refs") for assumption in email_assumptions):
        return fail("an email assumption has no evidence references")

    print(''',
    '''    if any(not assumption.get("evidence_refs") for assumption in email_assumptions):
        return fail("an email assumption has no evidence references")

    comparisons = {
        assumption["type"]: assumption.get("provider_comparison")
        for assumption in email_assumptions
    }
    if comparisons.get("presence") != "undocumented":
        return fail(
            f"email presence provider comparison was {comparisons.get('presence')!r}, "
            "expected 'undocumented'"
        )
    if comparisons.get("nullability") != "contradicted":
        return fail(
            f"email nullability provider comparison was {comparisons.get('nullability')!r}, "
            "expected 'contradicted'"
        )

    print(''',
    "demo provider comparison verification",
)
replace_once(
    verify,
    '''        "demo verification OK: OrbitDesk experimentally requires PeopleCRM email "
        "presence + non-nullability while tolerating empty strings"''',
    '''        "demo verification OK: OrbitDesk experimentally requires PeopleCRM email "
        "presence + non-nullability while tolerating empty strings; OpenAPI marks "
        "the dependency optional + nullable"''',
    "demo success message",
)

readme = Path("demo/README.md")
replace_once(
    readme,
    "Provider-spec mismatch classification is the next integration step; `peoplecrm.openapi.yaml` is included now as the source fixture and must not be used to manufacture consumer assumptions.\n",
    "`wireassume analyze` also compares those already-observed assumptions with `peoplecrm.openapi.yaml`: email presence is an undocumented consumer dependency because the provider marks it optional, and non-nullability is contradicted because the provider explicitly allows `null`. The OpenAPI document never creates consumer assumptions by itself.\n",
    "demo README provider comparison",
)

status = Path("docs/project-status.md")
replace_once(
    status,
    "| OpenAPI comparison | Roadmap | Configuration metadata exists, but actual provider-guarantee mismatch classification is not yet implemented. |",
    "| OpenAPI comparison | Implemented initial slice | `analyze` compares experimentally observed response-body assumptions with configured OpenAPI 3.x required/nullable/type guarantees; unsupported shapes are reported as unknown. |",
    "project status OpenAPI",
)
replace_once(
    status,
    "| OrbitDesk + PeopleCRM demo | Roadmap | Next major milestone: a real consumer/provider vertical slice with no hard-coded discoveries. |",
    "| OrbitDesk + PeopleCRM demo | Implemented vertical slice | CI executes the real OrbitDesk command consumer against controlled PeopleCRM replay mutations and verifies evidence-backed email presence/non-null assumptions plus provider-spec mismatch classification. |",
    "project status demo",
)
