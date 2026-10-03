# EXIT-1-02 — Engine vectors on PostgreSQL (final report, PH-1 step 7c)

| Item | Value |
|---|---|
| Criterion | EXIT-1-02: 100% of the engine-profile vectors pass on PostgreSQL through the runner (signed runner reports) |
| Standard | BPA 1.0.0-rc1 (`standard/release/1.0-rc1/manifest.json`, DEC-045): 1,227 vectors of the profiles Core, Bitemporal, Ledger and Proof |
| Build | PH-1 step 7c (`cargo build --release --locked -p ubos-forge`), DEC-060 |
| Database | PostgreSQL 16.13, development workstation (4 vCPU, 15 GB RAM); the ENV-REF-SERVER run is made at gate-1a acceptance (step 9, DEC-047) |
| Status | **met on the workstation**: every vector passes on PostgreSQL and in memory with identical outcomes; the interim report of step 4 (DEC-054: 869 of 1,227) is replaced |
| Result | 1227 of 1,227 pass (Core 1038, Bitemporal 61, Ledger 55, Proof 73); 0 fail; 0 not applicable |
| Reports | `conformance-postgres.json`, `conformance-memory.json` (DAT-ConformanceReport, STD-CONF-003), signed by an ephemeral runner key |

## History

| Step | Passing | Note |
|---|---|---|
| 4d-2 (DEC-054) | 869 | interim: steps 5 and 6 not applicable |
| 5a–5c (DEC-055…057) | 1,009 → 1,127 → 1,165 | runtime |
| 6a–6b (DEC-059) | 1,203 → 1,227 | proofs and archive |
| 7c (DEC-060) | 1,227 | final report after the host, Forge and the scenarios |

## Reproduce

```
cargo build --release --locked -p ubos-forge
./target/release/forge conform --vectors standard/vectors --report conformance-memory.json
./target/release/forge conform --vectors standard/vectors --backend postgres \
    --database "host=127.0.0.1 user=postgres dbname=bpa" --report conformance-postgres.json
```
