# Project status

WireAssume is under active construction toward v0.1.0. This matrix distinguishes implemented components from end-to-end product integration. The Rust workspace currently passes formatting, Clippy with warnings denied, and workspace tests on Rust 1.98 with a committed `Cargo.lock`.

| Area | Status | Notes |
| --- | --- | --- |
| Prior-art research | Implemented | Research notes and product distinction documented. |
| `consumption.lock` v1 schema | Implemented | Draft 2020-12 JSON Schema with evidence, revision, confidence, and history references. |
| Traffic corpus model | Implemented | Canonical IDs, request/response/metadata artifacts, and content hashing. |
| Redaction | Implemented | Default credential headers/query params and simple object-key JSONPaths. Full JSONPath/regex rules remain. |
| Traffic normalization | Implemented | Separate deterministic normalization stage; raw evidence is not mutated in place. |
| Reverse proxy recording | Implemented | Reverse-proxy capture with bounded bodies, configured upstream boundary, and redirects disabled. Full product E2E remains. |
| Deterministic replay | Implemented | Recorded interactions can be replayed deterministically from the persisted corpus. |
| Controlled experiment replay | Implemented | Supports one response override at a time while other recorded calls replay normally. |
| Deterministic mutation engine | Implemented | JSON, array, and protocol mutations with seeded stable IDs. |
| Mutation planner | Implemented, partial scheduling | Deterministic planning, budgets, tested-ID filtering, and candidate caps exist. Broader concurrency/cancellation policy remains. |
| Command oracle | Implemented | Explicit argv, controlled environment, timeout, and pass/fail/inconclusive semantics. |
| HTTP oracle | Implemented | HTTP workflow oracle with timeout and pass/fail/inconclusive semantics. |
| Playwright adapter | Partial | Playwright configuration can be executed through command-oracle infrastructure. Rich browser journeys/screenshots/console/network evidence are not implemented. |
| Experiment runner | Implemented | Verifies baseline first, applies one counterfactual at a time, runs the real oracle, resets state, and persists evidence artifacts. |
| Delta debugger | Implemented component | ddmin-family synchronous and asynchronous reducers exist and are unit tested. Full integration into `analyze` is still partial. |
| Assumption inference | Implemented | Contract inference consumes real experiment outcomes; failing counterfactuals create assumptions and inconclusive trials do not create tolerance claims. |
| Tolerance/resilience analysis | Implemented | Deterministic tolerance map and weighted resilience score are derived from decisive experiment outcomes. |
| `wireassume analyze` | Partial integration | Runs evidence-backed controlled mutation experiments and emits contract/report outputs. Integrated ddmin and provider-spec comparison are still required for the target vertical slice. |
| OpenAPI comparison | Roadmap | Configuration metadata exists, but actual provider-guarantee mismatch classification is not yet implemented. |
| OrbitDesk + PeopleCRM demo | Roadmap | Next major milestone: a real consumer/provider vertical slice with no hard-coded discoveries. |
| Backend API | Roadmap | FastAPI/PostgreSQL deferred until the deterministic CLI vertical slice is proven. |
| Dashboard | Roadmap | Next.js/TypeScript deferred until the deterministic CLI vertical slice is proven. |
| GitHub Action product integration | Roadmap | Core CI exists; WireAssume PR contract-diff integration is planned after CLI/report semantics stabilize. |
| Screenshots | Not available | Will only be captured from a running product. |
| Benchmarks | Not available | No benchmark numbers will be claimed before execution. |

## Validation

The core Rust CI runs:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

A passing workspace build validates the implemented Rust components; it does **not** yet constitute the target OrbitDesk/PeopleCRM end-to-end demo.

## Platform support

Initial target: Linux/macOS developer and CI environments. Windows support is intended but not yet validated.

## Proxy limitations

The current design is reverse-proxy-first. TLS MITM is not required for v0.1 and will not be enabled or installed implicitly.
