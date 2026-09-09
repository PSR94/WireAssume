# ADR 0002: Versioned evidence-backed `consumption.lock`

Status: accepted

## Decision

The contract artifact identifies itself as `wireassume.consumption/v1`, has a JSON Schema, and stores stable evidence references rather than free-form unsupported claims.

YAML and JSON are serialization formats of the same logical model. Deterministic ordering is required so source control diffs remain meaningful.

## Consequence

Breaking contract-format changes require a new schema identifier and compatibility documentation.
