---
id: FIN-DOC-40
title: Comparison Protocol
status: frozen
---

# Comparison Protocol

The same requirements are implemented by two independent implementations, each developed in
its own repository by its own team. The owner gives each implementation a one-letter
**system code** (for example `A` and `B`) that is used in all records. This protocol fixes
what is measured, how, and how the results are reported, so that the comparison is fair and
repeatable.

## 1. Principles

1. **Same input.** Both teams work from this folder at the same version (`README.md`,
   §Version). Each team copies the folder unchanged into its repository. A change to the
   requirements gets a new version (`99-change-log.md`) and is copied to both.
2. **Same oracle.** Both implementations are accepted only by FIN-SCN-01 … FIN-SCN-15 and
   the expected results FIN-EXP-*, run by a person who did not build the implementation.
3. **Record while working.** Effort and defects are recorded when they happen, not
   estimated afterwards.
4. **Scope differences are explicit.** Every requirement not met, or met differently, is
   logged per requirement with a reason.
5. **No tuning to the test.** Expected results are not typed into either system (§3 of
   `20-sample-company.md`).

## 2. Work items

Work is recorded per **work item** = one requirement (FIN-*), one acceptance scenario
(FIN-SCN-*), one change request (CR-A … CR-F) or one named cross-cutting task (for example
"statement import formats"; its ID is `TASK-` plus a short name chosen by the team).

### 2.1 Where and when

1. Each team keeps two files in its own repository, in a folder of its choice, started from
   `templates/work-items.csv` and `templates/defects.csv`. The files are committed at least
   once a week.
2. Recording starts with the first hour of work after the team copied the tagged version;
   the first row names that version in `req_version`.
3. A person adds one **effort row** per work item per day on which they worked on it, on the
   same day. Hours are recorded in steps of 0.25. Work that serves several work items is
   split in proportion, or recorded under a `TASK-` item.
4. Every commit message of the implementation starts with the work item ID
   (`FIN-GL-003: …`, `TASK-bank-import: …`). Size (§2.3) is derived from the commit history
   by these IDs; it is not typed by hand.
5. A defect is recorded when it is found, in `defects.csv`, with the work item it traces to.
   Its fix is recorded as effort rows of kind `defect-fix`.
6. Rows are never deleted. A wrong row is corrected by a new row with negative hours and a
   note.

### 2.2 Effort row (`templates/work-items.csv`)

| Field | Meaning |
|---|---|
| system | the system code |
| req_version | version of this folder the work is based on (for example `1.0`) |
| date | day of the work, `YYYY-MM-DD` |
| person | a pseudonym of the person, stable for the whole comparison |
| role | developer, analyst, tester or designer |
| work_item | requirement, scenario, change request or task ID |
| kind | build, change, defect-fix or rework |
| hours | person-hours, in steps of 0.25 |
| artefact_kind | where the work landed: code, configuration-model, test, documentation or none |
| notes | anything that affects comparability |

### 2.3 Size, from the commit history

For each work item, size is computed at report time from the commits whose message starts
with its ID: files changed; lines added, changed and deleted by language; and, for
configuration or model files, the number of definitions changed. Each team states in the
report which file types it counts as code and which as configuration or models, and how it
counts a definition.

### 2.4 Defect row (`templates/defects.csv`)

| Field | Meaning |
|---|---|
| system | the system code |
| defect_id | team-local ID |
| date_found | `YYYY-MM-DD` |
| work_item | the work item the defect traces to |
| severity | critical, major, minor or cosmetic |
| found_in | review, test, acceptance or operation |
| fin_exp | FIN-EXP section whose figure was wrong, if any |
| date_fixed | `YYYY-MM-DD`, empty while open |
| notes | description |

## 3. Metrics

| Metric | Definition | Unit |
|---|---|---|
| FIN-MET-01 | Total effort to pass all Must requirements (sum of work items of kind build and rework) | person-hours |
| FIN-MET-02 | Effort per area (GL, PC, AR, AP, BK, FA, FX, TX, RP, CT, SC, DI, NF, UI) | person-hours |
| FIN-MET-03 | Calendar time from start to first full acceptance | weeks |
| FIN-MET-04 | Size of the implementation: code by language, and configuration or model definitions | lines and definitions |
| FIN-MET-05 | Share of requirements met without writing general-purpose code (effort rows of the requirement have `artefact_kind` configuration-model, test or documentation only) | % |
| FIN-MET-06 | Defects found in acceptance and in the first 3 months of operation per 100 Must requirements, by severity | count |
| FIN-MET-07 | Expected-result mismatches at first acceptance run | count of FIN-EXP lines that differ |
| FIN-MET-08 | Change lead time for the standard change requests of §4 (from request to accepted in production) | hours |
| FIN-MET-09 | Effort for the standard change requests of §4 | person-hours |
| FIN-MET-10 | Performance at FIN-NF-001 volumes: the FIN-NF-002 percentiles | ms |
| FIN-MET-11 | Usability: task success and SUS in the same study design (FIN-UI-001) | %, score |
| FIN-MET-12 | Audit support: time to answer the audit requests of FIN-SCN-12 and whether the evidence was verifiable without system access | hours, yes/no |
| FIN-MET-13 | Requirements met: Must, Should and Could met per system | count |
| FIN-MET-14 | Operational effort: installation, upgrade and restore drill time (FIN-SCN-14) | hours |

## 4. Standard change requests

After first acceptance, both teams implement the same change requests, one at a time, with
effort and lead time recorded (FIN-MET-08, FIN-MET-09):

| Change | Content |
|---|---|
| CR-A | Lower the manual-journal approval threshold to 5,000.00 from next month and add a second approver above 50,000.00 |
| CR-B | Add a new state (for example Colorado with state and local rates) with an exemption certificate type |
| CR-C | Add a dimension "department" to journals, invoices and bills and an income statement by department |
| CR-D | Change FA-002's remaining useful life (change in estimate) and add the 150% declining-balance method |
| CR-E | Add a customer early-payment discount 2/10 net 30 with a sales-discount account |
| CR-F | Correct a posted rate for 2026-01-31 and show the effect on January's revaluation without changing issued reports |

Each change is accepted by an acceptance step written with the change and added to the
scenarios before either team starts.

## 5. Procedure

1. Freeze and tag the requirements version; both teams copy it unchanged (`README.md`).
2. Each team keeps its work-item and defect records per §2.
3. When a team declares a scenario ready, an independent tester runs it and records the
   result and any FIN-EXP mismatch.
4. After first full acceptance, run the change requests of §4.
5. Run FIN-SCN-14 (performance, operations, usability, accessibility) on comparable
   hardware, stated in the report.
6. Collect three months of operation data where the systems are used for demonstrations.

## 6. Report

The comparison report contains, per metric, both values, the ratio, and the notes that
limit comparability (team size and experience, reuse of existing components, scope
differences). It separates **first-build** effort from **change** effort, because the
platform is expected to show its value mainly in changes, audit support and correctness.
The report is written for the owner; neither team edits the other's data.
