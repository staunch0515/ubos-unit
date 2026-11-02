# BPA 1.0.0-rc13 — Release notes

Release candidate 13 of BPA 1.0 (DEC-157), the refinements that the finance Buk asked for in PH-2
step 10 (DEC-147, -151…156). Tag `bpa-v1.0.0-rc13`, signed by the owner (DSN-BPA-001). The
manifest (`release/1.0-rc13/manifest.json`) lists every released file with its SHA-256; its
`content_id` identifies this release. BPA 1.0-rc12 stays frozen as tagged. The standard precedes
the code (AR-033): the kernel implements this text in the next part, and the finance Buk adopts it
after that.

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 259 (scope by phase, `scope.json`): the clauses of BPA 1.0-rc12; none added, six changed |
| MUST clauses with a vector range | 221, 221 covered (`coverage.md`) |
| Vectors | 2,294: those of BPA 1.0-rc12 except one superseded, and 20 added |

## Changes since BPA 1.0-rc12

- **Posting dates of ledger entries** (STD-KIND-025): a ledger class may name a `date` field in `ledger.posting_date`. An entry with a value in it posts on that date; without one, on the date of its commit. A reversal posts on the date of its commit. The period check of appends and merges and the balances of close artefacts (STD-KIND-026) take each entry's posting date. The clause gains a second vector range, VER-CONF-1155…1159.
- **Explicit coordinates in a process** (STD-ISA-030, -037): a `read.get` target with selectors resolves on the process's branch unless it names another, and without `commit`, `tag` or `time` reads at the process snapshot, so `asof` alone reads the version valid at a date. `read.projection` takes `time`, the transaction time "as known at"; a time at or after the snapshot reads at the snapshot.
- **`read_only` before the change** (STD-LGS-011): the property is matched on the version that an update or end changes, so a selector over fields or state freezes the objects that already match it while the change into the match commits.
- **Business refusals in L2** (STD-CTX-020): an uncaught `throw #{code, message}` with a catchable registered code fails the execution with that code; any other thrown value fails with `LOGIC.TYPE_ERROR`. The L2 escape suite verifies it (L2E-383…389); engines without L2 fail with `LOGIC.NOT_AVAILABLE` as before.
- **Vectors of two kernel conformance fixes:** accounts of a class extending `LedgerAccount` are checked and rolled up like `LedgerAccount` itself (STD-KIND-029, DEC-155); a simulated call that committed nothing re-runs on the snapshot its journal pinned (STD-LGS-044, DEC-153).
- **Members:** the ledger declaration gains `posting_date`; `read.projection` gains `time`.

## Superseded vectors

- VER-CONF-3005 asserts the instruction catalogue of 1.0-rc12; `read.projection` takes `time`, and VER-CONF-3006 asserts the catalogue of this release.

## Known limits

- **Kernel.** The kernel runs the vectors of this release as the next part implements them.
- **Not in this release:** a logic instruction for kernel matching (STD-KIND-027, DEC-150). The finance Buk matches bank lines in its own logic over its `BankMatch` ledger, and no second package needs the instruction yet; it is left to a later release candidate (DEC-157).
