# ADR 0007: AI explanations are optional and non-authoritative

Status: accepted

## Decision

No AI dependency exists in the deterministic analysis path. A future optional provider abstraction may explain findings or suggest hardening only after the evidence-backed result is finalized.

AI output must not modify mutation outcomes, evidence, scores, or contract requirements.
