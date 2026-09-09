# Security policy

WireAssume executes consumer workflows and records/replays API traffic. Treat the workspace and all recorded artifacts as sensitive.

## Trust boundaries

WireAssume v0.1 is designed for a developer-controlled workstation or CI runner. It is **not** an internet-facing generic proxy and must not be deployed as one.

- The reverse proxy may contact only an explicitly configured upstream.
- Command oracles execute explicit argv vectors; shell interpretation is not required for the core interface.
- Oracle processes must have timeouts and inherit only intentionally supplied environment variables.
- Recorded traffic is redacted before persistence.
- Authorization, Cookie, Set-Cookie, X-API-Key, X-Auth-Token, and Proxy-Authorization are redacted by default.
- Common token/query-key names are redacted by default.
- JSON body redaction is configurable.
- TLS interception is out of scope for the initial reverse-proxy mode. WireAssume must never silently install a CA certificate.

## Sensitive traffic

API traffic may contain PII, credentials, payment data, health data, or other regulated information. Use synthetic or appropriately authorized test data whenever possible. Keep `.wireassume/` out of source control and configure additional redaction rules for application-specific secrets.

Redaction should be treated as defense in depth, not a guarantee that arbitrary payloads contain no sensitive values.

## Network safety

Do not expose the proxy to untrusted networks. Bind to loopback by default. Future server/SaaS modes require independent SSRF controls, network allowlists, authentication, workspace isolation, and command-execution trust boundaries.

## Reporting vulnerabilities

Please avoid opening a public issue for an undisclosed vulnerability that could expose user traffic or credentials. Use GitHub's private vulnerability reporting feature when enabled for this repository.
