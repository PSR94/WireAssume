# GitHub Action

WireAssume ships a composite action in the repository root (`action.yml`). It builds the locked Rust CLI from the action checkout, runs `wireassume analyze` in the caller repository, validates the expected artifacts, and exposes their paths as outputs.

The repository's permanent OrbitDesk workflow uses `uses: ./`, so the local action implementation is exercised against the real PeopleCRM demo on every push and pull request.

## Basic usage

Until a versioned v0.1 tag exists, use `@main` for evaluation or pin an exact commit SHA for reproducible production CI.

```yaml
name: Consumer contract

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
          config: .wireassume.yml
          scenario: customer-profile

      - name: Inspect generated paths
        env:
          RUN_ID: ${{ steps.wireassume.outputs.run-id }}
          LOCKFILE: ${{ steps.wireassume.outputs.lockfile }}
          HTML_REPORT: ${{ steps.wireassume.outputs.html-report }}
        run: |
          echo "Run: $RUN_ID"
          echo "Lock: $LOCKFILE"
          echo "HTML: $HTML_REPORT"
```

## Inputs

| Input | Required | Default | Description |
| --- | --- | --- | --- |
| `scenario` | yes | — | Scenario name from the selected WireAssume configuration. |
| `config` | no | `.wireassume.yml` | Configuration path relative to `working-directory`. |
| `working-directory` | no | `.` | Consumer/workspace directory inside the caller repository. |
| `source-revision` | no | empty | Contract revision; an empty value uses `GITHUB_SHA`, then WireAssume's normal fallback. |
| `toolchain` | no | `1.98.0` | Rust toolchain used to build the action's WireAssume binary. |

## Outputs

| Output | Description |
| --- | --- |
| `run-id` | Stable WireAssume run ID parsed from the completed analysis. |
| `report-directory` | Absolute path to `.wireassume/runs/<run-id>`. |
| `lockfile` | Absolute path to root `consumption.lock.yml`. |
| `markdown-report` | Absolute path to the run Markdown report. |
| `html-report` | Absolute path to the self-contained run HTML report. |

The action fails if analysis fails, if WireAssume does not emit a run ID, or if the expected lock/Markdown/HTML artifacts are missing.

## Upload reports as CI artifacts

WireAssume intentionally does not hide report retention policy inside the action. A caller can choose its own artifact retention and access controls:

```yaml
- uses: actions/upload-artifact@v6
  with:
    name: wireassume-report
    path: ${{ steps.wireassume.outputs.report-directory }}
```

Captured traffic and oracle output can contain application data. Review `SECURITY.md` and your repository's artifact visibility/retention before uploading `.wireassume` broadly.

## Monorepos

Use `working-directory` to place configuration, corpus, consumer command, and generated artifacts under one application directory:

```yaml
- id: wireassume
  uses: PSR94/WireAssume@main
  with:
    working-directory: services/orbitdesk
    config: wireassume.yml
    scenario: customer-profile
```

The action builds WireAssume from `$GITHUB_ACTION_PATH` and executes it from the caller working directory, so relative oracle paths and provider OpenAPI paths resolve as they do locally.
