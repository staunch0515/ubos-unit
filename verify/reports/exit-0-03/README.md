# EXIT-0-03 — BPA 0.1 prototype vectors on PostgreSQL

| Item | Value |
|---|---|
| Criterion | EXIT-0-03: the prototype passes 100% of the vectors tagged `prototype` on PostgreSQL through the runner |
| Standard | BPA 0.1.0 (`standard/release/0.1/manifest.json`, tag `bpa-v0.1.0`) |
| Build | commit `6a2215b96d3798517178fb27fb321d5ead0938b9` (`cargo build --release -p ubos-forge`) |
| Database | PostgreSQL 16.13 |
| Result | 453 of 453 vectors tagged `prototype` pass (Core 355, Bitemporal 30, Ledger 52, Proof 16); 98 class-system vectors are outside the prototype (`not_applicable`) |
| Reports | `conformance-postgres.json`, `conformance-memory.json` (DAT-ConformanceReport, STD-CONF-003) |

Each report is signed with an ephemeral ed25519 runner key; the public key is in
`signature.key`, and the signature covers the canonical JSON (STD-FND-001) of the report
without its `signature` member.

Reproduce:

```sh
cargo build --release -p ubos-forge
./target/release/forge conform --vectors standard/vectors --tag prototype \
    --backend postgres --database "host=127.0.0.1 user=postgres dbname=bpa" --report conformance.json
```
