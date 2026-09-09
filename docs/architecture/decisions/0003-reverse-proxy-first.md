# ADR 0003: Reverse-proxy-first recording

Status: accepted

## Decision

Ship an explicit reverse proxy before transparent proxying or TLS interception.

## Rationale

A reverse proxy is sufficient to prove the core record → mutate → replay → oracle loop without asking WireAssume to become a certificate-management product. TLS MITM, if ever added, must be opt-in and must never install trust roots silently.
