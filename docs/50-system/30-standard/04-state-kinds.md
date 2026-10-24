---
id: UBS-STD-04
title: BPA Standard — State Kinds (KIND)
status: draft
phase: PH-0
depends_on: [UBS-STD-03]
---

# BPA Standard — State Kinds (KIND)

| Kind | Versioned per branch | Merge | Valid time | Written by | Correction |
|---|---|---|---|---|---|
| Definition | yes | three-way (STD-VER-050) | period per version | commit instructions | new version |
| Ledger `[Profile: Ledger]` | append-only per ledger key | append (STD-VER-054) | instant or date per entry | `commit.append`, `commit.reverse`, `commit.adjust` | reversal or adjustment |
| System | yes (kernel services only) | not merged; branch-local | none | kernel services and dedicated instructions | kernel state transitions |
| RawDocument | yes | whole-value (conflict if both sides changed) | period | commit instructions | new version |

## 1. Definition kind

### STD-KIND-001 — Definition semantics
- **Clause:** Objects of Definition kind MUST follow STD-OBJ-002. Every version carries a valid-time period, and versions participate in diff, three-way merge and revert. Updates MUST name a base version (STD-TXN-020).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1000…1009
- **Satisfies:** FR-MODEL-052
- **Notes:** —

## 2. Ledger kind `[Profile: Ledger]`

### STD-KIND-020 — Ledger entry schema
- **Clause:** Each ledger entry MUST conform to `DAT-LedgerEntry`. Its entry hash is computed over the canonical entry with domain tag `ubos.entry.v1`, excluding `seq` and `source_seq`, which are assigned by the branch. Entries are immutable after commit.
- **Grammar / Schema:** `DAT-LedgerEntry`
- **Conformance vectors:** VER-CONF-1020…1039
- **Satisfies:** FR-MODEL-053, FR-LEDG-011, FR-LEDG-012
- **Notes:** Excluding the sequence from the hash lets the same entry be appended on another branch after a merge without changing its identity.

```yaml
DAT-LedgerEntry:
  entry_id: { type: text, required: true, description: "UUID of the entry" }
  class: { type: text, required: true, description: "ledger class URI" }
  class_version: { type: text, required: true, description: "content-id of the class version" }
  ledger_key: { type: map, required: true, description: "values of the declared ledger key fields" }
  seq: { type: integer, required: true, description: "branch-assigned sequence within the ledger key (1-based, gap-free)" }
  source_seq: { type: composite, required: false, description: "{branch, seq} on the branch where first appended, after merges" }
  valid_at: { type: text, required: true, description: "date or instant of business effect" }
  body_ref: { type: text, required: true, description: "content-id of the body (amounts and other fields)" }
  corrects: { type: list, required: false, description: "entry_ids corrected by this entry" }
  provenance_ref: { type: text, required: false, description: "content-id of the derivation record for logic-produced entries (STD-TXN-030, DEC-041)" }
  correction_type: { type: enum, required: false, description: "reversal | adjustment | repost" }
  posting_group: { type: text, required: false, description: "group id for double-entry balancing" }
  source_refs: { type: list, required: false, description: "canonical URIs of originating objects" }
```

### STD-KIND-021 — Append-only enforcement
- **Clause:** The only instructions that create ledger entries MUST be `commit.append`, `commit.reverse` and `commit.adjust`. Any update or end instruction targeting an entry MUST fail with `LEDG.IMMUTABLE_ENTRY`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1040…1049
- **Satisfies:** FR-LEDG-012, FR-MODEL-053
- **Notes:** —

### STD-KIND-022 — Gap-free sequences
- **Clause:** On each branch, entries of one `(class, ledger_key)` MUST be numbered 1, 2, 3, … in commit order, without gaps or duplicates, including under concurrent appends and crashes. A sequence number becomes visible only with the commit that assigns it.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1050…1069
- **Satisfies:** FR-LEDG-041, FR-LEDG-042
- **Notes:** Implementations typically allocate sequences inside the flush transaction from a locked counter (DEC-020).

### STD-KIND-023 — Reversal
- **Clause:** `commit.reverse(entry_id, reason)` MUST append an entry with every amount field negated, `corrects = [entry_id]`, `correction_type = reversal`, and `valid_at` equal to the original's unless specified. An entry that is already reversed MUST NOT be reversed again (`LEDG.ALREADY_REVERSED`). A reversal entry MUST NOT be reversed (`LEDG.REVERSAL_NOT_REVERSIBLE`).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1070…1084
- **Satisfies:** FR-LEDG-021, FR-LEDG-023
- **Notes:** —

### STD-KIND-024 — Double entry
- **Clause:** For ledger classes with `double_entry: true`, all entries sharing a `posting_group` within one commit MUST balance (the sum of signed amounts is zero per currency). Otherwise the commit MUST fail with `LEDG.UNBALANCED`.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1085…1094
- **Satisfies:** FR-LEDG-051
- **Notes:** —

### STD-KIND-025 — Periods
- **Clause:** A ledger class MAY declare a `period_scope`. Period objects with states `open`, `closing` and `closed` then govern the entries whose posting date falls in them. Appends with a posting date in a `closed` period MUST fail with `LEDG.PERIOD_CLOSED`. The posting date is the date of the commit's transaction time in the VAE business time zone, unless the class declares a posting-date field.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1095…1109
- **Satisfies:** FR-LEDG-061
- **Notes:** Since 1.0-rc8 (DEC-090):
  - **Period class.** A period class has the fields `start` and `end` (dates; the period is the half-open interval `[start, end)`) and `state`.
  - **Transitions.** A period object may be created in any state. An update moves its `state` only between `open` and `closing`, otherwise `MODEL.CONSTRAINT_VIOLATED`. `closing` is a soft close: appends still commit, and governance sheets may restrict who appends.
  - **Closing and reopening.** A period enters `closed` only through `close_period` (STD-KIND-026). It leaves `closed` only through `reopen_period`, which states a reason (`MODEL.DECLARATION_INCOMPLETE` without one) recorded with the commit's process, and returns the period to `open`. Its close artefacts stay.
  - **Corrections.** A correction of an entry of a closed period is appended with a posting date in an open period; the correcting entry keeps the `valid_at` of the original (STD-KIND-023).

### STD-KIND-026 — Close artefacts
- **Clause:** Closing a period in `closing` MUST be one commit that:
  - sets the period's state to `closed`;
  - creates a close artefact, an object of the System class `CloseArtefact`.

  The commit's process records the close, and the implementation MUST then create a signed tag `close-<period slug>-<n>` on the closing commit, where `n` counts the closes of the period from 1. The artefact MUST hold:
  - the period (`period`, `period_slug`, `start`, `end`);
  - the sequence `n` and the tag;
  - `as_of` = `{cseq, commit_id, root}` of the commit the close extends;
  - `balances`: for every ledger key of the classes the period scopes, the number of entries and the sum of each amount field over the entries whose posting date is before `end`, as of that commit.

  Close artefacts MUST NOT be created or changed by commits (`LOGIC.FORBIDDEN_TARGET`, STD-KIND-040). A report "as closed" reads the artefact; later corrections do not change it.
- **Grammar / Schema:** `DAT-CloseArtefact`
- **Conformance vectors:** VER-CONF-1110…1119
- **Satisfies:** FR-LEDG-062, DSN-DVM-443
- **Notes:**
  - Balances are ordered by class and canonical ledger key. A decimal amount sums to a decimal. A money amount sums to a list of money values, one per currency in currency order, each at the minor units of its currency.
  - Closing a period not in `closing` fails with `MODEL.CONSTRAINT_VIOLATED`; naming an object of a class that no ledger class scopes fails with `MODEL.INVALID_VALUE`.
  - The seal of STD-PROOF-055 is added when that clause is released (PH-5); until then the signed tag binds the closing commit. Since 1.0-rc8 (DEC-090).

```yaml
DAT-CloseArtefact:
  period: { type: text, required: true, description: "object id of the period" }
  period_slug: { type: text, required: true, description: "slug of the period" }
  sequence: { type: integer, required: true, description: "1 for the first close of the period, then 2, 3 …" }
  start: { type: date, required: true, description: "start of the period" }
  end: { type: date, required: true, description: "end of the period (exclusive)" }
  tag: { type: text, required: true, description: "close-<period slug>-<sequence>" }
  as_of: { type: map, required: true, description: "{cseq, commit_id, root} of the commit the close extends" }
  balances: { type: list, required: true, description: "{class, ledger_key, entries, amounts} per ledger key" }
```

### STD-KIND-027 — Matching
- **Clause:** Matching pairs the entries of two ledger sides (`left` and `right`, each a ledger class and optionally one ledger key) under a list of rules evaluated in priority order (`DAT-MatchingRule`). A rule names:
  - its `kind` (`one_to_one`, `one_to_many` or `many_to_one`);
  - `equal`, the field pairs whose values MUST be equal;
  - `amount`, the amount field of each side;
  - `tolerance`, the largest absolute difference of the amounts per currency (default 0);
  - `window_days`, the largest distance of the `valid_at` dates (default 0);
  - `negate`, which compares the left amount with the negated right amount.

  The candidates are the entries of each side that no active match record of the match set references (STD-KIND-028), ordered by `valid_at` and then `entry_id`. The rules MUST be applied as follows:
  - **One-to-one.** For each left entry in order, the first right entry that satisfies the rule is matched.
  - **One-to-many.** For each left entry in order, all unmatched right entries that satisfy `equal` and `window_days` form the group, and it is matched when the sum is within the tolerance.
  - **Many-to-one.** As one-to-many, with the sides exchanged.

  Amounts in different currencies never match. The result MUST list the matches (`rule`, `rule_ref`, `left`, `right`) in the order made and the breaks (the unmatched entries of each side, in candidate order). The same inputs MUST give the same result.
- **Grammar / Schema:** `DAT-MatchingRule`
- **Conformance vectors:** VER-CONF-1165…1184
- **Satisfies:** FR-LEDG-071, DSN-DVM-445
- **Notes:**
  - `rule_ref` is the content ID (`ubos.chunk.v1`) of the rule as given, the rule version of FR-LEDG-073.
  - A group is formed only from entries whose `equal` fields are identical. Matching does not search for a subset of entries that sums to the amount, so its cost grows with the number of entries rather than exponentially.
  - A rule without an `id` or a `kind`, or without `amount`, fails with `MODEL.DECLARATION_INCOMPLETE`; duplicate rule ids, a negative tolerance or window fail with `MODEL.INVALID_VALUE`.
  - Since 1.0-rc8 (DEC-090).

```yaml
DAT-MatchingRule:
  id: { type: text, required: true, description: "rule name, unique in the list" }
  kind: { type: enum, required: true, description: "one_to_one | one_to_many | many_to_one" }
  equal: { type: list, required: false, description: "[left field, right field] pairs that must be equal" }
  amount: { type: list, required: true, description: "[left amount field, right amount field]" }
  tolerance: { type: decimal, required: false, description: "largest absolute difference per currency; default 0" }
  window_days: { type: integer, required: false, description: "largest distance of the valid_at dates in days; default 0" }
  negate: { type: boolean, required: false, description: "compare the left amount with the negated right amount" }
```

### STD-KIND-028 — Match records
- **Clause:** A committed matching run MUST append one entry of the genesis ledger class `MatchRecord` per match. Its ledger key is `{match_set}`, and its body holds:
  - `left` and `right`, the matched entry ids;
  - `rule` and `rule_ref`;
  - `actor`, the initiator of the process.

  Its `entry_id` is derived from the record's content. An unmatch MUST append a new record with the same `left`, `right` and rule, `unmatches` naming the record it undoes and an optional `reason`. The undone record stays, and its entries become candidates again. A record is active when it is not an unmatch and no unmatch names it.
  - Undoing a record that is not active fails with `MODEL.CONSTRAINT_VIOLATED`.
  - An unknown record fails with `IAM.NOT_FOUND`.
  - Match records MUST NOT be appended by commits (`LOGIC.FORBIDDEN_TARGET`).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1185…1199
- **Satisfies:** FR-LEDG-073, DSN-DVM-445
- **Notes:**
  - The derived `entry_id` is the UUID (version 8, RFC 9562) whose hexadecimal digits are the first 32 digits of the SHA-256 of the canonical JSON `["ubos.match.v1", body]`, with the version and variant bits set. The same match in the same set therefore has the same identifier.
  - Since 1.0-rc8 (DEC-090).

### STD-KIND-029 — Chart of accounts
- **Clause:** Accounts MUST be objects of the genesis Definition class `LedgerAccount` (`DAT-LedgerAccount`), with a `type` (`asset`, `liability`, `equity`, `income` or `expense`), an optional `currency` and an optional `parent` account. An account is a leaf at a date when no account live at that date names it as `parent`. A ledger class MAY name a reference field of its entries as its `account` (`DAT-ClassDecl` member `ledger.account`). An append, reversal or adjustment of an entry of such a class MUST fail with `MODEL.CONSTRAINT_VIOLATED` unless the referenced account, at the entry's `valid_at`:
  - is live (its valid time covers the date);
  - is a leaf;
  - has no `currency`, or every money amount of the entry is in that currency.

  A commit that makes an account its own ancestor MUST fail with `MODEL.CONSTRAINT_VIOLATED`. The balance of an account that is not a leaf is the sum over the leaves below it (`rollup`, STD-ISA-037).
- **Grammar / Schema:** `DAT-LedgerAccount`
- **Conformance vectors:** VER-CONF-1130…1139
- **Satisfies:** FR-LEDG-052, DSN-DVM-441
- **Notes:** Since 1.0-rc9 (DEC-098):
  - **Details.** The error details name the account and the reason: `inactive`, `not_leaf`, `currency` or `cycle`.
  - **Changes.** An account is closed by ending its valid time (STD-TIME-011); its entries stay. Entries of an earlier valid time keep the account they reference.
  - **Hierarchy changes.** An account that already has entries may receive children: earlier entries stay valid, and later appends must use a leaf.
  - **Genesis class.** `LedgerAccount` is the kernel base class; packages extend it (STD-CLS-030) or subclass it.

```yaml
DAT-LedgerAccount:
  number: { type: text, required: true, description: "account number" }
  name: { type: text, required: true, description: "account name" }
  type: { type: enum, required: true, description: "asset | liability | equity | income | expense" }
  currency: { type: text, required: false, description: "the only currency of the account's money amounts" }
  parent: { type: reference, required: false, description: "the parent account" }
```

### STD-KIND-030 — Statement lines
- **Clause:** A ledger class MAY declare `ledger.statement` with the body fields `source`, `account`, `statement` and `line`, which identify a line of an external statement. The `entry_id` of an entry of such a class MUST be the UUID derived from those four values, otherwise the append MUST fail with `MODEL.INVALID_VALUE`; a second append of the same line fails with `TXN.ALREADY_EXISTS`. The adapter operation `ingest_statement` MUST ingest a list of lines idempotently:
  - a line whose entry exists with the same body is a duplicate and is not appended again;
  - a line whose entry exists with another body fails the whole ingestion with `TXN.IDEMPOTENCY_MISMATCH`;
  - the other lines are appended in one commit, in the order given.

  A repeated ingestion of the same statement therefore creates no entries.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1010…1019
- **Satisfies:** FR-LEDG-074, DSN-DVM-446
- **Notes:** Since 1.0-rc9 (DEC-098):
  - **Derived identifier.** It is the UUID (version 8) whose hexadecimal digits are the first 32 digits of the SHA-256 of the canonical JSON `["ubos.statement.v1", [source, account, statement, line]]`, with the version and variant bits set, as for match records (STD-KIND-028).
  - **Content.** The body is compared by its content ID (`ubos.chunk.v1`); within one ingestion, a repeated line with the same body is a duplicate too.
  - **Result.** `ingest_statement {branch, class, lines, process}` returns `{appended, duplicates}`, the entry ids in input order, and `cseq` and `commit_id` when it appended a line. A line is `{ledger_key, valid_at, body}`.
  - **File formats.** The parsers of bank formats (BAI2, ISO 20022 camt, MT940) produce these lines; they are not part of the standard (DSN-NOD-305).

## 3. System kind

### STD-KIND-040 — System-kind objects
- **Clause:** Objects of System kind MUST be written only by kernel services and by instructions dedicated to them (for example `emit.event` creates delivery jobs). Generic `commit.*` instructions targeting System-kind objects MUST fail with `LOGIC.FORBIDDEN_TARGET`. System-kind objects are branch-local and never merged.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1120…1129
- **Satisfies:** FR-MODEL-054
- **Notes:** —

## 4. RawDocument kind

### STD-KIND-060 — RawDocument semantics
- **Clause:** Bodies of RawDocument objects MAY be any canonical JSON within size limits. Only envelope fields and fields declared on the class are validated. L1 expressions, selectors, projections and ports MUST NOT reference undeclared content paths (`RULE.UNTYPED_ACCESS`). A declared parser (L2 or L3) MAY read the content and produce typed objects. On merge, a RawDocument conflicts whenever both sides changed its body.
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1140…1154
- **Satisfies:** FR-MODEL-055
- **Notes:** —

## 5. Kind stability

### STD-KIND-080 — Immutable kind
- **Clause:** A class's kind MUST NOT change once any object of the class, or of any subclass, exists on any branch of the VAE (`MODEL.KIND_IMMUTABLE`).
- **Grammar / Schema:** —
- **Conformance vectors:** VER-CONF-1160…1164
- **Satisfies:** FR-MODEL-056
- **Notes:** —
