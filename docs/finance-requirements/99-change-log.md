---
id: FIN-DOC-99
title: Change Log
status: frozen
---

# Change Log

## 1. Freeze rule

1. A released version of this folder is **frozen**: its content does not change. It is
   identified by a version number `MAJOR.MINOR` and the git tag `fin-req-vMAJOR.MINOR` in the
   source repository.
2. Every change is made in the source repository only. It is listed below with the
   requirement, scenario or dataset IDs it touches, the folder is regenerated and checked
   (`sh tools/regen.sh`, `errors: 0`), and the result is released as a new version with a
   new tag.
3. A change that alters an expected result, a Must requirement or a scenario raises MINOR at
   least; editorial corrections that change no requirement, figure or acceptance criterion
   may be collected into the next MINOR.
4. Both implementations move to a new version at an agreed date. Work after that date is
   recorded with the new `req_version` (`40-comparison-protocol.md` §2).
5. The standard change requests CR-A … CR-F (`40-comparison-protocol.md` §4) are not
   changes of this folder until they are run; each is released as a new version when its
   acceptance step is added to the scenarios.

## 2. Versions

| Version | Date | Tag | Content |
|---|---|---|---|
| 1.0 | 2026-09-29 | `fin-req-v1.0` | first release: 153 requirements (130 Must, 20 Should, 3 Could) in 14 areas; sample company Northwind Components, Inc. with dataset and expected results; FIN-SCN-01 … FIN-SCN-15; comparison protocol with FIN-MET-01 … FIN-MET-14, work-item and defect templates; values confirmed by the owner: Form 1099-NEC/MISC threshold for tax year 2026 = 2,000.00, Texas / City of Austin combined sales tax rate = 8.25% |
