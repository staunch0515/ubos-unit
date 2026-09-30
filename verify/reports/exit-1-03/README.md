# EXIT-1-03 — Storage suite on PostgreSQL

| Item | Value |
|---|---|
| Criterion | EXIT-1-03: the storage suite passes 100% on PostgreSQL, and the storage trait is free of backend-specific types (inspection) |
| Verification item | VER-CONF-8402 (SUITE-DVM-STORE) |
| Build | PH-1 step 3c (`cargo build --release -p ubos-dvm --bin ubos-store-suite`) |
| Database | PostgreSQL 16.13, development workstation (4 vCPU, 15 GB RAM); the ENV-REF-SERVER run is made at gate-1a acceptance (DEC-047) |
| Result | 20 of 20 cases pass (`storage-suite-postgres.json`) |

## Cases

| Case | Verifies | Evidence in the report |
|---|---|---|
| `transactions.rollback` | DSN-DVM-001 | versions and settings written in a rolled-back transaction are not visible |
| `transactions.head_cas` | DSN-DVM-208 | a second compare-and-set against the old head fails |
| `transactions.group_abort_alone` | DSN-DVM-006 | in a group of five commits, only the one with a stale base fails |
| `snapshot.reads` | DSN-DVM-004, -008 | a read at an earlier `cseq` returns the earlier body after a later commit |
| `indexes.verified` | DSN-DVM-005, -092 | `verify_indexes` finds no mismatch on `main` and `feature/x` after the workload |
| `indexes.corruption_detected` | DSN-DVM-092 | a `temporal_index` row with a changed version is reported by object key; clean again after restoring |
| `sequences.gap_free_with_failures` | DSN-DVM-402 | 10,000 appends to one key, 534 flush transactions rolled back after allocation; the 9,466 committed entries are dense, chained and match the counter |
| `sequences.concurrent_appends` | DSN-DVM-404 | 200 concurrent appends to one cash account all succeed with consecutive sequences |
| `sequences.branches` | DSN-DVM-402 | sequences of `main` and of a child branch (inherited counts, merge) verify |
| `chunks.placement` | DSN-DVM-003 | a small chunk is inline; a 3 MB chunk is in the blob store with only its descriptor in the database; both read back |
| `chunks.corruption_detected` | DSN-DVM-002 | a modified byte or a chunk presented under another content ID raises `NOD.BLOB_UNAVAILABLE` with `{"integrity": true}` |
| `physical.two_tenants` | DSN-DVM-002 | identical content has the same content ID and different stored bytes in two tenants; one tenant's key cannot open the other's |
| `physical.version_sealed` | DSN-DVM-002 | stored version envelopes contain no plaintext and read back through the engine |
| `tenancy.no_tenant` | DSN-DVM-010 | a session of `ubos_app` without `ubos.tenant` sees 0 rows in all 15 tables |
| `tenancy.cross_tenant` | DSN-DVM-010 | tenant B sees none of tenant A's rows and cannot write rows for tenant A |
| `tenancy.ledger_immutable` | DSN-DVM-010, STD-KIND-020 | deleting a ledger entry as owner raises `LEDG.IMMUTABLE_ENTRY`; updating as `ubos_app` is denied |
| `tenancy.partitions` | DSN-DVM-010 | the five large tables have 64 hash partitions each |
| `roots.verified` | DSN-DVM-007, -092 | every commit's state root recomputed from its state equals the recorded root; all reachable tree nodes hash to their identifiers |
| `roots.corruption_detected` | DSN-DVM-092 | a tampered root node is reported |
| `inspection.backend_neutral_traits` | DSN-DVM-001, VER-CONF-8402 | automated aid: `kernel.rs` names no sqlx, PostgreSQL, Tokio or SQLite type |

## Inspection record (VER-CONF-8402 step 2)

The storage traits `StoreTx` and `StoreRead` in `crates/ubos-store/src/kernel.rs` were
inspected on 2026-09-30:

- all parameters and results are `i64`, `u64`, `u32`, `u16`, `bool`, `String`, `&str`,
  byte vectors, the 32-byte `Hash`, `Option`, `Vec`, and the module's plain row structs
  (`BranchRow`, `CommitRow`, `ChunkRow`, `VersionRow`, `LedgerRow`, `SequenceRequest`,
  `Allocated`, `IndexRow`, `NodeRow`, `OutboxRow`, `IdempotencyRow`, `EntryFilter`,
  `StateRow`, `HeadRow`, `TemporalRow`, `EntryMeta`, `CounterRow`);
- errors are `ubos_types::BpaError`;
- the operations are `async` without naming a runtime; a transaction is consumed by
  `commit` or `rollback`;
- no type of sqlx, PostgreSQL, Tokio or SQLite occurs in the module (also checked by the
  case `inspection.backend_neutral_traits`).

Verdict: the traits are backend-neutral; the SQLite adapter of PH-4 (DEC-024) can
implement them without a trait change. Row-level security, partitions and the stored-byte
format are properties of the PostgreSQL adapter and the physical layer, not of the traits.

## Reproduce

```sh
cargo build --release -p ubos-dvm --bin ubos-store-suite
./target/release/ubos-store-suite --appends 10000 \
    --database "host=127.0.0.1 user=postgres dbname=bpa" --report storage-suite-postgres.json
```

The database user must be able to create the role `ubos_app` and the extension
`btree_gist` on first use (migrations, DEC-048). CI runs the suite with 1,000 appends.
