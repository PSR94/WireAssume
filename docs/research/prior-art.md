# Prior art and positioning

WireAssume mines the behavioral contract a consumer has *demonstrated* it needs. This document records adjacent techniques and the design lessons we adopt without collapsing WireAssume into any one of them.

## Summary

| Project / concept | Category | Similarity | Important difference | Lesson for WireAssume |
| --- | --- | --- | --- | --- |
| [Pact](https://docs.pact.io/) | Consumer-driven contract testing | Consumer/provider vocabulary; tests protect integration assumptions | Expectations are authored in consumer tests and then verified against a provider. WireAssume starts from observed traffic and systematically mutates provider behavior while executing the real consumer workflow. | Keep contracts consumer-centric, but make evidence—not authored expectations—the source of truth. |
| [WireMock](https://wiremock.org/docs/record-playback/) | Service virtualization; record/replay | Can proxy, record traffic, persist stubs, and replay behavior | Recording creates simulations/stubs; it does not search for the minimal provider behavior a real consumer requires. | Use a simple reverse-proxy-first recording model and keep captured artifacts inspectable. |
| [Hoverfly](https://docs.hoverfly.io/en/latest/pages/keyconcepts/modes/capture.html) | Service virtualization; capture/simulate | Captures HTTP(S) traffic and later simulates it | The principal artifact is a simulation. WireAssume treats captured traffic as experimental input and derives assumptions by mutation plus an oracle. | Separate capture from simulation/replay and make mode boundaries explicit. |
| [mitmproxy](https://docs.mitmproxy.org/stable/mitmproxytutorial-replayrequests/) | Interactive proxy; traffic replay | Records flows, supports replay, and permits editing before replay | A powerful traffic workbench, but not an automated consumer-dependency miner. | Favor deterministic, scriptable replay artifacts and avoid unsafe transparent/TLS defaults. |
| [Schemathesis](https://schemathesis.io/) | Schema-driven property-based API fuzzing | Systematically generates edge cases; minimizes reproducible failures; CI-friendly | It primarily derives tests from provider schemas and checks provider/API properties. WireAssume mutates provider *responses* and judges the *consumer workflow*. | Use deterministic mutation planning and aggressive shrinking/minimization, while keeping the oracle consumer-facing. |
| Delta debugging (Zeller & Hildebrandt, 2002) | Failure-inducing input minimization | Divide-and-conquer minimization can isolate the smallest relevant input | Classic delta debugging minimizes inputs that preserve failure. WireAssume often needs the dual question: the smallest subset of provider behavior that preserves consumer success, plus minimal mutations that preserve a specific failure. | Implement a real ddmin-style engine behind an oracle interface; record every trial as evidence. |
| OpenAPI / JSON Schema | Provider specification | Describes documented response structure and constraints | A provider contract states what *may* be returned; it cannot prove what a particular consumer actually tolerates. | Compare observed requirements with provider guarantees, but never infer consumer requirements from the spec alone. |
| API mocking generally | Mocking | Allows controlled provider responses | Hand-authored mocks reproduce what developers thought to test. | Generated mutations should be traceable back to real recorded interactions. |
| Traffic replay generally | Record/replay | Reproduces real integration traffic | Reproduction alone does not identify which response dimensions are semantically necessary to the consumer. | Make replay a primitive used by experiments, not the end product. |
| Specification mining | Program analysis / dynamic inference | Infers models from executions | Many approaches infer regularities in observed traces; observed regularity is not equivalent to consumer necessity. | Require counterfactual experiments: change one behavior, run the consumer, observe pass/fail. |

## What WireAssume deliberately does differently

WireAssume's primary loop is:

```text
Record
→ Normalize
→ Generate controlled mutations
→ Replay
→ Execute the real consumer workflow
→ Observe the oracle
→ Minimize required behavior
→ Classify assumptions
→ Persist evidence
```

The differentiator is the counterfactual step. A field appearing in every capture is only a candidate dependency. It becomes an observed consumer requirement only after a controlled experiment shows that removing or changing it causes the selected consumer workflow to fail, with evidence retained for that outcome.

### Compared with Pact

Pact correctly centers contracts on consumer needs and encourages loose matching where consumers do not care about exact values. Its contract is generated from consumer-authored tests. WireAssume takes the next step for legacy, third-party, or under-tested integrations: it attempts to *discover* those needs by perturbing real responses and executing the real consumer.

WireAssume should therefore interoperate conceptually with Pact-like output, but must not pretend experimental findings are authored promises or perfectly convertible into Pact semantics.

### Compared with record/replay and service virtualization

WireMock, Hoverfly, and mitmproxy establish that capture/replay is a practical development primitive. WireAssume uses the same primitive to create a stable experimental baseline, then layers deterministic mutation IDs, an oracle, evidence, minimization, and contract inference on top.

For v0.1.0 the proxy is intentionally reverse-proxy-first. TLS interception is not required for the core experiment and must never silently install certificates.

### Compared with API fuzzing

Schema-driven fuzzers such as Schemathesis are excellent at asking whether an API implementation satisfies safety and schema-derived properties under generated requests. WireAssume asks a different direction of compatibility question: if a provider returns a response that is still plausible—or deliberately adversarial within a configured local experiment—does the consuming workflow survive?

The two approaches are complementary. OpenAPI input helps WireAssume classify discoveries (for example, “consumer requires a provider-optional field”), but OpenAPI is not the experimental oracle.

### Compared with specification mining

Passive trace mining can infer shapes, enums, frequencies, and correlations. WireAssume uses passive observations only to generate candidates. Necessity requires an intervention and a consumer outcome. This distinction is central to avoiding false certainty from finite traffic samples.

## Delta debugging notes

The original delta-debugging formulation automatically simplifies failure-inducing inputs while preserving the failure. WireAssume uses the same family of algorithms in two places:

1. **Failure minimization** — reduce a mutation or changed response while preserving the same consumer failure.
2. **Success-preserving contract minimization** — remove response elements while the consumer workflow still passes, leaving a locally minimal response subset for the tested scenario.

The second operation is not proof of global minimality across all possible workflows. Results are scoped to the recorded provider interaction, scenario, source revision, mutation policy, seed, and oracle.

## Design consequences for v0.1.0

- Evidence is immutable and content-addressed where practical.
- Deterministic canonical serialization is used for IDs and reports.
- Mutation generation is modular and budgeted.
- An assumption points to the exact mutation trials that support it.
- “Observed” is the default confidence level; wording avoids claiming universal guarantees.
- Provider-spec comparison is a separate classification stage.
- Command and HTTP oracles are core; Playwright is an adapter rather than a dependency of the mutation engine.
- Reverse proxy mode ships before any TLS MITM mode.
- Secrets and common credential headers are redacted before persistence.

## Sources

- Pact documentation: https://docs.pact.io/
- Pact consumer guidance: https://docs.pact.io/consumer
- WireMock record/playback: https://wiremock.org/docs/record-playback/
- WireMock proxying: https://wiremock.org/docs/proxying/
- Hoverfly capture mode: https://docs.hoverfly.io/en/latest/pages/keyconcepts/modes/capture.html
- Hoverfly simulate mode: https://docs.hoverfly.io/en/stable/pages/keyconcepts/modes/simulate.html
- mitmproxy replay tutorial: https://docs.mitmproxy.org/stable/mitmproxytutorial-replayrequests/
- Schemathesis: https://schemathesis.io/
- A. Zeller & R. Hildebrandt, “Simplifying and Isolating Failure-Inducing Input,” IEEE TSE 28(2), 2002, DOI 10.1109/32.988498.
