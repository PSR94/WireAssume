# OrbitDesk × PeopleCRM vertical slice

This is WireAssume's first real end-to-end demo consumer.

PeopleCRM's OpenAPI document declares `customer.email` optional and nullable. OrbitDesk nevertheless indexes `email` directly and rejects `null`, creating an intentional hidden consumer dependency. It deliberately tolerates an empty email string and unrelated response fields so WireAssume has both failing and passing counterfactual controls.

The recorded corpus is checked in under `demo/.wireassume/corpus/` with stable fixture metadata. The findings are **not** checked in: `wireassume analyze` must execute OrbitDesk against controlled replay mutations and infer them from the real command-oracle outcomes.

From the repository root:

```bash
cargo build --workspace --locked
cd demo
rm -rf .wireassume/runs .wireassume/evidence consumption.lock.yml
../target/debug/wireassume --config wireassume.yml analyze --scenario customer-profile --source-revision demo-v1
python3 verify_demo.py
```

Expected behavioral result (IDs/counts come from the implementation, not this README):

- baseline: pass
- `body.email` removed: fail
- `body.email = null`: fail
- `body.email = ""`: pass
- harmless unrelated mutations: pass
- inferred email requirement: present, non-null, empty accepted
- async ddmin minimal successful response field set: `email`

`wireassume analyze` also compares those already-observed assumptions with `peoplecrm.openapi.yaml`: email presence is an undocumented consumer dependency because the provider marks it optional, and non-nullability is contradicted because the provider explicitly allows `null`. The OpenAPI document never creates consumer assumptions by itself.
