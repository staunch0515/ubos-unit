---
id: UBS-SYS-DVM-02-06
title: DVM — Authorization, Query and Proof Design
status: draft
phase: PH-1
depends_on: [UBS-SYS-DVM-02-05, UBS-STD-08, UBS-STD-12, UBS-STD-16]
---

# DVM — Authorization, Query and Proof Design (DSN-DVM-8xx, DSN-DVM-9xx)

This file designs the crates `ubos-authz`, `ubos-query` and `ubos-proof`. Reading order:
1. policy engine and decisions (801–808);
2. relations (ReBAC) (810–813);
3. row filters and masking (820–824);
4. field encryption and crypto-shredding (830–835);
5. access analysis (840–842);
6. query planner and execution (901–910);
7. view resolution (920–923);
8. proofs (930–938);
9. evidence packs, redaction and seals (940–944);
10. export, import and sync structures (950–956).

## Policy engine and decisions

### DSN-DVM-801 — One policy engine
- **Statement:** `ubos-authz` MUST embed a Cedar-class policy engine (DEC-021) and MUST be the only component that decides access (AR-024). Every enforcement point (instructions, reads, queries, diffs, exports, reflection, sync scopes) MUST call `authorize(principal, action, resource, context) → Decision` or its partial-evaluation form. Default MUST be deny (FR-IAM-041).
- **Rationale:** Consistent, analysable authorization.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the permission matrix suite, when run against every enforcement point, then 100% of outcomes match the expected matrix.
  2. Given no applicable policy, when any action is attempted, then it is denied.
- **Verification:** SEC, CONF
- **Origin:** FR-IAM-041, FR-IAM-042, AR-024, DEC-021, CR-SOX-001

### DSN-DVM-802 — Entity model for policies
- **Statement:** The policy entity model MUST be generated from the business model: principals (users, groups, roles, service principals, agents), resources (VAE, branch, class, object, field), actions (the instruction catalogue plus read, query, export, merge, approve). Object attributes available to policies MUST be limited to fields declared policy-visible, so policy evaluation never reads masked data.
- **Rationale:** Policies speak the business vocabulary (FR-IAM-051).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a new class, when committed, then the entity schema includes it, and existing policies still validate.
- **Verification:** CONF
- **Origin:** FR-IAM-051, FR-IAM-042, FR-IAM-031

### DSN-DVM-803 — Policies as governed objects
- **Statement:** Policies MUST be Definition objects, versioned and changed through change sets on protected branches (FR-IAM-044). The compiled policy set MUST be cached by `(policy set hash, entity schema hash)`. Policy validation against the entity schema MUST run at commit time.
- **Rationale:** FR-IAM-044.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a policy referencing an unknown attribute, when committed, then it is rejected.
- **Verification:** CONF
- **Origin:** FR-IAM-044

### DSN-DVM-804 — Not visible means not found
- **Statement:** When a principal cannot read a resource, point reads MUST return `IAM.NOT_FOUND`, identical in shape and timing class to a missing resource. `IAM.DENIED` MUST be returned only for actions on resources the principal can see.
- **Rationale:** FR-IAM-043.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given an existing invisible object and a non-existent ID, when read, then responses are indistinguishable.
- **Verification:** SEC
- **Origin:** FR-IAM-043

### DSN-DVM-805 — Decision logging
- **Statement:** Every deny, and every allow for actions flagged sensitive, MUST produce a decision log record (principal, action, resource, policy IDs, outcome) delivered to the host for the security audit log (FR-IAM-045, FR-AUD-012). Log records MUST NOT contain field values.
- **Rationale:** FR-IAM-045, NR-SEC-009.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a denied export, when the audit log is read, then the record names the determining policy.
- **Verification:** SEC, CONF
- **Origin:** FR-IAM-045, FR-AUD-012, NR-SEC-009, NR-SEC-010

### DSN-DVM-806 — Decision cache
- **Statement:** Decisions MAY be cached per process by `(principal, action, resource version, policy set hash)`. No decision cache MUST outlive a process unless keyed by all inputs including relation state `cseq`.
- **Rationale:** Speed without stale permissions (AR-028, NR-SEC-005).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a revoked role, when the next process starts, then the revocation is effective.
- **Verification:** SEC, CONF
- **Origin:** AR-028, NR-SEC-005, FR-IAM-102

### DSN-DVM-807 — Branch-level permissions
- **Statement:** Actions MUST be authorized against the branch as a resource as well as the object (FR-VER-026). Principals MAY have broader rights on work branches they own than on `main`.
- **Rationale:** Safe experimentation.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given a user with write on their work branch only, when writing to `main`, then it is denied.
- **Verification:** SEC
- **Origin:** FR-VER-026

### DSN-DVM-808 — Break-glass
- **Statement:** Break-glass access (AR-025) MUST be an explicit context flag granted by the host after step-up authentication, bounded in time, and it MUST force logging of every read and write in the session.
- **Rationale:** AR-025.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a break-glass session, when it reads 10 objects, then 10 read-log records exist.
- **Verification:** SEC
- **Origin:** AR-025, FR-IAM-024

## Relations

### DSN-DVM-810 — Relation tuples from the model
- **Statement:** Authorization relations (FR-IAM-051) MUST be derived from declared relationships marked authorization-relevant (for example `fund.manager`, `contract.party`) and stored as tuples in `relationship_index`. No separate tuple store MUST exist.
- **Rationale:** One source of truth for relations.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a new manager assignment, when committed, then relation checks see it in the next process.
- **Verification:** CONF
- **Origin:** FR-IAM-051, FR-IAM-053

### DSN-DVM-811 — Relation-based rules
- **Statement:** Policies MUST be able to use relation paths of bounded depth (default 4), for example `principal in resource.fund.team.members`. The engine MUST evaluate paths by index lookups at the process snapshot.
- **Rationale:** FR-IAM-052.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a 3-hop path, when evaluated for 10,000 objects in a list query, then latency meets NR-PERF-008.
- **Verification:** BENCH, CONF
- **Origin:** FR-IAM-052, NR-PERF-008

### DSN-DVM-812 — Consistent relation checks
- **Statement:** Relation checks MUST use the same snapshot as the data being read (FR-IAM-053), so there is no "new enemy" window within one process.
- **Rationale:** FR-IAM-053.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a removal of a user from a team and a document added afterwards, when the user reads at the later snapshot, then access is denied.
- **Verification:** SIM, SEC
- **Origin:** FR-IAM-053

### DSN-DVM-813 — Group expansion cache
- **Statement:** Group and role membership closures MUST be precomputed as a projection maintained in the flush, keyed by `(principal, cseq)`.
- **Rationale:** Fast checks for nested groups.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given 5-level nested groups, when checked, then membership is resolved with one lookup.
- **Verification:** BENCH, PROP
- **Origin:** FR-IAM-073, AR-016

## Row filters and masking

### DSN-DVM-820 — Row filters by partial evaluation
- **Statement:** For list queries, the engine MUST partially evaluate the policy set with the resource unknown, producing a residual predicate over resource attributes and relations. The query planner MUST push the residual into index scans (FR-IAM-062, FR-QRY-012). Residuals it cannot push MUST be applied as a post-filter, with scan limits enforced.
- **Rationale:** Authorization inside queries, not after them.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a user who can see 1% of 1,000,000 objects, when listing, then the result contains only visible objects, and latency meets NR-PERF-008.
  2. Given random policies and data, when comparing the filtered list with per-object checks, then they are equal.
- **Verification:** PROP, BENCH, SEC
- **Origin:** FR-IAM-062, FR-QRY-012, NR-PERF-008
- **Phase note:** PH-1 step 13d (DEC-074): in PH-1 the policy set is applied to the candidate rows as a residual post-filter after the index scan, its cost bounded by the order-by pushdown; partial evaluation and residual predicates pushed into the scan are revisited when `cedar-policy`'s partial evaluation stabilises or the ENV-REF-SERVER run of VER-BENCH-2000 fails MET-PERF-016.

### DSN-DVM-821 — Field masking
- **Statement:** After authorization, results MUST be passed through a masking step driven by field classification and policy (FR-IAM-061): full, partial (declared pattern), hashed or removed. Masking MUST apply to reads, query results, diffs, explanations, events (FR-IAM-063) and exports.
- **Rationale:** FR-IAM-061, FR-IAM-063, FR-AUD-033.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a masked tax ID, when read through every output path, then the value never appears unmasked.
- **Verification:** SEC, CONF
- **Origin:** FR-IAM-061, FR-IAM-063, FR-AUD-033, AR-022

### DSN-DVM-822 — Masked fields in expressions
- **Statement:** Filters and sorts on fields the caller cannot see MUST be rejected (`QRY.INVALID_QUERY`) rather than allowing inference by probing.
- **Rationale:** No side channels through queries.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a filter on a masked salary field, when submitted, then it is rejected.
- **Verification:** SEC
- **Origin:** FR-IAM-061

### DSN-DVM-823 — Personal-data read logging
- **Statement:** Reads of fields classified as personal or sensitive by principals other than the subject MUST emit read-log records (FR-AUD-051, NR-PRIV-005), batched per process.
- **Rationale:** FR-AUD-051.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a list query returning 50 personal records, when completed, then one batched read-log record lists 50 IDs.
- **Verification:** CONF
- **Origin:** FR-AUD-051, NR-PRIV-005

### DSN-DVM-824 — Privacy by default
- **Statement:** New fields without an explicit classification MUST be classified `internal` by default; personal-data field types (name, email, tax ID) MUST default to `personal`.
- **Rationale:** NR-PRIV-006.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a new email field without classification, when committed, then it is `personal`.
- **Verification:** CONF
- **Origin:** NR-PRIV-006, NR-PRIV-003, STD-FND-027
- **Phase note:** PH-1 step 19b (DEC-086): in PH-1 a field is classified by its declaration, else by the class `classification_default`, else `internal`; there are no personal-data field types in PH-1. Classifying the ontology `Person` name and email as `personal` comes with the PH-2 genesis upgrade.

## Field encryption and crypto-shredding

### DSN-DVM-830 — Per-subject field encryption
- **Statement:** Fields classified personal MUST be encrypted before hashing (STD-OBJ-010) with a per-subject data key (AES-256-GCM, key ID in the ciphertext envelope). Subject keys MUST be wrapped by the tenant key through the host key service (FR-IAM-092). The version stores ciphertext and a salted field commitment.
- **Rationale:** FR-PROOF-071, FR-PROOF-072.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a personal field, when the stored version is inspected, then only ciphertext and a commitment are present.
- **Verification:** SEC, CONF
- **Origin:** FR-PROOF-071, FR-PROOF-072, FR-IAM-092, STD-OBJ-010, AR-020

### DSN-DVM-831 — Erasure by key destruction
- **Statement:** Erasure of a subject MUST destroy the subject key through the host key service, after checking holds and retention (FR-PROOF-075, `AUD.LEGAL_HOLD_ACTIVE`). Afterwards, reads MUST return the erased marker (`PROOF.VALUE_ERASED`, STD-OBJ-011), and proofs MUST still verify.
- **Rationale:** FR-PROOF-073, NR-PRIV-001.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given an erased subject, when historical versions are read, then values are erased, and inclusion proofs of those versions verify.
  2. Given an active legal hold, when erasure is requested, then it is refused.
- **Verification:** SEC, CONF
- **Origin:** FR-PROOF-073, FR-PROOF-075, STD-OBJ-011, NR-PRIV-001

### DSN-DVM-832 — Backups honour erasure
- **Statement:** Subject keys MUST NOT be stored in business backups; they MUST live in the key service, whose own backups apply a tombstone list on restore (FR-PROOF-074).
- **Rationale:** Erasure survives restores.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a restore from a backup older than an erasure, when reading, then the subject's values are still erased.
- **Verification:** FAULT, SEC
- **Origin:** FR-PROOF-074

### DSN-DVM-833 — Equality lookups on encrypted keys
- **Statement:** Fields used as keys MAY carry a deterministic HMAC lookup token (STD-OBJ-012, AR-021) with a per-tenant key. Tokens MUST be dropped on erasure.
- **Rationale:** Lookups without exposing values.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given an encrypted email key, when looked up by value, then the object is found; after erasure it is not.
- **Verification:** CONF, SEC
- **Origin:** STD-OBJ-012, AR-021

### DSN-DVM-834 — Key unavailability
- **Statement:** When a key is temporarily unavailable, reads MUST fail with `IAM.KEY_UNAVAILABLE` (retryable), distinct from erased values.
- **Rationale:** Distinguish outage from erasure.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the key service down, when reading an encrypted field, then `IAM.KEY_UNAVAILABLE` is returned.
- **Verification:** FAULT
- **Origin:** FR-IAM-093

### DSN-DVM-835 — Content disposal
- **Statement:** Disposal of RawDocument content under retention (STD-PROOF-060) MUST delete chunks while keeping their IDs and a disposal certificate, so proofs over the IDs still verify.
- **Rationale:** FR-AUD-072.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a disposed document, when its commit is verified, then verification succeeds, and the content read returns the disposal certificate.
- **Verification:** CONF
- **Origin:** FR-AUD-071, FR-AUD-072, STD-PROOF-060

### DSN-DVM-836 — Rectification of personal data
- **Statement:** Rectifying personal data MUST create a new version with a reason code `rectification`; earlier versions stay in history encrypted with the subject key, marked "rectified" so that business reads, exports and model calls use the rectified value, while audit reads with the audit right can see the history (CR-GDPR-003).
- **Rationale:** Rectification without destroying the audit trail.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a rectified address, when exported for the subject, then only the rectified value appears; an auditor with the audit right sees both versions.
- **Verification:** CONF, SEC
- **Origin:** CR-GDPR-003, FR-PROOF-071
- **Phase note:** PH-1 step 19b (DEC-086): in PH-1 a rectification is a new version whose commit carries the reason `rectification`; reads use the rectified value and the audit trail keeps both values. Encrypting the earlier versions with the subject key follows DSN-DVM-830 in PH-2.

### DSN-DVM-837 — Subject access export
- **Statement:** The DVM MUST produce a subject export: all objects, versions and ledger entries linked to a subject through declared subject relations, in JSON with a manifest, within NR-PRIV-002 after approval (CR-CCPA-001, CR-GDPR-002). The export MUST exclude other subjects' personal data by masking.
- **Rationale:** CR-CCPA-001, CR-GDPR-002, NR-PRIV-002.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given an investor linked to 3 funds and 200 transactions, when exported, then all linked records are present, and co-investors' personal fields are masked.
- **Verification:** CONF, SEC
- **Origin:** CR-CCPA-001, CR-GDPR-002, NR-PRIV-002

## Access analysis

### DSN-DVM-840 — Who-can and what-can
- **Statement:** The engine MUST answer "who can do action A on resource R" and "what can principal P do" (FR-IAM-081) by combining policy analysis with the relation and membership projections.
- **Rationale:** FR-IAM-081.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a fixture tenant, when who-can is asked for approving payments, then the answer equals the brute-force evaluation.
- **Verification:** PROP
- **Origin:** FR-IAM-081

### DSN-DVM-841 — Policy change impact
- **Statement:** Before a policy change is merged, the engine MUST report principals gaining or losing access per action class, computed by differential analysis of old and new policy sets.
- **Rationale:** Safe policy change.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a policy that widens export rights, when previewed, then the new exporters are listed.
- **Verification:** CONF
- **Origin:** FR-IAM-044, FR-IAM-082

### DSN-DVM-842 — Unused permission detection
- **Statement:** The DVM MUST maintain per-principal, per-action last-use counters from decision logs to support unused-permission reports (FR-IAM-083).
- **Rationale:** Least privilege over time.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a role unused for 90 days, when reported, then it appears.
- **Verification:** CONF
- **Origin:** FR-IAM-083

## Query planner and execution

### DSN-DVM-901 — Query pipeline
- **Statement:** `ubos-query` MUST process a query (STD-EXPR-010) in these steps:
  1. parse and type-check against the effective definitions;
  2. resolve the snapshot and valid time;
  3. add the authorization residual (DSN-DVM-820);
  4. plan: choose indexes (kernel indexes, declared projections, subtree index), join order for traversals, and sort strategy;
  5. execute with scan limits (`QRY.SCAN_LIMIT`);
  6. mask;
  7. page with a snapshot cursor.
- **Rationale:** FR-QRY-011, FR-QRY-013, FR-QRY-014.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the query vectors, when executed, then results equal the vectors.
- **Verification:** CONF
- **Origin:** FR-QRY-011, FR-QRY-013, FR-QRY-014, STD-EXPR-010, STD-EXPR-011, FR-VER-102

### DSN-DVM-902 — Declared indexes
- **Statement:** Classes MAY declare field indexes (FR-ANL-014). The DVM MUST maintain them in a generic `field_index` table `(tenant, branch, class_ord, field id, value key, object, cseq_from, cseq_to)` in the flush, with value keys encoded order-preserving per type. Business classes MUST NOT create DDL (AR-018).
- **Rationale:** Indexed queries without per-class tables.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given an indexed field and 1,000,000 objects, when querying by equality and range, then latency meets NR-PERF-008.
- **Verification:** BENCH
- **Origin:** FR-ANL-014, AR-018, NR-PERF-008

### DSN-DVM-903 — Scan limits
- **Statement:** Every query MUST have a scan budget from its profile. Queries without a usable index whose estimated scan exceeds the budget MUST fail at planning with `QRY.SCAN_LIMIT` and a suggested index.
- **Rationale:** FR-QRY-014, NR-PERF-022.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given an unindexed filter over 1,000,000 objects, when planned, then it fails with a suggestion.
- **Verification:** CONF
- **Origin:** FR-QRY-014, NR-PERF-022

### DSN-DVM-904 — Same semantics in logic
- **Statement:** `search.query` from logic and queries from the host API MUST use the same planner and executor (FR-QRY-015).
- **Rationale:** One semantics.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a query run from a script and from the API, when compared, then results are identical.
- **Verification:** CONF
- **Origin:** FR-QRY-015, STD-ISA-080

### DSN-DVM-905 — Aggregates
- **Statement:** `search.aggregate` MUST support count, sum, min, max, avg (decimal) and group-by. It MUST use projections when a matching one exists. Money sums MUST group by currency (FR-QRY-042).
- **Rationale:** FR-QRY-041, FR-QRY-042.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given mixed-currency amounts, when summed, then one sum per currency is returned.
- **Verification:** CONF
- **Origin:** FR-QRY-041, FR-QRY-042, STD-ISA-081

### DSN-DVM-906 — Traversals
- **Statement:** `search.traverse` MUST walk relationships with a declared maximum depth and fan-out limit, at consistent temporal coordinates, with authorization per hop.
- **Rationale:** FR-MODEL-065.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given an ownership graph with cycles, when traversed to depth 5, then each node appears once, and invisible nodes are omitted.
- **Verification:** CONF, SEC
- **Origin:** FR-MODEL-065, STD-ISA-082

### DSN-DVM-907 — Change and transition queries
- **Statement:** The DVM MUST answer change queries ("objects of class C changed between commits X and Y") from the diff engine and transition queries ("objects that entered state S between t1 and t2") from lifecycle history (FR-QRY-051, FR-QRY-052).
- **Rationale:** Incremental consumers and operations reporting.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given 100 transitions in a period, when queried, then all are returned with times.
- **Verification:** CONF
- **Origin:** FR-QRY-051, FR-QRY-052

### DSN-DVM-908 — Snapshot cursors
- **Statement:** Cursors MUST encode `(query hash, snapshot map, asof, last sort key, per-level positions)` and be HMAC-signed with a tenant key. Pages MUST be stable across concurrent commits (FR-QRY-071), with a total order that breaks ties by object ID (FR-QRY-072). Expired cursors MUST fail with `INT.CURSOR_EXPIRED`.
- **Rationale:** Consistent paging.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given commits during paging, when all pages are read, then no item is duplicated or missing relative to the snapshot.
  2. Given a tampered cursor, when used, then it is rejected.
- **Verification:** PROP, SEC
- **Origin:** FR-QRY-071, FR-QRY-072

### DSN-DVM-909 — Saved queries
- **Statement:** Saved queries (FR-QRY-061) MUST be Definition objects with parameters, reusable by lists, reports, inboxes, agents and exports (FR-QRY-062).
- **Rationale:** One definition, many surfaces.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given a saved query, when used in a list and in an export, then both return the same rows.
- **Verification:** CONF
- **Origin:** FR-QRY-061, FR-QRY-062

### DSN-DVM-910 — Text and vector search hooks
- **Statement:** `search.text` and `search.vector` MUST be implemented by calling host index services (NOD or BRG adapters) with the authorization residual and classification constraints, and MUST re-check every returned hit against the snapshot and authorization before returning it (FR-QRY-022, FR-QRY-033).
- **Rationale:** External indexes never widen access.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a stale text index containing a revoked document, when searched, then the hit is dropped.
- **Verification:** SEC, CONF
- **Origin:** FR-QRY-021, FR-QRY-022, FR-QRY-031, FR-QRY-032, FR-QRY-033, STD-ISA-083, STD-ISA-084

## View resolution

### DSN-DVM-920 — View payloads
- **Statement:** View resolution (STD-CLS-023) MUST select the most specific view for the class and context (FR-UX-015), generate a default view when none exists (FR-UX-012), and return a payload with the data, field states (hidden, read-only, required from sheets and permissions) and available actions (DSN-DVM-732).
- **Rationale:** FR-UX-011, FR-UX-013.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a user without edit rights, when resolving a form, then all fields are read-only in the payload.
  2. Given a detail view, when resolved, then latency meets NR-PERF-014.
- **Verification:** CONF, BENCH
- **Origin:** FR-UX-011, FR-UX-012, FR-UX-013, FR-UX-015, FR-MODEL-036, STD-CLS-023, NR-PERF-014

### DSN-DVM-921 — Live validation payloads
- **Statement:** Form payloads MUST support incremental validation: given staged edits, the DVM returns violations and recomputed field states without a flush (FR-UX-031, DSN-DVM-709).
- **Rationale:** FR-UX-031.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given an edit that makes a field required, when validated, then the payload marks it required.
- **Verification:** CONF
- **Origin:** FR-UX-031, FR-RULE-035

### DSN-DVM-922 — View determinism
- **Statement:** For a snapshot, principal and view version, the payload MUST be deterministic, so renderings are reproducible (NR-DET-004).
- **Rationale:** Evidence of what a user saw.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given the same inputs twice, when resolved, then payloads are byte-identical.
- **Verification:** CONF
- **Origin:** NR-DET-004

### DSN-DVM-923 — Subscriptions support
- **Statement:** The DVM MUST expose, per commit, the set of changed objects and affected saved-query predicates, so the host can push updates to subscribed views (NR-PERF-015).
- **Rationale:** Live UIs without polling.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a subscribed list, when a matching object changes, then the host is notified within NR-PERF-015.
- **Verification:** BENCH
- **Origin:** NR-PERF-015, FR-UX-032

## Proofs

### DSN-DVM-930 — State root per commit
- **Statement:** Each commit MUST carry the state root of its effective branch state (STD-PROOF-003), computed by DSN-DVM-007. Roots MUST be history-independent (STD-PROOF-002): the same state yields the same root regardless of the operation order.
- **Rationale:** FR-PROOF-031, NR-DET-003.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given 1,000 random permutations of the same set of inserts, when roots are computed, then they are equal.
- **Verification:** PROP, CONF
- **Origin:** FR-PROOF-031, STD-PROOF-001, STD-PROOF-002, STD-PROOF-003, NR-DET-003

### DSN-DVM-931 — Inclusion proofs
- **Statement:** `prove_inclusion(commit, key)` MUST return an `InclusionProof` (STD-PROOF-020) with the path of tree nodes; `verify_inclusion` MUST be a standalone function in `ubos-proof` with no storage dependency, compilable to WASM.
- **Rationale:** FR-PROOF-032, FR-PROOF-061.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given proofs for 10,000 random keys, when verified by the standalone verifier, then all verify, and any single-bit change fails.
- **Verification:** CONF, PROP
- **Origin:** FR-PROOF-032, FR-PROOF-061, STD-PROOF-020
- **Phase note:** PH-1 step 6a (DEC-059) implements `inclusion_proof` and `verify_inclusion` in `ubos-proof` (dependencies `ubos-tree`, `ubos-crypto` only, no store); the store-reading `prove` of the adapter follows in part 6b.

### DSN-DVM-932 — Non-inclusion proofs
- **Statement:** For absent keys the DVM MUST return a proof of the two neighbouring keys, showing absence at the commit.
- **Rationale:** Prove that something did not exist.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given an absent key, when proven, then the verifier confirms absence.
- **Verification:** CONF
- **Origin:** STD-PROOF-020

### DSN-DVM-933 — Consistency proofs
- **Statement:** `prove_consistency(commit A, commit B)` MUST show that A is an ancestor of B on the first-parent chain by providing the chain of commit headers (STD-PROOF-021), compressed by skip links stored every 2^k commits.
- **Rationale:** FR-PROOF-033.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given commits 1,000,000 apart, when proven, then the proof has O(log n) headers and verifies.
- **Verification:** CONF, BENCH
- **Origin:** FR-PROOF-033, STD-PROOF-021
- **Phase note:** BPA 1.0-rc1 (STD-PROOF-021) carries the plain first-parent chain; PH-1 step 6a (DEC-059) implements its verification (`verify_consistency` in `ubos-proof`) and part 6b the proof from the store. The skip links of this item stay in PH-2.

### DSN-DVM-934 — Tenant root
- **Statement:** The DVM MUST compute a tenant root (STD-PROOF-030) over the heads of all VAEs and protected branches of a tenant, on request and at the anchoring cadence, for NTY to anchor.
- **Rationale:** FR-PROOF-034.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, NTY
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a tenant root and a commit on a protected branch, when proven, then a path from the commit to the tenant root verifies.
- **Verification:** CONF
- **Origin:** FR-PROOF-034, STD-PROOF-030
- **Phase note:** The tenant root of BPA 1.0-rc1 (STD-PROOF-030) over the heads of the active main, release and overlay branches of one VAE is implemented in PH-1 step 6b (DEC-059, `tenant_root`, node-signed); the roots over all VAEs of a tenant, the anchoring cadence and NTY stay in PH-3.

### DSN-DVM-935 — Anchor receipts
- **Statement:** The DVM MUST store anchor receipts (STD-PROOF-040) delivered by NTY and link them to the tenant roots they cover, so proofs can include the receipt.
- **Rationale:** FR-PROOF-041, FR-PROOF-042.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, NTY
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given an anchored commit, when a proof is exported, then it includes the receipt, and the verifier checks it.
- **Verification:** CONF
- **Origin:** FR-PROOF-041, FR-PROOF-042, STD-PROOF-040

### DSN-DVM-936 — Verification on receipt
- **Statement:** Every commit received from another node or device (sync, import, federation) MUST be verified before acceptance: IDs, signatures, state root recomputation over transferred nodes (FR-PROOF-023). Failures MUST reject the transfer with `SYNC.INTEGRITY_FAILURE`.
- **Rationale:** FR-PROOF-023.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a transfer with one tampered node, when received, then it is rejected, and nothing is applied.
- **Verification:** SEC, CONF
- **Origin:** FR-PROOF-023, STD-SYNC-003

### DSN-DVM-937 — Node co-signature
- **Statement:** The host MAY add a node co-signature to commits (FR-PROOF-022) through DSN-DVM-522.
- **Rationale:** Evidence of which node accepted a commit.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given co-signing enabled, when a commit is verified, then both author and node signatures verify.
- **Verification:** CONF
- **Origin:** FR-PROOF-022
- **Phase note:** Brought forward to PH-1 by DEC-059 (step 6a) as the signature every commit record of the conformance adapter carries: the version layer signs the commit ID with the node key of the store's node record at flush (STD-PROOF-011). Author signatures and the separate signature table of DSN-DVM-522 keep their phases.

### DSN-DVM-938 — Verification reports
- **Statement:** The verifier MUST produce a structured report (FR-PROOF-062): each check, pass or fail, and for failures the exact object and reason in plain language.
- **Rationale:** Auditors understand the result.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given an evidence pack with an invalid signature, when verified, then the report names the commit and signer.
- **Verification:** CONF, USE
- **Origin:** FR-PROOF-062

## Evidence packs, redaction and seals

### DSN-DVM-940 — Evidence pack builder
- **Statement:** The DVM MUST build evidence packs (STD-PROOF-050) scoped by a query and temporal coordinates (FR-PROOF-052), including objects, versions, commits, signatures, inclusion and consistency proofs, rule bindings, journals (optional) and anchor receipts, with an `EvidenceManifest`.
- **Rationale:** FR-PROOF-051, FR-PROOF-052.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a pack for "all NAVs of fund F in Q1", when verified offline, then every item verifies.
- **Verification:** CONF
- **Origin:** FR-PROOF-051, FR-PROOF-052, STD-PROOF-050

### DSN-DVM-941 — Reproducible packs
- **Statement:** Building the same pack twice from the same inputs MUST produce byte-identical archives (sorted entries, fixed timestamps from the snapshot).
- **Rationale:** FR-PROOF-054.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given two builds, when hashed, then the hashes are equal.
- **Verification:** CONF
- **Origin:** FR-PROOF-054

### DSN-DVM-942 — Provable redaction
- **Statement:** Redaction (STD-PROOF-051) MUST replace field values with their commitments so that the version ID still verifies, and MUST record the redaction reason in the manifest.
- **Rationale:** FR-PROOF-053.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a pack with redacted personal fields, when verified, then verification succeeds, and redacted values are absent.
- **Verification:** CONF, SEC
- **Origin:** FR-PROOF-053, STD-PROOF-051

### DSN-DVM-943 — Document seals
- **Statement:** A seal (STD-PROOF-055) MUST bind a rendered document's hash to the source commit, the view version and the signer, stored as a `Seal` object.
- **Rationale:** Rendered outputs are provable.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given a sealed PDF, when verified, then the seal links it to the commit, and a modified PDF fails.
- **Verification:** CONF
- **Origin:** STD-PROOF-055

### DSN-DVM-944 — Export logging
- **Statement:** Every evidence pack or export MUST be logged with the requester, scope and manifest hash (FR-AUD-052).
- **Rationale:** FR-AUD-052.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given an export, when the audit log is read, then the record includes the manifest hash.
- **Verification:** CONF
- **Origin:** FR-AUD-052

## Export, import and sync structures

### DSN-DVM-950 — Portable archive export
- **Statement:** The DVM MUST export a VAE, branch or history range in the portable archive format (STD-PROOF-070): content-addressed chunks, tree nodes, commits and signatures, in a streaming form that needs memory independent of the archive size.
- **Rationale:** NR-PORT-003, NR-PORT-004, NR-COMPAT-003.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a 100 GB VAE, when exported, then memory stays under 512 MB, and the archive verifies.
- **Verification:** BENCH, CONF
- **Origin:** NR-PORT-003, NR-PORT-004, NR-COMPAT-003, STD-PROOF-070
- **Phase note:** PH-1 step 6b (DEC-059) exports a whole VAE as one JSON document of format `ubos.archive.v1` from `ubos-version` (every commit with its state, versions, entries, chunks, branches, tags, DID documents, signed manifest); the streaming form and the branch and history-range selections follow with the host of step 7 and the benchmark of acceptance 1 on ENV-REF-SERVER.

### DSN-DVM-951 — Import with verification
- **Statement:** Import MUST verify every item (DSN-DVM-936), rebuild kernel indexes and projections, and produce the same roots. Re-import of the same archive MUST be idempotent.
- **Rationale:** Any conformant kernel can hold the data (NR-DET-002).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given an archive exported from one PostgreSQL VAE, when imported into another PostgreSQL VAE, then all roots are equal, and queries give identical results.
  2. Given the SQLite adapter (PH-4, DEC-024), when the same archive is imported into SQLite, then roots and query results are equal.
- **Verification:** CONF
- **Origin:** NR-PORT-003, NR-DET-002, FR-MIG-013
- **Phase note:** PH-1 step 6b (DEC-059): the import verifies the archive with the storage-free checks of `ubos-proof`, loads the commits in parent order through the kernel flush (indexes rebuilt from the write sets, DEC-053) and recomputes every state root and head root; it imports into an empty store, so re-import is rejected rather than idempotent until the sync structures of PH-4 (acceptance 1 is the round trip of part 6c, EXIT-1-12).

### DSN-DVM-952 — Sync data structures
- **Statement:** The DVM MUST provide the data structures of the sync protocol (STD-SYNC-001…004): scope evaluation to a set of keys, have/want computation by tree comparison, packing of missing nodes and commits, and integration of device branches by merge.
- **Rationale:** FR-SYNC-011, FR-SYNC-021, FR-SYNC-022.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** DVM
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given a device offline for a week with 100 local changes and 10,000 server changes, when synced, then transferred bytes meet NR-PERF-018, and both sides converge.
- **Verification:** SIM, BENCH
- **Origin:** FR-SYNC-011, FR-SYNC-021, FR-SYNC-022, STD-SYNC-001, STD-SYNC-002, STD-SYNC-003, STD-SYNC-004, NR-PERF-018
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-DVM-953 — Sync conflict handling
- **Statement:** Integrating device branches MUST use the merge engine; ledger entries MUST be appended with their original valid time (FR-SYNC-032); rejected changes MUST be returned to the device as rejection records (FR-SYNC-033) with attributed resolutions (FR-SYNC-034).
- **Rationale:** Offline work is merged, not overwritten.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** DVM
- **Personas:** PER-FieldWorker
- **Acceptance:**
  1. Given a device change that violates a new server rule, when synced, then the device receives a rejection record with the reason.
- **Verification:** SIM, CONF
- **Origin:** FR-SYNC-031, FR-SYNC-032, FR-SYNC-033, FR-SYNC-034
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-DVM-954 — Scope reduction and wipe
- **Statement:** When a device's scope shrinks (STD-SYNC-005), the DVM on the device MUST delete data outside the new scope, including chunks and index rows, and report completion.
- **Rationale:** FR-SYNC-042.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a scope reduction, when applied, then no out-of-scope record remains in the device store.
- **Verification:** SEC
- **Origin:** FR-SYNC-042, STD-SYNC-005
- **Phase note:** moved to PH-4 by DEC-024 (SQLite, Box, devices and sync).

### DSN-DVM-955 — Shared BPU structures
- **Statement:** The DVM MUST support shared objects replicated between VAEs (STD-SYNC-011): proposals, multi-party signatures and party-appended ledger entries, verified by the rules of the share.
- **Rationale:** FR-SYNC-071…075.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** DVM, FED
- **Personas:** PER-ExternalParty
- **Acceptance:**
  1. Given a shared contract with two parties, when one proposes a change, then it takes effect only with both signatures.
- **Verification:** SIM, CONF
- **Origin:** FR-SYNC-071, FR-SYNC-072, FR-SYNC-073, FR-SYNC-074, FR-SYNC-075, STD-SYNC-011, STD-SYNC-012

### DSN-DVM-956 — Browser build
- **Statement:** The DVM MUST compile to `wasm32` with the SQLite adapter on OPFS, and pass the same conformance suite (FR-SYNC-082). It MUST handle storage eviction by detecting missing data and re-syncing (FR-SYNC-083).
- **Rationale:** FR-SYNC-081…084.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** DVM
- **Personas:** PER-PersonalUser
- **Acceptance:**
  1. Given the conformance suite, when run in the browser build, then 100% pass.
  2. Given local reads in the browser, when benchmarked, then they meet NR-PERF-024.
- **Verification:** CONF, BENCH
- **Origin:** FR-SYNC-081, FR-SYNC-082, FR-SYNC-083, FR-SYNC-084, NR-PERF-024
