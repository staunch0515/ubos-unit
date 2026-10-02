# EXIT-1-02 — Engine vectors on PostgreSQL (interim report after PH-1 step 4)

| Item | Value |
|---|---|
| Criterion | EXIT-1-02: 100% of the engine-profile vectors pass on PostgreSQL through the runner (signed runner reports) |
| Standard | BPA 1.0.0-rc1 (`standard/release/1.0-rc1/manifest.json`, DEC-045) |
| Build | commit `BUILD_HASH` (`cargo build --release -p ubos-forge`) |
| Database | PostgreSQL 16.13, development workstation (4 vCPU, 15 GB RAM); the ENV-REF-SERVER run is made at gate-1a acceptance (DEC-047) |
| Status | **interim** (DEC-054): the areas implemented through step 4 pass completely; the areas of steps 5 and 6 are outside the claimed scope and reported as `not_applicable`. EXIT-1-02 is met when the full set passes at step 9. |
| Result | 869 of 1,227 vectors pass (Core 740, Bitemporal 61, Ledger 52, Proof 16); 0 fail; 358 not applicable |
| Reports | `conformance-postgres.json`, `conformance-memory.json` (DAT-ConformanceReport, STD-CONF-003), identical outcomes on both backends |

## Scope of this report

| Area | Vectors | Outcome |
|---|---|---|
| Foundations, object model, class system, kinds, versioning, bitemporal, ledger, transactions, addressing (STD-FND, -OBJ, -CLS, -KIND, -VER, -TIME, -TXN, -ADDR, -ERR), AEAD of STD-CONF-011 | 869 | pass |
| Runtime: `execute`, `eval_expr`, `check_expr`, `query`, `profile_limits`, `instructions`, custom value types (STD-CTX, -ISA, -EXPR, -BPU, STD-FND-026, STD-TXN-023) | step 5 | not applicable |
| Proofs and signatures: `sign`, `verify_signature`, `commit_record`, `prove`, `verify_inclusion`, `verify_consistency`, `tenant_root`, archives, node and key DIDs (STD-PROOF, parts of STD-CONF-011, STD-ADDR-010 node documents) | step 6 | not applicable |
| BPU and sync operations (`bpu_descriptor`, `bpu_state`, `verify_bpu`, `verify_asset`) | later steps | not applicable |

Each report is signed with an ephemeral ed25519 runner key; the public key is in
`signature.key`, and the signature covers the canonical JSON (STD-FND-001) of the report
without its `signature` member.

## Reproduce

```sh
cargo build --release -p ubos-forge
./target/release/forge conform --vectors standard/vectors --standard 1.0.0-rc1 --build <commit> \
    --backend postgres --database "host=127.0.0.1 user=postgres dbname=bpa" --report conformance-postgres.json
./target/release/forge conform --vectors standard/vectors --standard 1.0.0-rc1 --build <commit> \
    --report conformance-memory.json
```
