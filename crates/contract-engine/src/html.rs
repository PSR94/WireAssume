use crate::{ConsumptionLock, ContractAnalysis};

pub fn to_html(lock: &ConsumptionLock, analysis: &ContractAnalysis) -> String {
    let mut requirements = lock.requirements.clone();
    requirements.sort_by(|a, b| a.target.path.cmp(&b.target.path));
    let mut assumptions = lock.assumptions.clone();
    assumptions.sort_by(|a, b| {
        a.target
            .path
            .cmp(&b.target.path)
            .then(a.assumption_type.cmp(&b.assumption_type))
    });

    let mut requirement_rows = String::new();
    for requirement in &requirements {
        requirement_rows.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&requirement.target.path),
            escape_html(requirement.presence.as_deref().unwrap_or("observed")),
            escape_html(&display_types(&requirement.types)),
            display_bool(requirement.nullable),
            display_bool(requirement.accepted_empty),
        ));
    }
    if requirement_rows.is_empty() {
        requirement_rows.push_str("<tr><td colspan=\"5\" class=\"muted\">No response-body requirements were inferred.</td></tr>");
    }

    let mut assumption_rows = String::new();
    for assumption in &assumptions {
        assumption_rows.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td><span class=\"pill {}\">{}</span></td><td>{}</td><td>{}</td></tr>",
            escape_html(&assumption.target.path),
            escape_html(&assumption.assumption_type),
            escape_html(&assumption.behavior),
            comparison_class(&assumption.provider_comparison),
            escape_html(&assumption.provider_comparison.to_ascii_uppercase()),
            escape_html(&assumption.severity),
            escape_html(&assumption.evidence_refs.join(", ")),
        ));
    }
    if assumption_rows.is_empty() {
        assumption_rows.push_str("<tr><td colspan=\"6\" class=\"muted\">No hidden assumptions were discovered.</td></tr>");
    }

    let mut tolerance_rows = String::new();
    for entry in &analysis.tolerance_map {
        tolerance_rows.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&entry.target),
            entry.passed,
            entry.failed,
            entry.inconclusive,
            escape_html(&entry.tolerated.join(", ")),
        ));
    }
    if tolerance_rows.is_empty() {
        tolerance_rows.push_str("<tr><td colspan=\"5\" class=\"muted\">No decisive tolerance data.</td></tr>");
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>WireAssume · {scenario}</title>
<style>
:root {{ color-scheme: light dark; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }}
body {{ margin: 0; background: Canvas; color: CanvasText; }}
main {{ max-width: 1180px; margin: 0 auto; padding: 40px 24px 72px; }}
h1 {{ font-size: clamp(2rem, 5vw, 3.8rem); margin: 0 0 8px; letter-spacing: -0.04em; }}
h2 {{ margin-top: 40px; font-size: 1.35rem; }}
.subtle,.muted {{ opacity: .68; }}
.grid {{ display: grid; grid-template-columns: repeat(auto-fit,minmax(180px,1fr)); gap: 12px; margin: 28px 0; }}
.card {{ border: 1px solid color-mix(in srgb, CanvasText 18%, transparent); border-radius: 14px; padding: 18px; }}
.metric {{ font-size: 2rem; font-weight: 750; letter-spacing: -.04em; }}
.label {{ font-size: .82rem; opacity: .68; text-transform: uppercase; letter-spacing: .08em; }}
table {{ width: 100%; border-collapse: collapse; font-size: .92rem; }}
th,td {{ padding: 11px 10px; text-align: left; border-bottom: 1px solid color-mix(in srgb, CanvasText 14%, transparent); vertical-align: top; }}
th {{ font-size: .76rem; text-transform: uppercase; letter-spacing: .07em; opacity: .72; }}
code {{ font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }}
.pill {{ display: inline-block; padding: 3px 8px; border-radius: 999px; font-size: .74rem; font-weight: 700; letter-spacing: .04em; }}
.pill.bad {{ background: color-mix(in srgb, #d33 18%, Canvas); }}
.pill.good {{ background: color-mix(in srgb, #2a7 18%, Canvas); }}
.pill.unknown {{ background: color-mix(in srgb, CanvasText 10%, Canvas); }}
footer {{ margin-top: 48px; opacity: .62; font-size: .82rem; }}
</style>
</head>
<body><main>
<p class="label">WireAssume Consumer Contract Analysis</p>
<h1>{provider} → {consumer}</h1>
<p class="subtle"><strong>{method}</strong> <code>{path}</code> · scenario <code>{scenario}</code></p>
<div class="grid">
  <div class="card"><div class="label">Resilience score</div><div class="metric">{score}/100</div></div>
  <div class="card"><div class="label">Assumptions</div><div class="metric">{assumption_count}</div></div>
  <div class="card"><div class="label">Requirements</div><div class="metric">{requirement_count}</div></div>
  <div class="card"><div class="label">Evidence records</div><div class="metric">{evidence_count}</div></div>
</div>
<h2>Hidden assumptions</h2>
<table><thead><tr><th>Target</th><th>Type</th><th>Observed behavior</th><th>Provider</th><th>Severity</th><th>Evidence</th></tr></thead><tbody>{assumption_rows}</tbody></table>
<h2>Minimal consumer requirements</h2>
<table><thead><tr><th>Target</th><th>Presence</th><th>Types</th><th>Nullable</th><th>Empty accepted</th></tr></thead><tbody>{requirement_rows}</tbody></table>
<h2>Tolerance map</h2>
<table><thead><tr><th>Target</th><th>Pass</th><th>Fail</th><th>Inconclusive</th><th>Demonstrated tolerance</th></tr></thead><tbody>{tolerance_rows}</tbody></table>
<footer>Generated by WireAssume {tool_version}. Behavioral requirements come from controlled consumer experiments; provider comparison only annotates those observed dependencies.</footer>
</main></body></html>"#,
        scenario = escape_html(&lock.scenario.name),
        provider = escape_html(&lock.provider.name),
        consumer = escape_html(&lock.consumer.name),
        method = escape_html(&lock.endpoint.method),
        path = escape_html(&lock.endpoint.path),
        score = analysis.dependency_resilience_score,
        assumption_count = lock.assumptions.len(),
        requirement_count = lock.requirements.len(),
        evidence_count = lock.evidence.len(),
        assumption_rows = assumption_rows,
        requirement_rows = requirement_rows,
        tolerance_rows = tolerance_rows,
        tool_version = escape_html(&lock.metadata.tool_version),
    )
}

fn display_types(types: &[String]) -> String {
    if types.is_empty() {
        "observed".into()
    } else {
        types.join(" | ")
    }
}

fn display_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    }
}

fn comparison_class(value: &str) -> &'static str {
    match value {
        "guaranteed" => "good",
        "undocumented" | "contradicted" | "incompatible" => "bad",
        _ => "unknown",
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_dynamic_html_content() {
        assert_eq!(
            escape_html("<script a='b'>&\"</script>"),
            "&lt;script a=&#39;b&#39;&gt;&amp;&quot;&lt;/script&gt;"
        );
    }

    #[test]
    fn provider_comparison_classes_are_conservative() {
        assert_eq!(comparison_class("guaranteed"), "good");
        assert_eq!(comparison_class("undocumented"), "bad");
        assert_eq!(comparison_class("unknown"), "unknown");
    }
}
