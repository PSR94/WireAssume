# ADR 0001: Rust for the deterministic core

Status: accepted

## Decision

Implement the traffic model, recorder/replay primitives, mutation engine, minimizer, and CLI in Rust.

## Rationale

These components sit on the hot path of repeatable experiments, manipulate untrusted payloads, and benefit from explicit types, predictable resource use, and a single distributable CLI. Python and TypeScript remain appropriate for orchestration/API and dashboard layers respectively.
