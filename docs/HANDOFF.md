# Handoff — status of the UBOS specification work

Last updated: 2026-09-29. This file lets a new Claude session (or a new engineer) continue
the work without the earlier conversations. Read `CLAUDE.md` first for the working rules.

## 1. Background in brief

- The owner earlier built eight experimental repositories of the same platform idea with AI
  help. They diverged. The research volume (`docs/10-inventory` … `docs/40-spec`) analysed
  them; the **system specification** `docs/50-system/` replaced them as the single
  authority (DEC-001, DEC-008).
- The core idea: a public standard (BPA) for how business state changes over time — typed,
  versioned, bitemporal, governed and provable — run by a reference kernel (DVM); business
  behaviour ships as data in business packages (Buks). Principle: *harden the mechanism,
  soften the policy.*
- 13 systems: DVM, NOD, CTL, FRG, SDK, WSP, STU, AGT, EXC, NTY, BRG, FED, BPA.
- 6 phases: PH-0 foundations; PH-1 kernel and single-node server (gates 1a and 1b); PH-2
  first business package and production server; PH-3 CPA audit drill, evidence and governed
  AI; PH-4 devices, scale and ecosystem; PH-5 federation. **PH-4 and PH-5 are specified but
  not scheduled** (DEC-023).

## 2. Decisions that shape the current plan (all Accepted)

| Decision | Content |
|---|---|
| DEC-022 | The first business package is **US Basic Finance** for one company (GL, journals with approval, periods and close, AR, AP with Form 1099, bank reconciliation, fixed assets, multi-currency ASC 830, basic state sales tax, four statements, audit trail and controls). Fund operations and contract management are **Deferred** with IDs kept; contract-only capabilities move to PH-5. Excluded: payroll, inventory, consolidation, budgeting, project accounting, complex ASC 606, ASC 842. |
| DEC-023 | The product is **not for sale**; it demonstrates the capabilities the platform imposes and is compared with the owner's Java system. PH-3 acceptance is an external CPA audit drill, not a customer pilot. |
| DEC-024 | **PostgreSQL only** until PH-4; SQLite, Box and device sync move to PH-4. |
| DEC-025 | PH-2 is **Web only**, professional and practical; Smart Grid (bulk journal grid) is in PH-2; desktop and mobile apps in PH-4. |
| DEC-026 | The finance requirements are a separate, implementation-neutral English document (`docs/finance-requirements/`, IDs `FIN-*`) shared by both implementations, with a sample company, expected results, acceptance scenarios and a comparison protocol. |
| DEC-019 | Licensing: standard text CC BY 4.0; vectors, schemas, SDKs and verifier Apache-2.0; kernel, node and Forge AGPL-3.0 plus commercial; first-party Buks commercial. |
| DEC-027 | The specification and finance requirements moved with history to `staunch0515/ubos-unit`; `ubos-platform` is the archive. |
| DEC-018 | Component layout: `ubos` (kernel and clients), `bpa-standard`, `ubos-verify`, `ubos-buks`; revised by DEC-028. |
| DEC-028 | `ubos-unit` is the single implementation monorepo: `standard/`, `verify/`, `crates/`, `packages/`, `buks/` next to `docs/`; the DEC-018 names remain component names (UBS-ARC-07 §1). |
| DEC-030 | PH-0 is executed by the owner with AI sessions in eight steps (one pull request each), with PH-0 verification work requested, external review at acceptance, benchmark hardware rented for step 5 and owner-signed releases. |
| DEC-029 | Finance requirements are self-contained, **frozen at version 1.0** (tag `fin-req-v1.0`), copied unchanged into other repositories; work items recorded per `40-comparison-protocol.md` §2 (system codes U and J). |
| DEC-034 | The Rhai sandbox spike blocks 100 of 100 initial escape cases; ASM-008 is confirmed for L2 with conditions (empty engine with pure packages, per-execution memory accounting, uncatchable host errors, fine-grained time checks), recorded in DSN-DVM-612. |
| DEC-035 | EXIT-0-05 (storage benchmark) is carried over into PH-1 because hosts cannot be rented now; it is due before the PH-1 storage layout is fixed, at the latest at gate 1a. |
| DEC-036 | PH-1 re-baselined after PH-0: every finding dispositioned (UBS-PHR-0), work items W-1…W-12, OQ-007 and OQ-008 resolved, RSK-004 closed, RSK-016 and RSK-017 added. |
| DEC-037 | PH-0 accepted with EXIT-0-05 carried over; external review together with the carried benchmark. |
| DEC-038 | PH-1 executed by the owner with AI sessions: gate-1a steps 0–9, verification requested, ENV-REF-SERVER rented for W-1 and gate 1a, GitHub-hosted CI incl. nightly and ARM64/macOS runners. |
| DEC-039 | BPA 1.0-rc1 scope by rule (0.1 plus Core, Bitemporal, Ledger, Proof clauses serving gate 1a), checked by the builder; BPA 0.1 frozen; regen builds rc1; step 1 in parts 1a–1f. |
| DEC-040 | BPA 1.0-rc1 object and class model clarified: ports and dispatch, selectors, extensions (genesis `ClassExtension`), relationships, reference integrity, blind indexes; new adapter operations. |

The full log with context and alternatives is `docs/50-system/90-decisions/000-index.md`.

Values confirmed by the owner (2026-09-29): Form 1099-NEC/MISC threshold for tax year 2026
= USD 2,000.00; Texas, City of Austin combined sales tax rate = 8.25%.

## 3. What exists

**System specification (`docs/50-system/`)** — about 250 documents, fully cross-checked:
33 personas, 220 capabilities, 67 scenarios (19 deferred), 800 FR, 106 NR, 54 CR, 239
standard clauses, 695 design items, 118 interfaces, 85 exit criteria (7 withdrawn), 77
suites, 90 verification items, 30 decisions. Traceability has 0 Must gaps in every phase.
Annex `10-requirements/10-finance-mapping.md` maps the finance requirements to the platform.

**Finance requirements (`docs/finance-requirements/`, version 1.0, frozen)** — 153 requirements (130 Must, 20
Should, 3 Could) in 14 areas; sample company Northwind Components, Inc. with a CSV dataset
and generated expected results (January 2026 net income 5,127.10; total assets 617,415.00);
15 acceptance scenarios covering every Must requirement; comparison protocol with 14
metrics and 6 standard change requests; work-item and defect templates in `templates/`;
own generator and checker in `tools/` (`sh tools/regen.sh`). Copy guide for other
repositories: `docs/reports/finance-requirements-copy-guide-zh.md`.

**Reports to the owner (Chinese):** `docs/reports/ubos-system-spec-summary-zh.md`,
`docs/reports/finance-requirements-summary-zh.md`.

**Tools:** `docs/tools/` holds `check_system.py`, `gen_phase_features.py`,
`gen_traceability.py`, `gen_indexes.py`; `docs/finance-requirements/tools/` holds
`gen_finance_expected.py`, `check_finance.py` and `regen.sh`; `docs/tools/regen_all.sh` runs
all. Expected result today: system volume `errors: 0, warnings: 0`;
finance `errors: 0`.

## 4. Next steps

Done on 2026-09-29: finance requirements frozen at 1.0 (DEC-029); monorepo decided (DEC-028);
PH-0 plan confirmed by the owner and step 0 done (DEC-030). The step plan and status are in
`docs/reports/ph0-plan-zh.md`.

Step 1 done: monorepo skeleton (`standard/`, `verify/`, `crates/` with five skeleton
crates), licence files and `LICENSING.md`, CI (`.github/workflows/ci.yml`), dependency-graph
checker `docs/tools/check_deps.py` reading UBS-ARC-07 §2.

Step 2 done: TLA+ models in `verify/models/` checked per DEC-031, no violation; report
`verify/models/reports/README.md`; finding F-1 fixed in DSN-DVM-208; OQ-004 and OQ-005 open
(owner to decide).

Step 3a done (DEC-032): release build in `standard/` (oracle, generator, builder), 390
vectors for FND, OBJ, CLS and KIND, STD-CONF-005 adapter operations; OQ-006 open.

Step 3b done: 551 vectors, MUST coverage 94% (68/72), manifest in
`standard/release/0.1/manifest.json`.

PH-0 status (details in UBS-PHR-0, `docs/50-system/50-phases/PH-0/02-findings.md`):
EXIT-0-01…04 and EXIT-0-06 met (steps 3, 4 and 6); step 5 prepared (5a–5c: tree
maintenance, storage layout, generator, benchmark driver, group commit), but EXIT-0-05 is
carried over into PH-1 because hosts cannot be rented now (DEC-035); step 7 done: PH-1
re-baselined (DEC-036, EXIT-0-07) with work items W-1…W-12, OQ-007 and OQ-008 resolved.

PH-0 accepted (DEC-037). PH-1 started (DEC-038); gate-1a steps and status in
`docs/reports/ph1-plan-zh.md` and UBS-PH-1 §11.

1. **Next AI step:** PH-1 step 1 in six parts (DEC-039): 1a done (rc1 scope rule, tooling,
   STD-OBJ-009); 1b done (object and class model, DEC-040); next 1c versioning, time and
   transactions, 1d proofs, 1e execution, 1f release (owner signs `bpa-v1.0.0-rc1`).
2. **Owner, when possible:** rent ENV-REF-SERVER and run `docs/reports/ph0-step5-runbook-zh.md`
   (W-1, before the storage layout of step 3 is fixed); name the external reviewer for that
   occasion.
3. **Owner, before step 8:** confirm the GitHub Actions minutes for nightly jobs and ARM64
   and macOS runners.
4. **Java side** (owner): copy `fin-req-v1.0` into the Java repository
   (`docs/reports/finance-requirements-copy-guide-zh.md`) and start work-item records.
5. **Accounting review** (external): chart of accounts and expected results of the sample
   company (EXIT-2-16); findings become finance version 1.1.

## 5. Open items

| Item | Note |
|---|---|
| Legal review of licences | DEC-019 accepted; legal review before any public release. |
| Accounting reviewer | to be named (ASM-002, PH-3 CPA). |
| PH-0 external reviewer | database and formal-methods expert (DEC-030), to be named by the owner for the review at the carried benchmark (DEC-037). |
| ENV-REF-SERVER | rented by the owner for the carried benchmark (DEC-035), CPU with SHA extensions. |

## 6. Repositories

- `staunch0515/ubos-unit` — **current home** of the specification and the finance
  requirements (this content, with its git history), and the implementation monorepo
  (DEC-028). Continue work here.
- The owner's Java finance system lives in a separate, unrelated repository; it only holds
  an unchanged copy of a tagged finance-requirements version (DEC-029).
- `staunch0515/ubos-platform` — original repository; its `main` branch contains the same
  documents up to the migration, plus the older Java code of the research phase. Kept as an
  archive.
- The seven other research repositories (`ubos`, `ubos-system`, `ubos_core`, `ubos_web`,
  `logicorum`, `logrum-system`, `fund-ubos`) are inputs only (`docs/50-system/00-meta/06-inputs.md`).

## 7. How to resume

1. Read `CLAUDE.md`, this file, `docs/50-system/README.md` and
   `docs/50-system/00-meta/05-ai-reading-guide.md`.
2. Run `bash docs/tools/regen_all.sh` and confirm 0 errors.
3. Report to the owner in Chinese: your understanding of the state, and a recommended next
   step from §4. Wait for confirmation before changing documents.
