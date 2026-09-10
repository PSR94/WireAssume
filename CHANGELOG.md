# Changelog

All notable changes to WireAssume are documented here. The project is currently pre-release; compatibility guarantees begin with the first tagged v0.1 release.

## Unreleased

### Added

- Deterministic Rust workspace for corpus persistence, replay, response mutation, oracle execution, experiment evidence, contract inference, and reporting.
- Versioned `wireassume.consumption/v1` JSON Schema for `consumption.lock`.
- Reverse-proxy recording with pre-persistence credential redaction, bounded payloads, configured upstream boundaries, and redirects disabled.
- Deterministic replay plus controlled single-response experiment overrides.
- Seeded JSON, array, status/header/content-type/error/pagination/timing mutation families.
- Command and HTTP consumer oracles with explicit pass/fail/inconclusive outcomes.
- Sync and async ddmin-family minimization; `analyze` uses the real consumer oracle to minimize successful response field sets.
- Evidence-backed requirement/assumption inference and a deterministic Dependency Resilience Score.
- Initial OpenAPI 3.x comparison for experimentally observed response-body presence/nullability/type dependencies.
- YAML, JSON, Markdown, and self-contained HTML analysis artifacts.
- Real OrbitDesk × PeopleCRM end-to-end demo with generated-result verification in CI.
- Reusable composite GitHub Action for running `wireassume analyze` from consumer repositories.
- Reproducible CI with committed `Cargo.lock`, formatting checks, strict Clippy, and workspace tests on Rust 1.98.

### Security

- Reverse-proxy-first design; no silent CA installation or trust-store modification.
- Default redaction for credential-like headers and query parameters before traffic persistence.
- Validated regex redaction for captured textual/JSON values and persisted oracle/minimization evidence.
- Loopback-only proxy binding unless explicitly overridden.
- Explicit-argv command oracle execution rather than implicit shell interpolation.

### Known pre-release limitations

- Rich Playwright journey DSL, browser screenshots/console/network evidence, and interactive debugging UI are not yet implemented.
- Redaction supports configured simple object-key JSON paths and regexes; wildcard/array/full JSONPath remains to be completed.
- Experiment execution currently serializes response overrides; broader isolated-worker concurrency remains to be completed.
- OpenAPI comparison intentionally supports a conservative initial subset and returns `unknown` for unsupported shapes.
- FastAPI/PostgreSQL service and Next.js dashboard are not part of the validated CLI release surface yet.
