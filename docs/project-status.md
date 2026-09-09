# Project status

WireAssume is under active construction toward v0.1.0. This file is deliberately conservative: code existing in the repository is not marked validated until an automated check has executed it.

| Area | Status | Notes |
| --- | --- | --- |
| Prior-art research | Implemented | Research notes and product distinction documented. |
| `consumption.lock` v1 schema | Implemented, validation pending CI | Draft 2020-12 JSON Schema with evidence, revision, confidence, and history references. |
| Traffic corpus model | Implemented, validation pending CI | Canonical IDs, request/response/metadata artifacts, content hash. |
| Redaction | Implemented, validation pending CI | Default credential headers/query params and simple object-key JSONPaths. Full JSONPath/regex rules remain. |
| Reverse proxy recording | Roadmap | Next core milestone. |
| Replay | Roadmap | Next core milestone. |
| Deterministic mutation engine | Roadmap | Next core milestone. |
| Mutation planner | Roadmap | Budget/concurrency/cancellation planned. |
| Command oracle | Roadmap | Planned before browser oracle. |
| HTTP oracle | Roadmap | Planned before browser oracle. |
| Playwright oracle | Roadmap | Required for OrbitDesk E2E. |
| Delta debugger | Roadmap | ddmin and success-preserving minimization planned. |
| Assumption inference | Roadmap | Must consume real mutation outcomes only. |
| OpenAPI comparison | Roadmap | Classification stage, not source of assumptions. |
| OrbitDesk demo | Roadmap | No hard-coded discoveries permitted. |
| Backend API | Roadmap | FastAPI/PostgreSQL planned after deterministic engine. |
| Dashboard | Roadmap | Next.js/TypeScript planned after deterministic engine. |
| GitHub Action | Roadmap | Planned after CLI/report semantics stabilize. |
| Screenshots | Not available | Will only be captured from a running dashboard. |
| Benchmarks | Not available | No benchmark numbers will be claimed before execution. |

## Platform support

Initial target: Linux/macOS developer and CI environments. Windows support is intended but not yet validated.

## Proxy limitations

The initial design is reverse-proxy-first. TLS MITM is not required for v0.1 and will not be enabled or installed implicitly.
