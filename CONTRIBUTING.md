# Contributing to WireAssume

Thanks for helping improve WireAssume. The project has one non-negotiable design rule: **deterministic experiment evidence is the source of truth.** New inference behavior must be explainable from recorded traffic, controlled mutations, real consumer-oracle outcomes, and persisted evidence.

## Development setup

Prerequisites:

- Rust 1.98.x (the repository is pinned by `rust-toolchain.toml`)
- Python 3 for the OrbitDesk demo verifier
- Git

Clone and validate the workspace:

```bash
git clone https://github.com/PSR94/WireAssume.git
cd WireAssume
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Run the end-to-end demo before proposing changes to replay, experiments, inference, minimization, OpenAPI comparison, or reports:

```bash
cargo build --workspace --locked
cd demo
rm -rf .wireassume/runs .wireassume/evidence consumption.lock.yml
../target/debug/wireassume --config wireassume.yml analyze --scenario customer-profile --source-revision local-dev
python3 verify_demo.py
```

## Change guidelines

- Keep mutation planning deterministic for the same corpus, configuration, seed, and source revision.
- Treat oracle errors/timeouts as inconclusive unless the configured oracle explicitly proves consumer failure.
- Do not infer requirements from frequency, OpenAPI, source-code heuristics, or an LLM alone.
- Do not weaken redaction, payload limits, loopback defaults, or reverse-proxy safety controls to make a demo easier.
- Do not commit fabricated findings, generated `consumption.lock` results as fixtures of truth, or screenshots that were not produced by a running product.
- Prefer stable IDs derived from canonical fingerprints over random identifiers for evidence-addressable artifacts.
- Keep unsupported provider-schema shapes `unknown` rather than guessing.

## Tests

Unit tests should cover deterministic behavior and edge cases. Integration changes should add or extend a workflow that executes the real path being claimed. A green unit test is not enough to claim an end-to-end capability.

For Rust changes, the required local checks are exactly the permanent CI gates:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

## Commits and pull requests

Use focused commits with descriptive messages. Conventional-style prefixes such as `feat:`, `fix:`, `docs:`, `test:`, `ci:`, and `refactor:` are encouraged but not mechanically required.

A pull request should explain:

1. the behavior being changed;
2. why the change preserves deterministic ground truth;
3. what evidence/tests validate it;
4. any new limitations or unsupported cases.

If behavior changes the `consumption.lock` schema or compatibility guarantees, update the schema, docs, changelog, and migration/compatibility notes together.

## Security issues

Do not open public issues containing credentials, private captured traffic, or an unpatched vulnerability with exploit details. Follow `SECURITY.md` for responsible reporting and safe-use boundaries.

## License

By contributing, you agree that your contribution may be distributed under the repository's Apache-2.0 license.
