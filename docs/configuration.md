# Configuration reference

WireAssume reads `.wireassume.yml` by default. Use the global `--config <path>` option to select another file.

This document describes the configuration surface implemented by the current CLI. Run `wireassume doctor` after editing configuration; invalid loopback/proxy/oracle/mutation settings fail before an experiment starts.

## Minimal shape

```yaml
project:
  name: MyConsumer

provider:
  name: MyProvider

proxy:
  listen: 127.0.0.1:0

scenarios:
  - name: smoke
    traffic:
      endpoint:
        method: GET
        path: /resource/*
    oracle:
      type: command
      argv: ["./scripts/smoke-test.sh"]
    mutation:
      budget: 50
      seed: 42
```

## `project`

```yaml
project:
  name: OrbitDesk
```

`name` identifies the consumer in generated contract metadata.

## `provider`

```yaml
provider:
  name: PeopleCRM
  openapi: peoplecrm.openapi.yaml
```

`openapi` is optional. When present, `analyze` loads the document after the consumer experiment and compares only assumptions that were already derived from actual counterfactual failures. The provider schema never creates a consumer requirement by itself.

Relative OpenAPI paths are resolved relative to the configuration file.

## `proxy`

```yaml
proxy:
  listen: 127.0.0.1:0
  max_payload_bytes: 2097152
  allow_non_loopback: false
```

- `listen` controls the local record/replay listener. Port `0` asks the OS for an available port.
- `max_payload_bytes` bounds captured request/response bodies.
- non-loopback listening is rejected unless `allow_non_loopback` is explicitly enabled.

Recording is reverse-proxy-first. WireAssume does not install certificates or modify a trust store.

## `scenarios`

Each scenario scopes traffic, the consumer oracle, and mutation policy.

```yaml
scenarios:
  - name: customer-profile
    traffic:
      endpoint:
        method: GET
        path: /customers/*
    oracle:
      type: command
      argv: ["python3", "orbitdesk.py"]
      cwd: "."
      timeout_ms: 5000
      inherit_env: true
      stdout_contains: ["OrbitDesk customer profile OK"]
    mutation:
      budget: 50
      concurrency: 1
      seed: 42
      include:
        - remove-field
        - null-field
        - empty-string
        - additional-unknown-property
```

Scenario names must be unique.

### Traffic endpoint

The initial matcher is deliberately conservative. `method` is matched case-insensitively and the scenario path may include `*` wildcard segments, for example `/customers/*`.

### Command oracle

A command oracle runs an explicit argv array. This avoids implicit shell interpolation.

```yaml
oracle:
  type: command
  argv: ["cargo", "test", "--test", "consumer_flow"]
  cwd: "."
  timeout_ms: 30000
  inherit_env: true
  env:
    FEATURE_MODE: integration
  accepted_exit_codes: [0]
  stdout_contains: ["consumer flow passed"]
  stderr_contains: []
```

During `analyze`, WireAssume supplies `WIREASSUME_REPLAY_BASE_URL=http://<experiment-listener>` to command-like oracles unless the oracle environment already defines it.

### HTTP oracle

HTTP oracles call a real consumer workflow endpoint and validate the response according to configured assertions. Use them when the consumer exposes a stable health/workflow endpoint rather than a command-line test.

### Playwright adapter

The current Playwright configuration is executed through command-oracle infrastructure. A native browser-step DSL and browser-native screenshots/console/network evidence are intentionally not claimed yet.

## Mutation policy

`budget` limits selected experiment mutations. `seed` controls deterministic randomized cases. `include` narrows mutation families; `exclude` can remove families. The current experiment replay controller executes trials serially even if configuration exposes concurrency metadata.

Representative mutation names include:

```text
remove-field
null-field
empty-string
whitespace-string
wrong-primitive-type
empty-array
empty-object
unknown-enum
numeric-zero
numeric-negative
numeric-large
numeric-float
unicode-string
long-string
additional-unknown-property
array-reverse
array-shuffle
array-duplicate-items
array-remove-item
status-400
status-422
status-429
status-500
status-502
status-503
status-504
redirect-302
empty-response
response-delay
```

The exact accepted names are defined by the typed `MutationKind` configuration deserializer; invalid names fail configuration parsing rather than being ignored.

## Redaction

Credential-like headers and query parameters have secure defaults. Additional simple object-key JSON paths can be configured:

```yaml
redaction:
  headers:
    - authorization
    - cookie
    - x-api-key
  query_parameters:
    - token
    - access_token
  jsonpaths:
    - $.user.secret
    - $.credentials.password
  regexes:
    - 'Bearer\s+[A-Za-z0-9._-]+'
    - 'demo-secret-[0-9]+'
```

Traffic redaction happens before persistence. Regexes are validated at configuration load and are applied to textual captured bodies, JSON string values, header values, request URIs, and persisted oracle/minimization summaries. The current JSON path implementation supports root/object-key paths such as `$.user.secret`; wildcard/array/full JSONPath remains a pre-release limitation.

## Reproducibility

For comparable runs, keep these stable together:

- the traffic corpus;
- consumer source revision;
- scenario configuration;
- mutation seed and budget;
- oracle implementation;
- WireAssume version.

The CLI records an explicit `--source-revision` when supplied. Otherwise it attempts `GITHUB_SHA`, then local `git rev-parse HEAD`, then a working-tree fallback.
