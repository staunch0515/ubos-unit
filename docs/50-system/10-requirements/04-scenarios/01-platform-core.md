---
id: UBS-REQ-04-01
title: Scenarios — Platform Core
status: complete
phase: PH-1
depends_on: [UBS-REQ-04]
---

# Scenarios — Platform Core (SCN-001 … SCN-018)

### SCN-001 — Bootstrap a VAE from genesis
- **Goal:** A new, empty node becomes a working VAE with the base model, without manual database steps.
- **Personas:** PER-PlatformOperator, PER-PersonalUser
- **Systems:** NOD, DVM
- **Phase:** PH-1
- **Preconditions:** 1. A Server node or a Box is installed with an empty store. 2. The node carries the embedded genesis Buks `system` and `ontology`.
- **Main flow:**
  1. The operator runs the bootstrap command with a tenant name and an administrator identity → the node creates the tenant, the default VAE and the `main` branch.
  2. The node installs `system` and then `ontology` through a loader branch → each installation is one merged change set with a signed commit.
  3. The node generates the administrator's key pair and DID → the administrator principal is created with the role `tenant-admin`.
  4. The operator queries the class `Party` → the class, its inheritance chain and fields are returned.
- **Alternate and failure flows:**
  - A1. Bootstrap is interrupted after step 2 → rerunning bootstrap detects the partial state and completes it idempotently. No duplicate objects exist.
  - A2. The genesis Buk signature is invalid → bootstrap stops with `PKG.SIGNATURE_INVALID`, and the store stays empty.
- **Postconditions (observable):** 1. The history of `main` shows exactly two genesis merges. 2. Every genesis commit verifies against the publisher key. 3. The Merkle root of `main` is reported.
- **Business rules exercised:** FR-OPS-011, FR-PKG-071, FR-TEN-011, FR-IAM-031, FR-PROOF-021
- **Verification:** SCN, FAULT, CONF
- **Phase note:** PH-1 step 16 (DEC-077): each genesis Buk installs by one signed commit on `main` recording the Buk; installation through a loader branch follows in PH-2 (FR-PKG-031).

### SCN-002 — Define a class hierarchy and create instances
- **Goal:** A builder defines a base class and a subclass, and users create instances that are validated on commit.
- **Personas:** PER-BusinessArchitect, PER-BusinessUser
- **Systems:** DVM, STU (API only in PH-1)
- **Phase:** PH-1
- **Preconditions:** 1. SCN-001 completed.
- **Main flow:**
  1. The architect defines `Agreement` (Definition kind) with fields `title` (text, required), `effective_period` (period) and `parties` (relationship to `Party`, 2..n).
  2. The architect defines `ServiceAgreement` extending `Agreement`: it adds `service_level` (enum) and narrows `title` to at most 120 characters.
  3. The architect merges the change set into `main` → both classes are active.
  4. A user creates a `ServiceAgreement` with a 150-character title → the commit is rejected with `MODEL.CONSTRAINT_VIOLATED`, naming the field and the inherited-and-narrowed constraint.
  5. The user corrects the title and commits → the object exists with a version, transaction time and valid-time period.
- **Alternate and failure flows:**
  - A1. The architect tries to widen `title` in the subclass (for example, making it optional) → the class commit is rejected with `MODEL.NARROWING_VIOLATION`.
- **Postconditions (observable):** 1. Querying `Agreement` returns the `ServiceAgreement` instance (subclass inclusion). 2. The model export includes JSON Schema for both classes.
- **Business rules exercised:** FR-MODEL-011, FR-MODEL-021, FR-MODEL-041, FR-MODEL-061, FR-MODEL-101, FR-QRY-011
- **Verification:** SCN, CONF

### SCN-003 — Polymorphic action dispatch
- **Goal:** One action name behaves differently by subclass, and the platform chooses the most specific implementation.
- **Personas:** PER-BusinessArchitect, PER-LogicDeveloper
- **Systems:** DVM
- **Phase:** PH-1
- **Preconditions:** 1. SCN-002 completed. 2. `Agreement.terminate` is defined with L2 logic. 3. `ServiceAgreement.terminate` overrides it and adds a notice-period guard.
- **Main flow:**
  1. A user invokes `terminate` on a `ServiceAgreement` inside its notice period → the override's guard fails with an explanation that names the guard and the inherited rule chain.
  2. The user invokes `terminate` on a plain `Agreement` → the base logic runs and the state changes to `terminated`.
  3. A governance sheet with selector `Agreement[value > 1000000]` adds `require_approval: legal` → both classes are affected by one declaration.
- **Alternate and failure flows:**
  - A1. The override declares a narrower parameter type than the base port → the class commit is rejected with `MODEL.PORT_SIGNATURE_INCOMPATIBLE`.
- **Postconditions (observable):** 1. The execution records name the resolved implementation (class and logic version) for each call.
- **Business rules exercised:** FR-MODEL-031, FR-FLOW-021, FR-FLOW-031, FR-RULE-011
- **Verification:** SCN, CONF

### SCN-004 — Change on a draft, review the diff, merge
- **Goal:** A master-data change is prepared in isolation, reviewed as a diff and published atomically.
- **Personas:** PER-BusinessArchitect, PER-Approver
- **Systems:** DVM, STU (API only in PH-1)
- **Phase:** PH-1
- **Preconditions:** 1. 50 `Product` objects exist on `main`.
- **Main flow:**
  1. The architect opens a draft branch → the branch is created in constant time, and no object data is copied.
  2. The architect edits 12 products and adds 3 products → the edits are visible on the draft only.
  3. The architect requests the change-set diff → 15 entries are listed with field-level before and after values.
  4. The approver approves the change set → the merge produces one merge commit on `main`, and all 15 changes become visible at the same commit.
- **Alternate and failure flows:**
  - A1. `main` changed one of the same products after the draft was opened, on the same field → the merge reports the conflict with base, ours and theirs values, and nothing is merged until it is resolved.
  - A2. `main` changed a different field of the same product → the merge succeeds automatically.
- **Postconditions (observable):** 1. `main` history shows a merge commit with two parents. 2. Reading `main` at the commit before the merge shows the old state.
- **Business rules exercised:** FR-VER-021, FR-VER-031, FR-VER-041, FR-VER-051, FR-VER-101
- **Verification:** SCN, PROP, CONF

### SCN-005 — Concurrent edits are detected, never lost
- **Goal:** Two users editing the same record never overwrite each other silently.
- **Personas:** PER-BusinessUser
- **Systems:** DVM, SDK
- **Phase:** PH-1
- **Preconditions:** 1. Object `Customer/acme` exists at version v7.
- **Main flow:**
  1. User A and user B read v7.
  2. A commits a change based on v7 → it succeeds and creates v8.
  3. B commits a change based on v7 → it is rejected with `TXN.HEAD_MOVED`, naming the current version v8 and the changed fields.
  4. B's client fetches v8, re-applies B's change and commits → it succeeds and creates v9.
- **Alternate and failure flows:**
  - A1. 100 concurrent writers target the same object → exactly one succeeds per base version, and every other writer receives `TXN.HEAD_MOVED`.
- **Postconditions (observable):** 1. The version chain is linear (v7 → v8 → v9). 2. No acknowledged write is missing.
- **Business rules exercised:** FR-TXN-021, FR-TXN-011
- **Verification:** SCN, SIM, PROP

### SCN-006 — Future-dated price becomes effective automatically
- **Goal:** A price change approved today takes effect on the first day of next month without anyone acting on that day.
- **Personas:** PER-FundOperationsManager, PER-BusinessUser
- **Systems:** DVM
- **Phase:** PH-1
- **Preconditions:** 1. `PriceList/standard` is a Definition object valid from 2026-01-01 with an open end.
- **Main flow:**
  1. On 2026-09-15 a user commits a new version of the price list with valid time from 2026-10-01.
  2. On 2026-09-20 a query `asof=2026-09-20` returns the old prices, and a query `asof=2026-10-05` returns the new prices.
  3. On 2026-10-01 00:00 in the tenant time zone, calculations that use "current" prices use the new version with no job or manual step.
- **Alternate and failure flows:**
  - A1. A second future version with valid time from 2026-11-01 is added → each date resolves to exactly one effective version.
- **Postconditions (observable):** 1. The valid-time timeline of the object shows two non-overlapping periods.
- **Business rules exercised:** FR-TIME-021, FR-TIME-031, FR-TIME-051, FR-LOC-031
- **Verification:** SCN, CONF, PROP

### SCN-007 — Bitemporal question from an auditor
- **Goal:** An auditor answers the question "what did we believe on date X about date Y?".
- **Personas:** PER-InternalAuditor
- **Systems:** DVM
- **Phase:** PH-1
- **Preconditions:** 1. A rate object was corrected on 2026-03-10 with valid time from 2026-02-01 (a back-dated correction).
- **Main flow:**
  1. The auditor queries the rate with `asof=2026-02-15&time=2026-03-01` → the pre-correction value is returned.
  2. The auditor queries with `asof=2026-02-15&time=2026-03-11` → the corrected value is returned.
  3. The auditor requests the object history → both versions are listed with their transaction times, valid periods, authors and processes.
- **Alternate and failure flows:**
  - A1. `time` before the object's creation → the result is `QRY.NOT_FOUND_AT_TIME`, not an empty object.
- **Postconditions (observable):** 1. Results are identical when repeated. 2. The results carry the commit IDs used.
- **Business rules exercised:** FR-TIME-011, FR-TIME-031, FR-VER-101, FR-QRY-051, FR-AUD-011
- **Verification:** SCN, CONF, PROP

### SCN-008 — Ledger correction by reversal
- **Goal:** A wrong posting is corrected without altering history.
- **Personas:** PER-FundAccountant, PER-InternalAuditor
- **Systems:** DVM
- **Phase:** PH-1
- **Preconditions:** 1. The ledger class `CashMovement` holds entry #1041 for 10,000.00 USD, which should have been 1,000.00 USD.
- **Main flow:**
  1. The accountant invokes `reverse` on #1041 with a reason → entry #1187 is appended with the opposite amount and a reference to #1041.
  2. The accountant posts a new entry #1188 for 1,000.00 USD referencing #1041 as the corrected entry.
  3. The balance projection shows the corrected balance at the next read.
- **Alternate and failure flows:**
  - A1. An attempt to update #1041 directly → rejected with `LEDG.IMMUTABLE_ENTRY`.
  - A2. An attempt to merge a branch that contains ledger entries → the ledger entries are appended in branch order with new sequence numbers. They are never merged field by field.
- **Postconditions (observable):** 1. The sequence numbers are gap-free. 2. The audit shows the reversal chain #1041 → #1187 → #1188.
- **Business rules exercised:** FR-LEDG-011, FR-LEDG-021, FR-LEDG-031, FR-LEDG-041, FR-MODEL-051
- **Verification:** SCN, CONF, PROP

### SCN-009 — Compliance officer injects a control through a governance sheet
- **Goal:** A new control applies across all relevant records without any application change.
- **Personas:** PER-ComplianceOfficer
- **Systems:** DVM, STU (API only in PH-1)
- **Phase:** PH-1
- **Preconditions:** 1. The classes `Payment` and `Refund` both extend `MoneyMovement`.
- **Main flow:**
  1. The officer authors the sheet `MoneyMovement[amount > 10000] { audit_level: strict; require_signature: true; }` and merges it.
  2. A user commits a `Refund` of 12,000 without a signature → the commit is rejected with `RULE.DECLARATION_UNSATISFIED`. The explanation names the sheet, selector and declaration.
  3. A user commits a `Payment` of 9,000 without a signature → it is accepted.
  4. The officer asks for an explanation of the effective declarations of the refund → the cascade trace lists every matching sheet, its specificity and the winning values.
- **Alternate and failure flows:**
  - A1. Two sheets set `audit_level` with equal specificity and priority → the sheet merge is rejected with `RULE.AMBIGUOUS_CASCADE` at authoring time.
- **Postconditions (observable):** 1. The control is effective from the merge commit's transaction time and is recorded in the rule-version binding of each checked commit.
- **Business rules exercised:** FR-RULE-011, FR-RULE-021, FR-RULE-051, FR-TIME-061
- **Verification:** SCN, CONF

### SCN-010 — Runaway logic is stopped and rolled back
- **Goal:** Faulty logic cannot harm the node or leave partial state.
- **Personas:** PER-SecurityOfficer, PER-LogicDeveloper
- **Systems:** DVM, NOD
- **Phase:** PH-1
- **Preconditions:** 1. An L2 script contains an unbounded loop that commits in each iteration. 2. The static analyser misses it because the loop bound is data-dependent.
- **Main flow:**
  1. A user triggers the action → the script runs under profile `standard`.
  2. The operation limit is reached → execution terminates with `LOGIC.LIMIT_EXCEEDED`, and the script cannot catch it.
  3. The process is aborted → none of the script's writes are committed, and no event or job is released.
  4. The metering record shows the consumed operations and wall time.
- **Alternate and failure flows:**
  - A1. The script tries to open a network socket → it is rejected at load time by static analysis with `LOGIC.FORBIDDEN_CAPABILITY`.
- **Postconditions (observable):** 1. The node stays healthy. 2. Other tenants see no latency impact beyond the NR limits.
- **Business rules exercised:** FR-LOGIC-061, FR-LOGIC-091, FR-TXN-011, FR-TXN-051
- **Verification:** SCN, FAULT, SEC

### SCN-011 — Deterministic replay of a past process
- **Goal:** Any past process can be re-executed to reproduce exactly the same result.
- **Personas:** PER-InternalAuditor, PER-KernelEngineer
- **Systems:** DVM, FRG
- **Phase:** PH-1
- **Preconditions:** 1. A process P ran a fee calculation that called `ctx.now`, generated a random identifier and called an external rate connector (recorded).
- **Main flow:**
  1. The auditor runs replay of P → the kernel loads the exact logic versions, the input state at P's base commits and the recorded non-deterministic values.
  2. The replay produces the same writes, events and outputs byte-for-byte, and the report states "identical".
- **Alternate and failure flows:**
  - A1. The recorded connector response is missing (corrupted) → replay stops with `LOGIC.REPLAY_INPUT_MISSING`. It does not call the live connector.
- **Postconditions (observable):** 1. The replay leaves no state change on any branch.
- **Business rules exercised:** FR-LOGIC-071, FR-LOGIC-021, FR-TXN-041
- **Verification:** SCN, PROP, CONF

### SCN-012 — Invoice approval with guards and segregation of duties
- **Goal:** An invoice above a threshold needs two approvals by different people, and the maker cannot approve.
- **Personas:** PER-BusinessUser, PER-Approver, PER-ComplianceOfficer
- **Systems:** DVM, WSP
- **Phase:** PH-2
- **Preconditions:** 1. The `Invoice` lifecycle is `draft → submitted → approved → paid`. 2. The sheet `Invoice[amount > 50000] { approvals: 2; maker_checker: true; }` is active.
- **Main flow:**
  1. A user creates and submits an invoice of 80,000 → the transition to `approved` is deferred, and two approval tasks are created.
  2. The maker tries to approve → the action is denied with an explanation citing maker–checker.
  3. Approver 1 approves → the approval is recorded, and the state stays `submitted`.
  4. Approver 2 approves → the deferred commit is applied, and the state becomes `approved`.
- **Alternate and failure flows:**
  - A1. Approver 2 rejects with a reason → the state returns to `draft`, the reason is visible, and the other approval is voided.
  - A2. The invoice is edited while approvals are pending → the pending approvals are invalidated, and the diff since submission is shown.
- **Postconditions (observable):** 1. The audit trail shows maker, approvers, timestamps and the rule versions applied.
- **Business rules exercised:** FR-FLOW-011, FR-FLOW-041, FR-FLOW-051, FR-RULE-051, FR-UX-061
- **Verification:** SCN, USE

### SCN-013 — Tenant overlay survives a vendor upgrade
- **Goal:** A customer's customisations survive an ISV upgrade, and real conflicts are reported.
- **Personas:** PER-ImplementationConsultant, PER-IsvDeveloper, PER-TenantAdministrator
- **Systems:** DVM, STU, FRG
- **Phase:** PH-2
- **Preconditions:** 1. Buk `contracts@1.2.0` is installed. 2. The tenant overlay adds field `cost_center` to `Contract` and overrides the view `Contract.form`.
- **Main flow:**
  1. The administrator installs `contracts@1.3.0` into a loader branch → the diff against 1.2.0 is shown.
  2. The platform merges the loader into the overlay → the new field `renewal_notice_days` is added, and `cost_center` is preserved.
  3. The vendor also changed `Contract.form` → a conflict is reported for the overridden view, with base, vendor and tenant versions.
  4. The consultant resolves the conflict in Studio → the release is published.
- **Alternate and failure flows:**
  - A1. The administrator cancels → the overlay is unchanged, and the loader branch is discarded.
- **Postconditions (observable):** 1. Tenants that did not override `Contract.form` receive the vendor's new form automatically.
- **Business rules exercised:** FR-PKG-041, FR-TEN-031, FR-VER-081, FR-VER-051, FR-MODEL-071
- **Verification:** SCN, PROP

### SCN-014 — Schema evolution with a migration lens
- **Goal:** A field split does not break old versions, reports or merges.
- **Personas:** PER-BusinessArchitect, PER-DataAnalyst
- **Systems:** DVM, FRG
- **Phase:** PH-2
- **Preconditions:** 1. `Person.name` (text) holds 10,000 versions across two years.
- **Main flow:**
  1. The architect changes `Person` to `given_name` and `family_name` and attaches a lens (split forward, join backward).
  2. The class change is checked by Forge → the lens round-trip test runs on a sample of existing versions.
  3. After the merge, reading a 2025 version of any person returns the new shape through the lens, and the stored bytes are unchanged.
  4. A draft branch opened before the change and editing `name` is merged → the merge aligns both sides to the new schema version before the three-way merge.
- **Alternate and failure flows:**
  - A1. A lens is not total for some values (for example a single-word name) → Forge reports the failing samples, and the class change cannot merge until the lens handles them.
- **Postconditions (observable):** 1. Proofs of old versions still verify, because stored bytes did not change.
- **Business rules exercised:** FR-MODEL-081, FR-VER-051, FR-DEV-041
- **Verification:** SCN, PROP, CONF

### SCN-015 — Spreadsheet import becomes a model and records
- **Goal:** A team moves a tracking spreadsheet into UBOS in minutes.
- **Personas:** PER-PersonalUser, PER-ImplementationConsultant
- **Systems:** BRG, DVM, WSP
- **Phase:** PH-2
- **Preconditions:** 1. An XLSX file holds 3 sheets, 2,400 rows, merged header cells and one lookup column.
- **Main flow:**
  1. The user uploads the file → the importer proposes classes, field types, a relationship for the lookup column and a primary key.
  2. The user adjusts two field types and confirms → the classes are created on a draft branch, and records are imported with row-level results.
  3. The reconciliation report shows 2,400 rows read, 2,391 imported and 9 rejected, with reasons.
  4. The user merges → the records are available in Workspace with views generated from the classes.
- **Alternate and failure flows:**
  - A1. The file is re-imported → the rows are matched by key, and unchanged rows create no new versions.
- **Postconditions (observable):** 1. The import mapping is saved as a versioned object for reuse.
- **Business rules exercised:** FR-MIG-011, FR-MIG-041, FR-MIG-051, FR-UX-011
- **Verification:** SCN, USE

### SCN-016 — Privacy erasure by crypto-shredding
- **Goal:** A data subject's personal data is erased without breaking history integrity.
- **Personas:** PER-ComplianceOfficer
- **Systems:** DVM, NOD
- **Phase:** PH-2
- **Preconditions:** 1. `Person` fields `email`, `phone` and `date_of_birth` are classified `personal` and encrypted with a per-subject key. 2. No legal hold applies.
- **Main flow:**
  1. The officer executes the erasure request for subject S → the subject key is destroyed, and an erasure record is committed.
  2. Every version of S's personal fields now reads as `erased`, and non-personal fields remain.
  3. Commit signatures and Merkle proofs of the affected commits still verify, because ciphertexts are unchanged.
- **Alternate and failure flows:**
  - A1. (Verified from PH-3, when legal holds exist.) A legal hold covers S → erasure is refused with `AUD.LEGAL_HOLD_ACTIVE`, and the refusal is recorded.
- **Postconditions (observable):** 1. Backups older than the erasure are covered, because the key is absent from all key stores after the key-store retention window.
- **Business rules exercised:** FR-PROOF-071, FR-AUD-011, FR-IAM-091
- **Verification:** SCN, INSP, AUDIT

### SCN-017 — Backup and point-in-time restore
- **Goal:** An operator restores a node to a chosen point and proves that the restored state is exact.
- **Personas:** PER-PlatformOperator
- **Systems:** NOD, DVM
- **Phase:** PH-1
- **Preconditions:** 1. Continuous backup is enabled on a Server node.
- **Main flow:**
  1. The operator restores to 2026-09-28T14:05:00Z in a separate environment.
  2. The node verifies that the Merkle roots of the restored branch heads equal those recorded at the chosen time.
  3. The operator runs the projection rebuild check → the rebuilt projections match the restored ones.
- **Alternate and failure flows:**
  - A1. The backup chain is broken → the restore reports the last consistent point and does not restore past it.
- **Postconditions (observable):** 1. The restore report lists the heads, roots and verification results.
- **Business rules exercised:** FR-OPS-021, FR-PROOF-031, FR-ANL-021
- **Verification:** SCN, FAULT

### SCN-018 — Revert a whole process
- **Goal:** A faulty bulk operation is undone in one action, with full traceability.
- **Personas:** PER-BusinessArchitect, PER-AgentSupervisor
- **Systems:** DVM
- **Phase:** PH-1
- **Preconditions:** 1. Process P updated 340 Definition objects and appended 12 ledger entries on `main`. 2. Later commits changed 5 of those objects again.
- **Main flow:**
  1. The architect requests revert of P → the platform computes compensating changes: inverse updates for the 335 unchanged objects, a conflict list for the 5 changed objects, and reversal entries for the 12 ledger entries.
  2. The architect resolves the 5 conflicts (keep the later change, or revert) → one revert process commits.
- **Alternate and failure flows:**
  - A1. P's events triggered process Q → the revert report lists Q as a dependent process for optional revert.
- **Postconditions (observable):** 1. History contains P and its revert. 2. Nothing is deleted.
- **Business rules exercised:** FR-VER-071, FR-LEDG-021, FR-TXN-041
- **Verification:** SCN, PROP
