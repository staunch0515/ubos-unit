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

1. **PH-0 step 1** — monorepo skeleton: `standard/`, `verify/`, `crates/` (Cargo workspace
   with `ubos-types`, `ubos-store`, `ubos-tree`, `ubos-version`, `ubos-time`), licence files
   per DEC-019, CI (GitHub Actions: `regen_all.sh`, `cargo fmt`, `clippy`, dependency-graph
   check of AR-031).
2. Then steps 2…7 of DEC-030 in order, one pull request each, merged by the owner.
3. **Java side** (owner): copy `fin-req-v1.0` into the Java repository
   (`docs/reports/finance-requirements-copy-guide-zh.md`) and start work-item records.
4. **Accounting review** (external): chart of accounts and expected results of the sample
   company (EXIT-2-16); findings become finance version 1.1.

## 5. Open items

| Item | Note |
|---|---|
| Legal review of licences | DEC-019 accepted; legal review before any public release. |
| Accounting reviewer | to be named (ASM-002, PH-3 CPA). |
| PH-0 external reviewer | database and formal-methods expert for PH-0 acceptance (DEC-030), to be named by the owner. |
| ENV-REF-SERVER | rented by the owner for PH-0 step 5 (DEC-030). |

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
