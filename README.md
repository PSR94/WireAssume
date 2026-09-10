<div align="center">

# WireAssume

**Discover what your application actually assumes about the APIs it depends on.**

An API schema tells you what a provider *may* send. WireAssume runs controlled counterfactual experiments to discover what your consumer *actually needs*.

[![Rust core](https://github.com/PSR94/WireAssume/actions/workflows/rust.yml/badge.svg)](https://github.com/PSR94/WireAssume/actions/workflows/rust.yml)
[![OrbitDesk demo](https://github.com/PSR94/WireAssume/actions/workflows/demo.yml/badge.svg)](https://github.com/PSR94/WireAssume/actions/workflows/demo.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

</div>

WireAssume is an evidence-driven consumer-dependency mining tool. It records real provider traffic, replays it deterministically, mutates one behavior at a time, runs a real consumer workflow through an oracle, minimizes successful response shapes, and emits an evidence-backed `consumption.lock`.

The central rule is simple: **a dependency is not inferred because a field appears in traffic or in OpenAPI. It is inferred because changing provider behavior changes the real consumer outcome.**

## See it work

The repository contains a real end-to-end demo: **OrbitDesk** consumes a recorded **PeopleCRM** response. PeopleCRM documents `email` as optional and nullable; OrbitDesk secretly requires it to be present and non-null.

```bash
git clone https://github.com/PSR94/WireAssume.git
cd WireAssume
cargo build --workspace --locked

cd demo
rm -rf .wireassume/runs .wireassume/evidence consumption.lock.yml
../target/debug/wireassume \
  --config wireassume.yml \
  analyze \
  --scenario customer-profile \
  --source-revision demo-v1

python3 verify_demo.py
```

The CI-verified demo currently produces this behavioral result:

```text
Provider: PeopleCRM
Consumer: OrbitDesk
Scenario: customer-profile
Endpoint: GET /customers/*
Mutations executed: 11

Response minimizations: 1
  4 → 1 fields; minimal [email]; tests 4; established=true

Consumer failures: 2
Inconclusive trials: 0
Discovered assumptions: 2
Dependency Resilience Score: 78 / 100

Provider spec comparisons: 2
Provider guarantee mismatches: 2
  /email nullability: provider=nullable classification=contradicted
  /email presence: provider=optional classification=undocumented
```

`demo/verify_demo.py` independently checks that those findings came from the generated report/lock, that the response was minimized to `{email}`, and that the OpenAPI comparison is attached only after the consumer dependency has been experimentally observed.

## What WireAssume writes

A successful `analyze` run keeps both machine-readable and human-readable evidence:

| Artifact | Purpose |
| --- | --- |
| `consumption.lock.yml` | Stable root contract artifact for the current analysis. |
| `.wireassume/runs/<run-id>/consumption.lock.yml` | Run-scoped YAML contract. |
| `.wireassume/runs/<run-id>/consumption.lock.json` | Run-scoped JSON contract. |
| `.wireassume/runs/<run-id>/report.json` | Raw experiment report, including mutation outcomes and minimization results. |
| `.wireassume/runs/<run-id>/report.md` | Human-readable contract summary. |
| `.wireassume/runs/<run-id>/report.html` | Self-contained static HTML report with assumptions, requirements, provider comparison, evidence references, and tolerance data. |
| `.wireassume/evidence/<evidence-id>/oracle.json` | Per-trial oracle evidence. |
| `.wireassume/runs/<run-id>/baseline-oracle.json` | Baseline consumer-oracle evidence. |

Stable IDs are derived from canonical fingerprints; seeded mutation planning and deterministic replay make repeated experiments explainable and reproducible.

## Experimental loop

```text
record real traffic
      ↓
redact before persistence
      ↓
normalize an analysis view
      ↓
plan deterministic mutations
      ↓
replay baseline + one controlled change
      ↓
run the real consumer oracle
      ↓
observe PASS / FAIL / INCONCLUSIVE
      ↓
minimize successful response fields with ddmin
      ↓
infer requirements from evidence
      ↓
compare observed assumptions with OpenAPI guarantees
      ↓
write consumption.lock + evidence + Markdown/HTML reports
```

WireAssume keeps **ground truth deterministic**. An LLM is not used to decide whether a field is required, whether a mutation failed, or whether a contract changed.

## CLI

```text
wireassume init [--force]
wireassume doctor
wireassume record --scenario <name>
wireassume replay [--listen <host:port>]
wireassume mutate <response.json> [--scenario <name>] [--budget N] [--seed N]
wireassume analyze --scenario <name> [--source-revision <revision>]
```

Typical lifecycle:

```bash
# 1. Create a starter configuration and local workspace.
wireassume init
wireassume doctor

# 2. Point the consumer at the WireAssume reverse proxy and exercise the scenario.
wireassume record --scenario customer-profile

# 3. Inspect deterministic replay if useful.
wireassume replay

# 4. Run the actual counterfactual experiment.
wireassume analyze --scenario customer-profile
```

`record` is reverse-proxy-first: WireAssume does not silently install a CA, modify system trust, or require TLS interception.

## Configuration

A scenario connects recorded traffic to a real consumer oracle and a mutation policy:

```yaml
project:
  name: OrbitDesk

provider:
  name: PeopleCRM
  openapi: peoplecrm.openapi.yaml

proxy:
  listen: 127.0.0.1:0
  max_payload_bytes: 2097152

scenarios:
  - name: customer-profile
    traffic:
      endpoint:
        method: GET
        path: /customers/*
    oracle:
      type: command
      argv: ["python3", "orbitdesk.py"]
      cwd: "."
      timeout_ms: 5000
      inherit_env: true
      stdout_contains: ["OrbitDesk customer profile OK"]
    mutation:
      budget: 50
      concurrency: 1
      seed: 42
      include:
        - remove-field
        - null-field
        - empty-string
        - additional-unknown-property
```

During `analyze`, command-like oracles receive `WIREASSUME_REPLAY_BASE_URL` unless they explicitly provide their own value. This lets the consumer run normally while WireAssume controls the provider side of the experiment.

## GitHub Action

The repository includes a composite action at `action.yml`. Until a versioned v0.1 tag is published, pin to `main` for development evaluation or pin an exact commit SHA for reproducible CI.

```yaml
name: Consumer API assumptions

on:
  pull_request:

permissions:
  contents: read

jobs:
  wireassume:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7

      - id: wireassume
        uses: PSR94/WireAssume@main
        with:
          working-directory: .
          config: .wireassume.yml
          scenario: customer-profile

      - name: Show generated artifacts
        env:
          LOCKFILE: ${{ steps.wireassume.outputs.lockfile }}
          HTML_REPORT: ${{ steps.wireassume.outputs.html-report }}
        run: |
          echo "Contract: $LOCKFILE"
          echo "Report:   $HTML_REPORT"
```

The action builds the locked WireAssume workspace with Rust 1.98 by default, runs `analyze`, and exposes `run-id`, `report-directory`, `lockfile`, `markdown-report`, and `html-report` outputs. The repository's own OrbitDesk workflow uses `uses: ./`, so the action path is exercised end-to-end on every push and pull request.

## What can be tested

The deterministic mutation engine includes response-body and protocol perturbations such as:

- field removal, `null`, empty/whitespace strings, wrong primitive types, empty arrays/objects, unknown enum-like values, numeric boundaries, Unicode/long strings, and unknown properties;
- array reversal, deterministic shuffle, duplicate/remove-item cases, cardinality changes, and repeated elements;
- status changes including 4xx/5xx families, redirects, header/content-type changes, empty responses, optional malformed JSON, response delay, common error-contract changes, and pagination metadata changes.

Mutation IDs are stable and randomness is explicitly seeded. `INCONCLUSIVE` oracle outcomes never become consumer requirements or tolerance claims.

## Oracles

WireAssume currently supports:

| Oracle | Status | Use |
| --- | --- | --- |
| Command | Implemented | Run a real CLI/test/workflow process and assert exit/stdout/stderr behavior. |
| HTTP | Implemented | Call a real HTTP consumer workflow and assert status/body/header behavior. |
| Playwright command adapter | Partial | Execute a Playwright-backed external command; rich browser-step DSL/evidence is not yet implemented. |
| Custom command | Implemented through command infrastructure | Integrate project-specific workflow runners without putting application logic inside WireAssume. |

## What gets inferred

WireAssume can turn decisive counterfactual evidence into scenario-scoped assumptions around presence, nullability, primitive type, empty-string tolerance, array behavior, status/error/header/content-type behavior, pagination, redirects, and timing where the corresponding mutation exists.

For structural response fields, passing experiments also demonstrate tolerance. For example:

```text
remove /email → FAIL   ⇒ /email is required
null /email   → FAIL   ⇒ /email is non-null
empty /email  → PASS   ⇒ empty string is tolerated
```

The success-preserving async ddmin pass then asks a different question: *what is the smallest recorded response field set that still lets the real consumer workflow pass?*

## Provider comparison

OpenAPI is comparison evidence, not consumer ground truth.

After WireAssume has observed a consumer assumption, it can classify the provider relationship:

- `guaranteed` — the provider specification promises the behavior the consumer needs;
- `undocumented` — the consumer relies on behavior the provider does not require/document as a guarantee;
- `contradicted` — the provider explicitly permits behavior the consumer rejected;
- `incompatible` — observed type expectations conflict with documented provider types;
- `unknown` — the current comparison engine cannot safely classify that shape.

The initial comparison slice handles OpenAPI 3.x response-body required/nullable/type guarantees and `$ref` resolution for the tested path. Unsupported schema constructs remain `unknown` instead of being guessed.

## Architecture

The deterministic core is a Rust workspace split by responsibility:

| Crate | Responsibility |
| --- | --- |
| `model` | Traffic/evidence models, canonical serialization, stable IDs, redaction, corpus persistence. |
| `config` | Typed `.wireassume.yml` parsing and safety validation. |
| `normalizer` | Separate deterministic analysis normalization. |
| `replay` | Conservative deterministic corpus matching. |
| `proxy` | Reverse-proxy recording, replay server, and controlled experiment overrides. |
| `mutation-engine` | Seeded response-body, array, and protocol mutations plus planning. |
| `oracles` | Command and HTTP consumer workflow evaluation. |
| `delta-debugger` | Sync/async ddmin-family minimization. |
| `experiment` | Baseline validation, real mutation trials, oracle evidence, and response minimization. |
| `contract-engine` | Evidence-backed requirement inference, resilience/tolerance analysis, OpenAPI comparison, Markdown/HTML rendering. |
| `cli` | `wireassume` command-line product surface. |

Architecture decisions live under [`docs/architecture/decisions/`](docs/architecture/decisions/).

## Security model

WireAssume is intended for local development and CI robustness testing of systems you control.

- Credential-like headers and query parameters are redacted before captured traffic is persisted.
- Configured JSON body paths can be redacted before persistence.
- Recording is reverse-proxy-first; there is no silent certificate or trust-store modification.
- Proxy payload sizes are bounded.
- Upstream redirects are disabled while recording.
- Non-loopback proxy listening requires explicit opt-in.
- Command oracles use explicit argv instead of implicit shell interpolation.

See [`SECURITY.md`](SECURITY.md) for the threat model and safe-use boundaries.

## Current scope

The evidence-producing CLI vertical slice is real and continuously tested. The permanent CI gates are:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The OrbitDesk workflow separately executes the real consumer against mutated PeopleCRM replay and verifies the generated contract, minimization result, provider comparison, and HTML report.

The richer browser evidence DSL, multi-worker experiment concurrency, full JSONPath/regex redaction, backend service, and interactive Next.js dashboard are not represented here as completed features. Their status is tracked explicitly in [`docs/project-status.md`](docs/project-status.md); the README does not use mock screenshots or hard-coded findings to imply otherwise.

## Prior art and design

WireAssume combines ideas from consumer-driven contracts, record/replay proxies, schema fuzzing, and delta debugging, but changes the source of truth: the contract is learned from **provider behavior change → real consumer workflow → observed outcome**.

See [`docs/research/prior-art.md`](docs/research/prior-art.md) for the detailed positioning and citations.

## Contributing

Contributions that preserve deterministic ground truth and evidence traceability are welcome. Please run the three Rust CI commands above and the OrbitDesk demo before proposing core inference changes. See [`CONTRIBUTING.md`](CONTRIBUTING.md) when present in your checkout for contribution conventions.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
