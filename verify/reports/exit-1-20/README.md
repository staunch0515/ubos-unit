# EXIT-1-20 — PH-1 traceability (VER-INSP-9500, PH-1 step 19d)

| Item | Value |
|---|---|
| Criterion | EXIT-1-20: the traceability report shows every PH-1 Must requirement covered by at least one passing verification item (UBS-PH-1 §9) |
| Verification item | VER-INSP-9500: generate the traceability report and the verification results index; list the Must requirements without a passing mapped item; pass when the list is empty |
| Result | At 19d: **24 of 323 PH-1 Must requirements without a passing result** (291 verified, 8 on the workstation only). After DEC-087 (carry-overs moved to PH-2 and PH-3): **13 of 313**, all of them reference runs — 12 benchmarks waiting for ENV-REF-SERVER and NR-PERF-011, not met on the workstation (292 verified, 8 on the workstation only) |
| Decision | DEC-085 (part 19d) |
| Sources | `docs/50-system/60-verification/30-results-ph1.md` (results index, UBS-VER-30) and the traceability (UBS-VER-22) |
| Generated | `docs/50-system/60-verification/31-results-trace-ph1.md` (UBS-VER-31) by `docs/tools/gen_results.py`, run by `regen_all.sh` |
| Date | 2026-10-18, `main` after PH-1 step 19c |

## 1. Method

1. **Traceability (UBS-VER-22).** A requirement is realised by design items (DSN **Origin**) and
   standard clauses (STD **Satisfies**). The suites of a design item's system run the item's
   verification methods; standard clauses are verified by SUITE-DVM-CONF.
2. **Results index (UBS-VER-30).** It records the PH-1 result of every suite and verification
   item, with its evidence:
   - `passed`: in CI or on the workstation, no reference environment needed;
   - `workstation`: the reference run is pending;
   - `partial`, `pending`, `not-built`, `later`.

   Requirement-level results record a requirement whose own target or acceptance was checked:
   - `met`, `met-workstation`, `pending`, `not-met`.
3. **Results by requirement (UBS-VER-31).**
   - A suite counts for a requirement only when it runs one of the verification methods the
     requirement states (a BENCH target is not verified by a CONF suite).
   - A requirement-level result takes precedence.
   - A Must requirement is verified when one counting suite or item has passed, or when its
     requirement result is `met`.

The first pass, without the method rule, showed 6 gaps. With the rule, the performance targets
that conformance suites had appeared to cover became gaps (NR-PERF-001…010). Every gap was then
examined against the code and the reports, and the result was recorded at the requirement level.

The coverage of the 291 verified requirements is structural: a passing suite of the right method
verifies a design item that realises them. It is not a per-requirement review of each test. The
per-requirement examination was made for the gaps, and for every requirement-level result above.

## 2. Summary

| PH-1 Must requirements | Verified | Verified on the workstation only | Gaps |
|---|---|---|---|
| 323 | 291 | 8 | 24 |

Verified on the workstation only (reference run pending):
- FR-ANL-014, FR-RULE-024, NR-PERF-008, NR-PERF-009 (benchmarks, ENV-REF-SERVER);
- FR-TEN-024 (fairness);
- FR-OPS-011, FR-OPS-012, NR-DUR-004 (installation and backup retention by SUITE-NOD-OPS, ENV-REF-SMALL).

## 3. Gaps

### 3.1 Benchmarks waiting for the reference environment (12)

| Requirement | Pending run |
|---|---|
| NR-PERF-001, NR-PERF-002, NR-PERF-004, NR-PERF-005, NR-PERF-006, NR-PERF-007, NR-PERF-010 | VER-BENCH-0100 on ENV-REF-SERVER with WL-GENERIC-10M (EXIT-1-09) |
| FR-LEDG-014 | appends on ledgers of 1,000 and 100,000,000 entries |
| FR-VER-043, FR-VER-045 | diffs of 1,000,000 changes and diff cost by change |
| FR-PROOF-013 | the storage of a changed 10 MB document (chunks work; not measured) |
| FR-EVT-034 | tenant A with 100,000 queued jobs and tenant B with 10 (limits and priorities pass functionally in SUITE-NOD-JOBS) |

These close with the rental of ENV-REF-SERVER that EXIT-1-09 already needs. FR-PROOF-013 and
FR-EVT-034 need small additions to the benchmark drivers first.

### 3.2 Not met on the workstation (1)

| Requirement | Result |
|---|---|
| NR-PERF-011 | commit to handler start under WL-MIXED-OLTP at 70 % of one VAE's capacity: p95 578 ms against 500 ms (p99 659 ms within 2 s), `verify/reports/exit-1-17/edge/` |

One actor per VAE serialises reads and commits, so handlers queue behind the load. A process
also reads its branch's whole state (the 19c finding). The ENV-REF-SERVER run decides the gate.

### 3.3 Not built in PH-1 (8)

| Requirement | What is missing | What exists |
|---|---|---|
| CR-SOC2-003, NR-SEC-004 | TLS for external and node-to-node traffic: the Server listens on plain HTTP and WebSocket, and its packages have no TLS-terminating proxy | encryption at rest (XChaCha20-Poly1305 chunks, sealed backups), Ed25519 signatures, SHA-256 content IDs |
| FR-LEDG-031, FR-LEDG-033 | declared balance projections (group-by, `sync` and `async` maintenance, watermarks, `min_commit`) | balances computed from entries at read, property `balance_bitemporal` |
| FR-DEV-011 | `forge new` and `forge pack` | `init`, `check`, `types`, `test`, `conform`, `dev` |
| FR-ANL-021 | projection rebuild (its only design item, DSN-NOD-344, is PH-2) | field index rebuild |
| FR-TEN-031 | tenant customisation lines on overlays (its only design item, DSN-NOD-407, is PH-2) | overlay branches in the kernel |
| NR-OBS-002 | a documented metrics catalogue (its design items, DSN-NOD-612 and DSN-DVM-514, are PH-2) | OTLP metrics of the Server |

Each needs a decision of the owner: build in PH-1, or move to PH-2 by a DEC record with a
**Phase note**.
- FR-ANL-021, FR-TEN-031 and NR-OBS-002 already have only PH-2 design items.
- FR-LEDG-031/033 are what the finance Buk of PH-2 needs.
- TLS is a security baseline of the Server, needed before any deployment outside the
  workstation.

### 3.4 Acceptance tests not written (2)

| Requirement | Missing test |
|---|---|
| FR-FILE-011 | replace an attachment and read both files by version |
| FR-TXN-032 | an idempotent retry after the retention-period clock advance and after a failover |

Both are small additions to SUITE-NOD-EVENTS and SUITE-NOD-JOBS.

### 3.5 Needs a later version (1)

| Requirement | Why |
|---|---|
| FR-STD-042 | importing a 1.0 export into a later minor version needs that version; 1.0 archives round-trip today (`verify/reports/exit-1-12/`) |

## 4. Corrections of the traceability found on the way

Ten Must requirements are realised in the traceability only by design items of systems PH-1 does
not build (Notary, federation, Control plane, Web workspace) or of PH-2. They were verified at
the requirement level from the PH-1 evidence:
- CR-GDPR-001, FR-AUD-011 and NR-PRIV-006 (privacy and the audit trail);
- CR-SEC-002 (recording and time-dating);
- FR-EVT-024, FR-EVT-041, FR-PKG-071 and FR-VER-072 (handlers, schedules, genesis, revert);
- FR-DEV-063 (SDK versioning).

Their design mapping can be completed with the PH-1 design items that realise them. This is a
specification edit for the owner to schedule (not done here).

## 5. After DEC-087

The owner moved the requirements of §3.3, §3.4 and §3.5 out of PH-1 (DEC-087, each with a
**Phase note**), as follows:
- the process reading its branch on demand, FR-FILE-011 and FR-TXN-032 to the new PH-2 step 0b;
- FR-LEDG-031, FR-LEDG-033 and FR-ANL-021 to step 3;
- `forge new` and `forge pack` of FR-DEV-011 to step 7 (the PH-1 part of FR-DEV-011 is `met`);
- CR-SOC2-003, NR-SEC-004, FR-TEN-031 and NR-OBS-002 to step 11;
- FR-STD-042 to PH-3.

The regenerated results (UBS-VER-31) leave 13 gaps of 313 PH-1 Must requirements: the 12
benchmarks of §3.1 and NR-PERF-011. They close with the reference runs on the frozen line
`release/ph1`. EXIT-1-20 is then met.

## 6. Reproduce

```
bash docs/tools/regen_all.sh          # runs gen_results.py; prints the PH-1 results line
python3 docs/tools/gen_results.py --json
```
