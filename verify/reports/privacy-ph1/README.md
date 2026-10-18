# Privacy and classification in the PH-1 scope (PH-1 step 19b)

| Item | Value |
|---|---|
| Requirements | NR-PRIV-003 (classification coverage), NR-PRIV-006 (privacy by default, PH-1 part), FR-MODEL-047 (classification on fields, PH-1 part), DSN-DVM-824, DSN-DVM-836 and CR-GDPR-003 (rectification, PH-1 part), STD-FND-027, STD-CLS-013 |
| Result | **Coverage inspection: 34 fields of the scenario project and 12 of the SDK fixture, each with an effective classification; privacy tests PRIV-1…PRIV-4: 4 of 4 in memory and on PostgreSQL, PRIV-1…PRIV-3 over UBTP (WebSocket and HTTP), PRIV-4 in process only** |
| Decision | DEC-086 (scope), DEC-085 (part 19b) |
| Runners | `forge check <project> --json`; `forge test verify/scenarios` (`--backend postgres`, `--backend ubtp --binding ws|http`) |
| Environment | development workstation (4 vCPU, linux x86_64), PostgreSQL 16, debug build, 2026-10-18; CI runs the coverage check and the tests in the jobs `rust workspace` (memory, PostgreSQL) and `scenarios over UBTP` |
| Records | `classification-scenarios.json`, `classification-fixture.json`, `tests-memory.json`, `tests-postgres.json`, `tests-ubtp-ws.json`, `tests-ubtp-http.json` |

## 1. Scope (DEC-086)

In PH-1 a field's effective classification is its declaration, else its class's
`classification_default`, else `internal`. A refinement may raise a classification and never
lower it. A rectification is a new version on the same valid period, committed with the reason
`rectification`. Moved to PH-2: masking personal fields for agents and exports by default
(DSN-DVM-821), read logging of personal data by default (DSN-DVM-823), and encrypting a
rectified field's earlier versions with the subject key (DSN-DVM-830). The ontology `Person`
name and email become `personal` with the PH-2 genesis upgrade.

## 2. Classification coverage (NR-PRIV-003, inspection)

`forge check` reports, for the fields a project declares, the effective level and its source.
Every field has an effective level by the rule above, so coverage is 100% by construction. The
counts show how much of it is declared.

| Project | Fields | Declared | Class default | Default `internal` | public / internal / confidential / personal / sensitive_personal | Personal fields |
|---|---|---|---|---|---|---|
| scenario project (`verify/scenarios`) | 34 | 4 | 1 | 29 | 0 / 30 / 1 / 2 / 1 | `Contact.name`, `Contact.email`, `VipContact.email` (sensitive) |
| SDK fixture (`verify/sdk/fixture`) | 12 | 1 | 0 | 11 | 0 / 11 / 0 / 1 / 0 | `Document.passport` |

Most fields in both projects fall back to `internal`. These are test projects whose data are
not personal. The first-party Buks of PH-2 (Basic Finance) are inspected against NR-PRIV-003
when they exist.

## 3. Privacy tests (`verify/scenarios/tests/privacy.yaml`)

| Test | Checks | Memory | PostgreSQL | UBTP ws | UBTP http |
|---|---|---|---|---|---|
| PRIV-1 | the effective class of `Contact` carries the declared levels (`name`, `email` personal; `segment` internal); `VipContact` inherits `name` and raises `email` to `sensitive_personal` | pass | pass | pass | pass |
| PRIV-2 | a subclass lowering `email` from `personal` to `internal` is refused with `MODEL.NARROWING_VIOLATION` | pass | pass | pass | pass |
| PRIV-3 | a rectification of `email` from 2026-01-01, nine days after creation, with the reason `rectification`: the current read and a read as of 2026-02-15 give the rectified value; the same read as of the earlier transaction time gives the earlier value; the history has both commits | pass | pass | pass | pass |
| PRIV-4 | the audit trail (FR-AUD-011) of the object names the rectification (`why.reason = rectification`, the message) and keeps both values (`/email` old and new) on the same valid period | pass | pass | in process | in process |

PRIV-4 uses the kernel's `audit` query, which is not a Protocol request (STD-PROTO-015). The
UBTP runner reports it as verified in process only, like SCN-011 A1.

The model export of `forge types` (`verify/scenarios/generated/model.json`) carries the declared
classification of every field (FR-MODEL-047 acceptance, the export part).

## 4. Observation

On the workstation, the UBTP runs first failed on a database reused across many earlier runs.
A test Server ran out of PostgreSQL connections (`max_connections` 100): every Server opens, at
start, the VAEs of all tenants with pending work, and that database held a tenant for every
earlier test with timers. On a fresh database, as in CI, all runs pass. This is the
connection-budget point noted at step 15 (a node's connections grow with its open VAEs).
