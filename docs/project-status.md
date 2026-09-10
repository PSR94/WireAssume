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
| Delta debugger | Integrated | ddmin-family synchronous/asynchronous reducers exist; `analyze` now performs async success-preserving response-field minimization through the real consumer oracle and persists the candidate-trial evidence. |
| Assumption inference | Implemented | Contract inference consumes real experiment outcomes; failing counterfactuals create assumptions and inconclusive trials do not create tolerance claims. |
| Tolerance/resilience analysis | Implemented | Deterministic tolerance map and weighted resilience score are derived from decisive experiment outcomes. |
| Static HTML reporting | Implemented | Each analysis run emits a self-contained HTML report alongside Markdown and JSON/YAML artifacts, including provider comparison and evidence references. |
| `wireassume analyze` | Implemented vertical slice | Runs evidence-backed controlled mutations, async response-field minimization, deterministic inference, configured OpenAPI guarantee comparison, and lock/report output for the OrbitDesk/PeopleCRM scenario. Broader protocol coverage remains roadmap. |
| OpenAPI comparison | Implemented initial slice | `analyze` compares experimentally observed response-body assumptions with configured OpenAPI 3.x required/nullable/type guarantees; unsupported shapes are reported as unknown. |
| OrbitDesk + PeopleCRM demo | Implemented vertical slice | CI executes the real OrbitDesk command consumer against controlled PeopleCRM replay mutations and verifies evidence-backed email presence/non-null assumptions plus provider-spec mismatch classification. |
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
