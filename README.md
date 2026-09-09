<div align="center">

# WireAssume

**Discover what your application actually assumes about the APIs it depends on.**

> An API schema tells you what a provider may send. WireAssume experimentally discovers what your application actually depends on.

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.1.0--dev-orange.svg)](docs/project-status.md)

</div>

WireAssume is an evidence-driven consumer-dependency mining tool. It records real API interactions, systematically mutates provider responses, runs the real consuming workflow through an oracle, and retains the evidence needed to infer a scenario-scoped behavioral contract.

The primary artifact is `consumption.lock`: a versioned record of what a specific consumer revision has experimentally demonstrated it requires—not what the provider documentation happens to promise.

## Experimental loop

```text
record → normalize → mutate → replay → run consumer oracle
       → observe → minimize → classify → persist evidence
```

A field appearing in every captured response is only a candidate dependency. WireAssume does not call it required until an experiment changes that behavior and the configured consumer workflow fails.

## Example

```text
Provider response               Controlled experiment
─────────────────               ─────────────────────
email: "alice@example.com"  →   remove body.email  → FAIL
avatar: null                →   avatar = ""        → PASS
metadata: {...}             →   remove metadata    → PASS
customers: [A, B, C]        →   reverse ordering   → FAIL

Observed consumer requirements
──────────────────────────────
body.email: present, non-null string
customers: ordering-sensitive
```

Every finding is scoped by provider, consumer revision, scenario, traffic corpus, seed, mutation policy, and oracle evidence.

## Repository state

The project is being built from the deterministic core outward. See [`docs/project-status.md`](docs/project-status.md) for an explicit implemented/partial/roadmap matrix. The repository does **not** claim unexecuted tests, fabricated screenshots, or hard-coded discoveries.

## Core design constraints

- Local/development/CI robustness testing; not offensive tooling.
- Reverse-proxy-first; no silent TLS certificate installation.
- Secrets are redacted before traffic is persisted.
- Stable mutation IDs and canonical serialization make experiments reproducible.
- AI, if added later, may explain evidence but is never the source of truth.
- OpenAPI comparison classifies a finding; it does not manufacture one.

## Documentation

- [Prior art and positioning](docs/research/prior-art.md)
- [Project status](docs/project-status.md)
- [Security model](SECURITY.md)
- [Architecture decisions](docs/architecture/decisions/)

## License

Apache-2.0.
