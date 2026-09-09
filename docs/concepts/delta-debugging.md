# Delta debugging and contract minimization

WireAssume uses a ddmin-family reducer behind the same kind of pass/fail/inconclusive oracle used by experiments.

## Two directions

**Failure minimization** starts with a mutation known to fail and removes pieces while the same failure-class outcome remains. This produces a smaller counterexample.

**Success-preserving contract minimization** starts with a response that passes and removes fields or behaviors while the consumer workflow still passes. The remaining set is a locally minimal requirement set for that exact scenario and revision.

## Algorithm

Given `c` and target outcome `T`:

1. Verify `test(c) == T`.
2. Split `c` into `n` partitions (start with `n = 2`).
3. Test each complement. If a complement still yields `T`, keep it and reduce granularity.
4. Otherwise test individual partitions. If one yields `T`, keep it and restart at `n = 2`.
5. If nothing reduces and finer granularity remains, double `n`.
6. Stop when no single tested chunk/complement can reduce the current candidate.

The result is 1-minimal with respect to the tested decomposition, not necessarily a globally minimum set.

## Complexity

The number of oracle executions depends on how often reductions succeed. Best cases reduce large chunks quickly. Adversarial cases require progressively finer partitions and substantially more runs. The reducer exposes `tests_executed` because consumer-oracle time—not in-memory set operations—is normally the dominant cost.

WireAssume therefore uses this reducer after mutation planning has already constrained the experiment space; it is not a license to brute-force the power set of a response.

## Determinism and flaky oracles

The v0.1 core treats `Unresolved` as a non-target outcome and does not silently reinterpret it. Retry policy belongs to the experiment runner and must be recorded as evidence. Future statistical/flaky-oracle strategies should be explicit modes because they change confidence semantics.
