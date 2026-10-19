# PH-2 step 0b-1b — commit checks of references and relationships on indexes (DEC-088)

| Item | Value |
|---|---|
| Finding | Part 0b-1 (`verify/reports/ph2-0b1/`): the commit path (`commit_on`, DEC-050 item 3) loads the whole branch state when the registry declares references or relationships, when class extensions or sheets are in force, or when the commit writes one of them. Every commit of a finance model would read every object of its branch. |
| Change | Relationship fields get reference rows in the field index (`@ref:<field>`, the `relationship_index` of DSN-DVM-109), written at flush and archive import, and built for existing data when a VAE opens or its declarations change. A commit on a branch without a parent line reads only what its checks need: URIs resolve from the slug index at the base, in bulk; the sources of an ended target come from the reference rows; cardinality is checked for the targets the commit can change; class extensions and sheets are read by class. Merges, reverts and branches with a parent line keep the full state. |
| Result | **On a model with relationships, a commit on a branch of 300,000 objects takes 22–33 ms p50 (28–42 ms p95), the same as on 30,000 objects.** On the full state the same commits take 4.1 s p50 at 30,000 objects (7.5 s for ending a customer), and more than 20 minutes each at 300,000 objects (the run was stopped). |
| Semantics | Unchanged: conformance 0 failures in memory and on PostgreSQL, replay corpus identical, simulation, properties, scenarios and node suites pass (table below) |
| Environment | development workstation (4 vCPU, linux x86_64), PostgreSQL 16, release build, 2026-10-19; `ubos-bench refs` (`bench-refs-pg-300k.json`, `bench-refs-pg-30k.json`) |

## The benchmark

`ubos-bench refs --objects N --backend postgres` loads, in commits of 1,000 objects:
- `Customer`;
- `SalesOrder`, with a `many_to_one` reference `customer` (`on_end` restrict);
- `Contact`, with a `one_to_one` reference `account` to a customer.

A third of the objects are customers, the rest orders; one customer in a hundred has a
contact. Each measure is one commit of one change, repeated `--rounds` times on the reference
rows (`indexed`) and `--full-rounds` times on the full branch state (`full`, forced through
`index_since`):

| Measure | What the checks do |
|---|---|
| `create_order` | resolve the customer URI (reference integrity) |
| `update_order` | the same, for a new version |
| `create_contact` | one-to-one cardinality: the other sources of the customer |
| `end_free` | `on_end`: find the sources of a customer without orders (none) |
| `end_referenced` | `on_end` restrict: a customer with orders, rejected `MODEL.REFERENCE_RESTRICTED` |

## Results (PostgreSQL, ms)

| Measure | 300,000 objects, indexed p50 / p95 / p99 (100) | 30,000 objects, indexed p50 / p95 (100) | 30,000 objects, full state p50 / p95 (10) |
|---|---|---|---|
| `create_order` | 30.1 / 40.2 / 45.1 | 27.5 / 35.7 | 4,111 / 4,205 |
| `update_order` | 30.8 / 39.4 / 43.4 | 28.4 / 36.4 | 4,052 / 4,438 |
| `create_contact` | 32.9 / 41.9 / 45.8 | 30.2 / 38.7 | 4,103 / 4,483 |
| `end_free` | 28.0 / 36.5 / 44.1 | 29.2 / 36.8 | 7,531 / 8,995 |
| `end_referenced` | 22.0 / 27.4 / 30.2 | 21.3 / 28.3 | 4,085 / 4,365 |

The indexed commits do not grow with the branch. The full-state commits grow with it: at
300,000 objects one commit read every object's version from the store, more than once, and
took over 20 minutes. That run was stopped and is not in the report files.

Loading the 300,000-object dataset took 1,120 s, in 301 commits of 1,000 objects each. Most
of a large commit's time goes to the rule cascade (`rules::apply_defaults` and
`rules::enforce`), which reads the transaction's state for every created object. That cost
is older than this part and is not changed by it.

## Verification

| Suite | Result |
|---|---|
| Conformance, BPA 1.0-rc7, in memory and on PostgreSQL | 1,706 vectors, 0 failures |
| `cargo test -p ubos-forge -p ubos-dvm -p ubos-version -p ubos-store` on PostgreSQL | passed |
| Reference rows for data written before a relationship existed | checked by hand: a `relationship` added to an existing reference field gets rows when the declarations load, and ending the referenced customer is then rejected `MODEL.REFERENCE_RESTRICTED` |
| Conformance, `prototype` tag | 0 failures |
| Replay corpus | 10,000 of 10,000 outcomes and 1,000 of 1,000 replays identical |
| Simulation | 2,000 seeds, 0 violations |
| Properties | 10,000 cases each, 0 failures |
| L2 escape | 382 of 382 blocked |
| Storage, fault | 20 of 20 cases; 40 SIGKILL events, 0 findings |
| Scenarios | 28 of 28 in memory, on PostgreSQL and over UBTP (WebSocket and HTTP) |
| Node | node suite 12 of 12; `cargo test -p ubos-node` (SUITE-NOD-PROTO, -AUTH, -EVENTS, -JOBS) passed; SUITE-NOD-ISO 544 of 544 |
| Benchmark tools | `ubos-bench` small dataset and `ubos-edge-bench --quick` run |
| fmt, clippy, `check_deps.py`, `regen_all.sh` | clean |

## Notes

1. **Cardinality.** On the index path, cardinality is checked for the targets this commit can
   change, since the committed state already satisfies it. Suppose a model adds a one-to-one
   rule to data that already breaks it. The full state rejected every later commit of the
   branch. Now only the commits that touch an offending target are rejected (DEC-088).
2. **Unique keys.** The 0b-1 report listed uniqueness among the checks that read the whole
   branch. They do not: the commit path checks no unique keys today. The `keys` of a class are
   indexed, but `MODEL.KEY_CONFLICT` is raised only by `release`, so DSN-DVM-111 is not
   enforced at commit. This is reported to the owner as a separate finding.
3. **Stores without field indexes.** These keep the full state; the simulation store is one.

## Reproduce

```
cargo build --release --locked -p ubos-dvm --bin ubos-bench
psql -h 127.0.0.1 -U postgres -c "CREATE DATABASE refs300"
./target/release/ubos-bench refs --objects 300000 --rounds 100 --full-rounds 0 --backend postgres \
  --database "host=127.0.0.1 user=postgres dbname=refs300" --report bench-refs-pg-300k.json
psql -h 127.0.0.1 -U postgres -c "CREATE DATABASE refs30"
./target/release/ubos-bench refs --objects 30000 --rounds 100 --full-rounds 10 --backend postgres \
  --database "host=127.0.0.1 user=postgres dbname=refs30" --report bench-refs-pg-30k.json
```
