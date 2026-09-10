# Project status

WireAssume is under active construction toward v0.1.0. This matrix distinguishes implemented components from broader product work. The Rust workspace passes formatting, Clippy with warnings denied, and workspace tests on Rust 1.98 with a committed `Cargo.lock`; the separate OrbitDesk workflow executes the reusable local Action against the real PeopleCRM demo and verifies generated evidence/contract/report artifacts.

| Area | Status | Notes |
| --- | --- | --- |
| Prior-art research | Implemented | Research notes and product distinction documented. |
| `consumption.lock` v1 schema | Implemented | Draft 2020-12 JSON Schema with evidence, revision, confidence, and history references. |
| Traffic corpus model | Implemented | Canonical IDs, request/response/metadata artifacts, and content hashing. |
| Redaction | Implemented, partial JSONPath surface | Default credential headers/query params, simple object-key JSON paths, and validated regex rules are redacted before persistence. The same regex policy scrubs persisted oracle text. Full wildcard/array JSONPath remains. |
| Traffic normalization | Implemented | Separate deterministic normalization stage; persisted evidence is not mutated in place for analysis. |
| Reverse proxy recording | Implemented | Reverse-proxy capture with bounded bodies, configured upstream boundary, redirects disabled, and loopback-first defaults. |
| Deterministic replay | Implemented | Recorded interactions can be replayed deterministically from the persisted corpus. |
| Controlled experiment replay | Implemented | Supports one response override at a time while other recorded calls replay normally. |
| Deterministic mutation engine | Implemented | JSON, array, and protocol mutations with seeded stable IDs. |
| Mutation planner | Implemented, partial scheduling | Deterministic planning, budgets, tested-ID filtering, and candidate caps exist. Isolated multi-worker execution/cancellation remains. |
| Command oracle | Implemented | Explicit argv, controlled environment, timeout, and pass/fail/inconclusive semantics. |
| HTTP oracle | Implemented | HTTP workflow oracle with timeout and pass/fail/inconclusive semantics. |
| Playwright adapter | Partial | Playwright configuration can be executed through command-oracle infrastructure. Rich browser journeys/screenshots/console/network evidence are not implemented. |
| Experiment runner | Implemented | Verifies baseline first, applies one counterfactual at a time, runs the real oracle, resets state, and persists evidence artifacts. |
| Delta debugger | Integrated | Sync/async ddmin-family reducers exist; `analyze` performs async success-preserving response-field minimization through the real consumer oracle and persists minimization trial evidence. |
| Assumption inference | Implemented | Contract inference consumes real experiment outcomes; failing counterfactuals create assumptions and inconclusive trials do not create tolerance claims. |
| Tolerance/resilience analysis | Implemented | Deterministic tolerance map and weighted resilience score are derived from decisive experiment outcomes. |
| Static HTML reporting | Implemented | Each analysis run emits a self-contained HTML report alongside Markdown and JSON/YAML artifacts, including provider comparison and evidence references. |
| `wireassume analyze` | Implemented vertical slice | Runs controlled mutations, real consumer oracles, async response-field minimization, deterministic inference, configured OpenAPI guarantee comparison, and lock/report output. |
| OpenAPI comparison | Implemented initial slice | Compares experimentally observed response-body assumptions with configured OpenAPI 3.x required/nullable/type guarantees; unsupported shapes remain `unknown`. |
| OrbitDesk + PeopleCRM demo | Implemented vertical slice | CI executes OrbitDesk against controlled PeopleCRM replay and verifies email presence/non-null assumptions, `{email}` ddmin result, OpenAPI mismatches, and static HTML output. |
| Reusable GitHub Action | Implemented | Root `action.yml` builds the locked CLI, runs `analyze`, validates artifacts, exposes run/lock/report outputs, and can fail on breaking differences from a supplied baseline lock. OrbitDesk CI self-tests `uses: ./`. |
| Contract diff/history | Implemented initial slice | `wireassume diff` deterministically classifies added/removed assumptions and stricter requirements; `history` lists run-scoped revision/assumption history and can identify the first recorded revision containing an assumption. |
| Pact export | Implemented compatibility bridge | `wireassume export-pact` renders experimentally observed structural response requirements as Pact v3 matchers while retaining non-Pact behavioral assumptions in WireAssume metadata. |
| Backend API | Roadmap | FastAPI/PostgreSQL service remains broader product work outside the validated CLI release surface. |
| Dashboard | Roadmap | Interactive Next.js/TypeScript dashboard remains broader product work outside the validated CLI release surface. |
| Screenshots | Not available | No product screenshots are committed; any future screenshots must come from a running dashboard. |
| Benchmarks | Not available | No benchmark numbers are claimed before an executed benchmark suite exists. |

## Validation

The permanent Rust core workflow runs:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The separate OrbitDesk workflow then uses the repository's composite Action (`uses: ./`) to execute `wireassume analyze` against the checked-in PeopleCRM corpus and real OrbitDesk command consumer. `demo/verify_demo.py` checks the generated contract, provider comparison, async minimization result, action output paths, and self-contained HTML report.

A passing unit/workspace build alone is not treated as proof of the end-to-end product path; both permanent workflows exist for that reason.

## Platform support

Initial validated CI target: Linux. The code is designed for Linux/macOS developer environments; Windows has not yet been validated in CI.

## Proxy limitations

The current design is reverse-proxy-first. TLS MITM is not required for v0.1 and is not enabled or installed implicitly.
