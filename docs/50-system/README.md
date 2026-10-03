---
id: UBS-README
title: UBOS Business Processing Architecture — System Specification
status: draft
phase: ALL
depends_on: []
---

# UBOS Business Processing Architecture — System Specification

This is the **authoritative, self-contained specification** of the UBOS Business
Processing Architecture (BPA) and the systems built on it. It supersedes `docs/40-spec/`
as the current authority. The earlier volume is kept as history (DEC-008).

**The idea in one sentence:** UBOS standardises how business state changes over time.
State is typed, inheritable, versioned, bitemporal, governed and provable. Business policy
stays soft and versioned. Every contract or order runs as a standard **Business
Processing Unit (BPU)** on a **Data Virtual Machine (DVM)**.

## Start here

| You want to … | Open |
|---|---|
| know what this volume is and is not | [`00-meta/00-charter.md`](00-meta/00-charter.md) |
| work as an AI agent or new engineer on this volume | [`00-meta/05-ai-reading-guide.md`](00-meta/05-ai-reading-guide.md) |
| understand the sources and resolved conflicts | [`00-meta/06-inputs.md`](00-meta/06-inputs.md) |
| know how IDs and requirement blocks work | [`00-meta/01-writing-rules.md`](00-meta/01-writing-rules.md) |
| see the six phases and how each is verified | [`50-phases/00-index.md`](50-phases/00-index.md) |
| see how verification works | [`60-verification/00-index.md`](60-verification/00-index.md) |
| find any ID | [`INDEX/00-index.md`](INDEX/00-index.md) |
| see decisions | [`90-decisions/000-index.md`](90-decisions/000-index.md) |
| see progress and history | [`00-meta/03-progress.md`](00-meta/03-progress.md) |

## Folder map

| Folder | Content | Status |
|---|---|---|
| `00-meta/` | charter, rules, templates, progress, inputs, AI guide | complete |
| `10-requirements/` | vision, 33 personas, 220 capabilities, 67 scenarios (19 deferred, DEC-022), 800 FR, 106 NR, 54 CR, constraints, assumptions, 146 glossary terms | complete (confirmed 2026-09-29) |
| `20-architecture/` | 13-system family, 36 contracts, topologies, data architecture, runtime flows, cross-cutting rules (34 AR), codebase, capability allocation | complete |
| `30-standard/` | the BPA standard: 239 normative clauses, 48 data schemas, instruction set ABI 1.0, UBTP, sync, errors, conformance | complete (normative) |
| `40-systems/` | 13 system chapters: 695 design items (DSN), 118 interfaces, data, state machines, configuration, operations, verification | complete |
| `50-phases/` | PH-0 … PH-5: goals, scope, generated feature lists, verification plans, 65 metrics, 85 exit criteria (7 withdrawn), 38 risks (5 withdrawn) | complete |
| `60-verification/` | principles, VER numbering, 12 methods, environments, datasets, 77 suites, 90 verification items, generated traceability | complete |
| `90-decisions/` | DEC-001 … DEC-060 | active |
| `INDEX/` | generated catalogues of documents and IDs, statistics | generated |

## The six phases at a glance

| Phase | Purpose | Proves |
|---|---|---|
| PH-0 | de-risk storage, core semantics and the testable standard | assumptions ASM-003, ASM-008 |
| PH-1 | reference kernel and single-node server (gates 1a engine core, 1b governance and services) | G1 mechanism |
| PH-2 | first business package (US Basic Finance) on an HA Server with a professional Web Workspace and Smart Grid | G2 applications without kernel changes |
| PH-3 | year of demonstration operation, external CPA audit drill, governed finance agents | G3 audit-grade evidence, G4 governed AI |
| PH-4 (not scheduled) | SQLite, Box, desktop and mobile with sync; multi-tenant Cells, Control Plane, Exchange, certification, workers | G5 platform |
| PH-5 (not scheduled) | federation, shared BPUs, browser kernel; contract-only capabilities | G6 federation |

## Tools

| Command | Purpose |
|---|---|
| `docs/tools/regen_all.sh` | regenerate feature lists, traceability and indexes, then validate |
| `python3 docs/tools/check_system.py` | validate only (must report 0 errors before every commit) |
| `python3 docs/tools/gen_phase_features.py` | regenerate `50-phases/PH-*/01-features.md` |
| `python3 docs/tools/gen_traceability.py` | regenerate `60-verification/2*-trace*.md` |
| `python3 docs/tools/gen_indexes.py` | regenerate `INDEX/` |
