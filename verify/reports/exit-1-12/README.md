# EXIT-1-12 — Portable export and import round trip (VER-CONF-8403)

| Item | Value |
|---|---|
| Criterion | EXIT-1-12: a portable export/import round trip between PostgreSQL VAEs yields identical roots (NR-PORT-003, STD-PROOF-070) |
| Verification item | VER-CONF-8403 (SUITE-DVM-STORE); DSN-DVM-950, DSN-DVM-951 |
| Result | **pass: 3 of 3 VAEs identical** after import and after re-import, every archive verified by the independent verifier |
| Build | PH-1 step 6c (`cargo build --release --locked -p ubos-dvm --bin ubos-roundtrip -p ubos-verify`) |
| Tool | `ubos-roundtrip` (crate `ubos-dvm`); verifier `ubos-verify` (`verify/ubos-verify/`, depends on `ubos-proof` only) |
| Database | PostgreSQL 16, development workstation (4 vCPU, linux x86_64, Rust 1.94); the ENV-REF-SERVER run with 20 VAEs including WL-GENERIC-10M is made at gate-1a acceptance (step 9) |
| Report | `roundtrip.json`, signed by an ephemeral runner key; whole run 29.5 s |

## 1. Procedure

For each VAE the tool runs in one PostgreSQL database:

1. **Build** a dataset on a fresh VAE scope: 200 objects of two definition classes
   (`Item`, `Customer`) created in batches, 300 ledger entries of `StockMove` over
   3 months, 10% of the customers updated each month, a feature branch
   per month with two new customers merged back into `main`, a `release/v1` branch and a tag
   per month (20 commits, 5 branches, 3 tags).
2. **Export** the VAE (`export_archive`, format `ubos.archive.v1`) and write the archive to a
   file; run `ubos-verify archive <file>` on it (manifest signature, every content ID, commit
   signatures and parents, branch heads and head roots, counts).
3. **Import** the archive into a second, empty VAE scope of the same database
   (`import_archive`: the same checks without a store, then the commits loaded in parent
   order with every state root and head root recomputed).
4. **Compare** the source and the imported VAE at the same adapter clock: `export_state`
   (root and entries) and `branch_info` of every branch, `tenant_root`, `tag_info` of every
   tag, `read_ledger` of every ledger key, and `read` of every object of every class on
   every branch.
5. **Re-export** the imported VAE and compare the archive with the first one byte for byte
   (the same node key signs both, so even the signatures agree); **import** it into a third
   VAE scope and compare again as in step 4.

A VAE passes when no compared member differs after either import and the second export is
identical to the first.

## 2. Results

| VAE | Commits | Versions | Entries | Branches | Tags | Archive KB | Export ms | Import ms | Compared branches / objects / ledgers / tags | `ubos-verify` exit | Second export | Result |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 20 | 249 | 300 | 5 | 3 | 1700 | 212 | 242 | 5 / 1018 / 50 / 3 | 0 | identical | pass |
| 2 | 20 | 261 | 300 | 5 | 3 | 1720 | 211 | 257 | 5 / 1018 / 50 / 3 | 0 | identical | pass |
| 3 | 20 | 253 | 300 | 5 | 3 | 1702 | 211 | 285 | 5 / 1018 / 50 / 3 | 0 | identical | pass |

The compared objects are the reads of every object on every branch (the heads of `main`,
`release/v1` and the three feature branches), the ledgers every `StockMove` key on `main`.

## 3. Verifier

`ubos-verify` is the independent verifier of DEC-028 (`verify/ubos-verify/`, Apache-2.0):
a command line and a library over `ubos-proof` with no kernel dependency. It verifies an
inclusion proof (`inclusion`), a consistency proof (`consistency`), a signature over a
content ID (`signature`) and an archive (`archive`), and prints a structured report
(DSN-DVM-938): each check with pass or fail and, for a failure, the registered error code
and the reason naming the object. A tampered chunk in one of the archives of this run gave:

```
{ "check": "chunks", "code": "PROOF.VERIFICATION_FAILED",
  "detail": "chunk sha256:0c95ccca… does not match its content ID", "result": "fail" }
ubos-verify: archive NOT verified (failed: chunks)
```

## 4. Reproduce

```
cargo build --release --locked -p ubos-dvm --bin ubos-roundtrip -p ubos-verify
./target/release/ubos-roundtrip --database "host=127.0.0.1 user=postgres dbname=bpa" \
    --vaes 3 --objects 200 --entries 300 --months 3 \
    --verifier ./target/release/ubos-verify --report roundtrip.json
```

CI runs the same tool with a smaller dataset on every push (`.github/workflows/ci.yml`).
