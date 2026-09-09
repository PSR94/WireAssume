# ADR 0005: Delta-debugging family minimization

Status: accepted

## Decision

Implement an oracle-driven ddmin-style minimizer and use it for both failure reduction and success-preserving response reduction.

## Scope

A minimized result is local to a scenario, captured interaction, consumer revision, oracle, and mutation policy. It is not a proof that no other workflow depends on removed behavior.
