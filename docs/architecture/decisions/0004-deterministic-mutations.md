# ADR 0004: Deterministic mutations

Status: accepted

## Decision

Mutation traversal, mutation IDs, seeded randomness, and serialization must be deterministic for the same input, configuration, and seed.

## Consequence

Randomized mutators derive randomness from an explicit experiment seed. Reports record that seed. A mutation ID is computed from mutation semantics, not execution order.
