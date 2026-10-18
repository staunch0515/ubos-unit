---
id: UBS-REQ-05-14
title: Functional Requirements — ANL (Projections, Reporting, Analytics Exit) and PROOF (Integrity and Evidence)
status: complete
phase: PH-1
depends_on: [UBS-REQ-05, UBS-REQ-05-02]
---

# Functional Requirements — ANL and PROOF

## CAP-ANL-01 — Declared projections

### FR-ANL-011 — Typed projection tables
- **Statement:** Classes and Buks MUST be able to declare projections: typed tables with columns, source classes, filter, grouping and indexes. They are maintained from commits (DEC-011).
- **Rationale:** Fast reads and reporting without making business types into DDL by default.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper, PER-DataAnalyst
- **Acceptance:**
  1. Given a projection of open orders by fund, when orders change, then the projection reflects them per its mode.
- **Verification:** CONF
- **Origin:** DEC-011, IMP-09

### FR-ANL-012 — Maintenance modes
- **Statement:** Projections MUST declare their mode: `sync` (updated in the commit's storage transaction) or `async` (updated after commit with a watermark). Invariants MAY reference only `sync` projections.
- **Rationale:** Trade-off between write latency and read freshness.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given an invariant over an `async` projection, when declared, then it fails with `ANL.ASYNC_IN_INVARIANT`.
- **Verification:** CONF
- **Origin:** IMP-09

### FR-ANL-013 — Projections are non-authoritative
- **Statement:** Projections MUST be labelled non-authoritative, MUST NOT be writable except by their maintainer, and MUST NOT be used as the source for proofs or exports of record.
- **Rationale:** Single source of truth.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a direct write to a projection table through any API, when attempted, then it is refused.
- **Verification:** CONF, SEC
- **Origin:** DEC-011

### FR-ANL-014 — Index declarations
- **Statement:** Projections and classes MUST be able to declare indexes, which the query planner uses (FR-QRY-014).
- **Rationale:** Predictable performance.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given an index on `fund_code`, when filtering by it, then the plan uses the index.
- **Verification:** BENCH
- **Origin:** NEW

## CAP-ANL-02 — Projection rebuild

### FR-ANL-021 — Rebuild from authoritative state
- **Statement:** Any projection MUST be rebuildable from authoritative state for any branch and point in time.
- **Rationale:** Projections can always be discarded (DEC-011).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a dropped projection, when rebuilt, then it equals the previous contents at the same watermark.
- **Verification:** CONF, PROP
- **Origin:** DEC-011
- **Phase note:** PH-2 step 3 (DEC-087): moved from PH-1; its design item DSN-NOD-344 is PH-2. PH-1 rebuilds field indexes only.

### FR-ANL-022 — Verification by checksum
- **Statement:** The platform MUST verify a live projection against a rebuild using order-independent checksums, and MUST report differences.
- **Rationale:** Detect maintenance bugs.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an injected projection corruption, when verified, then the difference is reported with sample keys.
- **Verification:** FAULT
- **Origin:** NEW

### FR-ANL-023 — Online rebuild
- **Statement:** Rebuilds MUST run into a shadow table and switch atomically, with no read downtime.
- **Rationale:** Availability.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a rebuild during reads, when switched, then no read fails.
- **Verification:** FAULT, BENCH
- **Origin:** NEW

## CAP-ANL-03 — Reports

### FR-ANL-031 — Report definitions
- **Statement:** Reports MUST be versioned objects combining a query or projection, parameters (including `asof` and `time`), a layout, and output formats (screen, PDF, XLSX, CSV).
- **Rationale:** Operational and regulatory reporting.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP, DVM
- **Personas:** PER-DataAnalyst, PER-FundOperationsManager
- **Acceptance:**
  1. Given the report "holdings by fund" with `asof`, when run for two dates, then each reflects its date.
- **Verification:** SCN
- **Origin:** NEW

### FR-ANL-032 — Reproducible report runs
- **Statement:** Every report run MUST record parameters and the commit IDs used, so that it can be re-run to identical output.
- **Rationale:** Regulatory reproducibility.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given a stored run, when re-run with its recorded commits, then the output hash matches.
- **Verification:** CONF
- **Origin:** IMP-02

### FR-ANL-033 — Scheduled distribution
- **Statement:** Reports MUST be schedulable, with distribution to users, groups or connectors, under permission checks at run time.
- **Rationale:** Routine reporting.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a monthly schedule, when run, then recipients without access receive nothing.
- **Verification:** SCN, SEC
- **Origin:** NEW

## CAP-ANL-04 — Embedded analytics

### FR-ANL-041 — Single-tenant analytical engine
- **Statement:** Server and Cell editions MUST provide an embedded analytical engine (columnar, DuckDB-class) over exported snapshots or projections of one tenant, for heavier aggregate queries.
- **Rationale:** Analytics without a separate warehouse for smaller customers (IMP-09).
- **Priority:** Should · **Phase:** PH-3 · **Systems:** BRG, NOD
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given 50,000,000 ledger entries, when a monthly aggregation runs, then it completes within the NR-PERF analytics bound.
- **Verification:** BENCH
- **Origin:** IMP-09

### FR-ANL-042 — Read-only SQL for analysts
- **Statement:** Analysts with permission MUST be able to run read-only SQL over analytical datasets, with row filters and masking applied.
- **Rationale:** Familiar tooling.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given a masked column, when queried by SQL, then masked values are returned.
- **Verification:** SEC
- **Origin:** NEW

### FR-ANL-043 — Governed datasets
- **Statement:** Analytical datasets MUST be declared objects with owner, refresh policy, lineage and classification.
- **Rationale:** No shadow data.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-DataAnalyst, PER-ComplianceOfficer
- **Acceptance:**
  1. Given a dataset, when inspected, then its sources and refresh time are shown.
- **Verification:** CONF
- **Origin:** NEW

## CAP-ANL-05 — CDC export

### FR-ANL-051 — Incremental export to Parquet and Iceberg
- **Statement:** The Bridge MUST export changes incrementally to Parquet files or Iceberg tables. Each row MUST carry object ID, class, version, valid-from, valid-to, transaction time, commit ID, process ID and a deleted or ended flag.
- **Rationale:** An analytics exit with full temporal and lineage context (IMP-09).
- **Priority:** Must · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given an Iceberg sink, when 1,000 commits occur, then the table contains exactly the resulting versions with those columns.
- **Verification:** CONF
- **Origin:** IMP-09

### FR-ANL-052 — Exactly-once into tables
- **Statement:** Exports into table formats MUST be idempotent per commit range, so that retries never duplicate rows.
- **Rationale:** Trustworthy analytics.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given a retry after a partial write, when completed, then row counts match the source.
- **Verification:** FAULT
- **Origin:** NEW

### FR-ANL-053 — Schema evolution in exports
- **Statement:** When classes evolve, exports MUST apply lenses to a declared export schema version, or evolve the table schema compatibly, and MUST record the mapping.
- **Rationale:** Stable downstream contracts.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given the split lens of SCN-014, when exported, then both old and new rows appear in the declared schema.
- **Verification:** CONF
- **Origin:** IMP-04

### FR-ANL-054 — Masking and classification in exports
- **Statement:** Exports MUST apply masking by the sink's clearance and MUST exclude or tokenise classified fields as configured.
- **Rationale:** Exports are an output channel.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a sink cleared for `internal`, when exporting persons, then personal fields are absent or tokenised.
- **Verification:** SEC
- **Origin:** NEW

## CAP-ANL-06 — Dashboards

### FR-ANL-061 — Dashboard objects
- **Statement:** Dashboards MUST be versioned objects composed of tiles (metrics, charts, lists) bound to queries, projections or datasets.
- **Rationale:** Live operations views and War Room feeds.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-Executive, PER-FundOperationsManager
- **Acceptance:**
  1. Given a dashboard, when opened, then each tile shows data with its freshness.
- **Verification:** SCN
- **Origin:** NEW

### FR-ANL-062 — Live updates
- **Statement:** Tiles MUST update through subscriptions when their sources change, within the NR-PERF freshness bound.
- **Rationale:** Real-time awareness.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a new exception, when it is committed, then the exception tile updates within the bound.
- **Verification:** BENCH
- **Origin:** NEW

### FR-ANL-063 — Drill-down to records
- **Statement:** Every tile value MUST drill down to its contributing records within permissions.
- **Rationale:** Explainable numbers.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-Executive
- **Acceptance:**
  1. Given a total, when drilled, then the listed records sum to it.
- **Verification:** SCN
- **Origin:** NEW

## CAP-PROOF-01 — Canonical content addressing

### FR-PROOF-011 — Canonical JSON
- **Statement:** Every JSON value that is hashed or signed MUST be canonicalised with RFC 8785 (JCS).
- **Rationale:** Cross-implementation hash stability (EXT-RFC 2.4).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, BPA
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given the JCS vector suite, when canonicalised, then the outputs match byte for byte.
- **Verification:** CONF
- **Origin:** EXT-RFC

### FR-PROOF-012 — Content identifiers
- **Statement:** Object versions, commits and chunks MUST be identified by SHA-256 hashes of their canonical form, prefixed with an algorithm identifier to allow future algorithm changes.
- **Rationale:** Content addressing (IMP-03), with algorithm agility.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, BPA
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a version, when its ID is recomputed from content, then it matches.
- **Verification:** CONF, PROP
- **Origin:** EXT-RFC, IMP-03

### FR-PROOF-013 — Chunking of large content
- **Statement:** Large documents and files MUST be split into content-defined chunks. Unchanged chunks between versions MUST be stored once.
- **Rationale:** Structural sharing (IMP-03).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a 10 MB document with a 1-line change, when committed, then the new storage is bounded by a few chunks.
- **Verification:** BENCH
- **Origin:** IMP-03

### FR-PROOF-014 — Deduplication
- **Statement:** Identical content MUST be stored once per tenant and encryption domain.
- **Rationale:** Storage efficiency.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the same attachment added to 100 objects, when measured, then it is stored once.
- **Verification:** BENCH
- **Origin:** IMP-03

## CAP-PROOF-02 — Signed commits

### FR-PROOF-021 — Author signatures
- **Statement:** Every commit MUST be signed with Ed25519 by the author principal's key over the commit ID.
- **Rationale:** Non-repudiation (EXT-RFC 2.2.1).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer, PER-RegulatorExaminer
- **Acceptance:**
  1. Given any commit, when verified with the author's key valid at that time, then the signature is valid.
- **Verification:** CONF
- **Origin:** EXT-RFC

### FR-PROOF-022 — Node co-signature
- **Statement:** Commits made by kernel services, schedules or server-side processes on behalf of a principal MUST also be co-signed by the node key.
- **Rationale:** Attests which node produced server-side commits.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a scheduled commit, when inspected, then both signatures are present and valid.
- **Verification:** CONF
- **Origin:** NEW

### FR-PROOF-023 — Verification on receipt
- **Statement:** Signatures MUST be verified on every commit received by sync, import or federation. Verification on local reads is available on demand and by periodic background audit.
- **Rationale:** Trust boundaries.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given an imported commit with an invalid signature, when imported, then it is rejected with `PROOF.SIGNATURE_INVALID`.
- **Verification:** CONF, SEC
- **Origin:** EXT-RFC

## CAP-PROOF-03 — Merkle roots and proofs

### FR-PROOF-031 — Root per commit
- **Statement:** Each commit MUST record the Merkle root of its branch's complete state after the commit, computed over a canonical, history-independent tree of object versions (for example a prolly tree or Merkle search tree).
- **Rationale:** One hash proves a whole state (IMP-03).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, BPA
- **Personas:** PER-RegulatorExaminer, PER-KernelEngineer
- **Acceptance:**
  1. Given the same set of object versions reached by different histories, when the roots are computed, then they are equal.
- **Verification:** CONF, PROP
- **Origin:** IMP-03

### FR-PROOF-032 — Inclusion proofs
- **Statement:** The platform MUST produce inclusion proofs showing that an object version is part of a root. The proof size MUST be logarithmic in the state size.
- **Rationale:** Evidence of individual records.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given a version and a root, when a proof is produced and verified offline, then it succeeds. A tampered version fails.
- **Verification:** CONF, PROP
- **Origin:** IMP-03

### FR-PROOF-033 — Consistency proofs
- **Statement:** The platform MUST prove that a later commit extends an earlier commit on a branch's first-parent history, without rewriting.
- **Rationale:** Append-only assurance.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given two anchored commits, when a consistency proof is verified, then it succeeds.
- **Verification:** CONF
- **Origin:** NEW

### FR-PROOF-034 — Tenant root
- **Statement:** The platform MUST compute a tenant root over all VAE branch heads at a point in time, for anchoring.
- **Rationale:** One anchor per tenant per period.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a tenant root, when a branch head changes, then the next tenant root differs.
- **Verification:** CONF
- **Origin:** NEW

## CAP-PROOF-04 — Anchoring

### FR-PROOF-041 — Periodic anchoring
- **Statement:** The Notary MUST anchor tenant roots on a configurable schedule (default daily) to one or more external timestamping services: an RFC 3161 time-stamp authority, and optionally a public ledger as a timestamp only.
- **Rationale:** Independent proof of existence in time.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NTY
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a daily schedule, when a week passes, then 7 anchors exist with receipts.
- **Verification:** CONF, AUDIT
- **Origin:** NEW

### FR-PROOF-042 — Anchor receipts
- **Statement:** Anchor receipts MUST be stored as ledger entries that link the root, the service, the receipt and the time.
- **Rationale:** Evidence chain.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NTY, DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given an anchor, when verified with the service's public data, then the receipt validates.
- **Verification:** CONF
- **Origin:** NEW

### FR-PROOF-043 — Anchoring monitoring
- **Statement:** Failed or delayed anchors MUST raise alerts and MUST be retried. Gaps MUST be reported in compliance reports.
- **Rationale:** Continuous assurance.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NTY
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a TSA outage, when the schedule passes, then an alert is raised, and the retry succeeds later.
- **Verification:** FAULT
- **Origin:** NEW

## CAP-PROOF-05 — Evidence packs

### FR-PROOF-051 — Evidence pack contents
- **Statement:** An evidence pack MUST be a self-contained archive with:
  - the requested objects and versions;
  - the rule and logic versions bound to them;
  - commits with signatures, and inclusion and consistency proofs;
  - anchor receipts and the principals' public keys;
  - a manifest signed by the Notary.
- **Rationale:** SCN-109.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NTY
- **Personas:** PER-RegulatorExaminer, PER-InternalAuditor
- **Acceptance:**
  1. Given SCN-109, when the pack is verified, then all checks pass.
- **Verification:** SCN, AUDIT
- **Origin:** NEW

### FR-PROOF-052 — Scoped by query and temporal coordinates
- **Statement:** Packs MUST be definable by query, relationship expansion, `asof` and `time`.
- **Rationale:** Answer precise questions.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NTY
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given fund X at the SCN-109 coordinates, when packed, then exactly the specified state is included.
- **Verification:** SCN
- **Origin:** NEW

### FR-PROOF-053 — Provable redaction
- **Statement:** Redacted fields in packs MUST be replaced by their hash commitments (salted per field), so that the proofs still verify and disclosure is minimal.
- **Rationale:** Privacy with integrity (SCN-109 A1).
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NTY
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given redacted personal fields, when verified, then the proofs pass, and the values are not recoverable.
- **Verification:** SEC, CONF
- **Origin:** NEW

### FR-PROOF-054 — Reproducible packs
- **Statement:** Pack generation MUST be deterministic for the same parameters, and pack creation MUST be logged (FR-AUD-052).
- **Rationale:** Evidence integrity.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NTY
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given the same parameters twice, when packed, then the content hashes are equal.
- **Verification:** CONF
- **Origin:** NEW

## CAP-PROOF-06 — Independent verifier

### FR-PROOF-061 — Open verifier
- **Statement:** An open-source verifier (CLI, library and static web page) MUST verify signatures, content hashes, Merkle proofs, anchor receipts, document seals and evidence packs without vendor services.
- **Rationale:** Trust through mathematics, not authority (EXT-TRI).
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NTY
- **Personas:** PER-RegulatorExaminer, PER-ExternalParty
- **Acceptance:**
  1. Given an air-gapped machine, when a pack is verified, then all checks except online anchor lookups complete.
- **Verification:** CONF, AUDIT
- **Origin:** EXT-TRI

### FR-PROOF-062 — Clear verification reports
- **Statement:** The verifier MUST produce a human-readable and a machine-readable report listing each check and its result.
- **Rationale:** Usable by examiners.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NTY
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given a tampered pack, when verified, then the report pinpoints the failing item.
- **Verification:** CONF, USE
- **Origin:** NEW

### FR-PROOF-063 — Published specifications
- **Statement:** The formats verified by the verifier MUST be specified in the BPA standard, so that third parties can build their own verifiers.
- **Rationale:** No dependence on one tool.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** BPA
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given the specification, when a clean-room verifier is built, then it passes the verifier vectors.
- **Verification:** CONF
- **Origin:** NEW

## CAP-PROOF-07 — Encrypted fields and crypto-shredding

### FR-PROOF-071 — Per-subject encryption of personal fields
- **Statement:** Fields classified `personal` or `sensitive_personal` MUST be stored encrypted with a data key specific to the data subject (a person or other subject key declared by the class).
- **Rationale:** Erasure without rewriting history (DEC-003).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given two persons, when their emails are inspected at storage level, then they are encrypted with different keys.
- **Verification:** SEC, INSP
- **Origin:** NEW

### FR-PROOF-072 — Proofs over ciphertext
- **Statement:** Hashes and Merkle trees MUST cover the ciphertext (with a per-field commitment), so that destroying keys does not change any hash.
- **Rationale:** Proofs survive erasure (SCN-016).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given an erased subject, when the affected commits are verified, then all verify.
- **Verification:** CONF, PROP
- **Origin:** NEW

### FR-PROOF-073 — Erasure by key destruction
- **Statement:** An erasure request MUST destroy the subject key in all key stores, commit an erasure record, and cause the affected fields to read as `erased`.
- **Rationale:** GDPR Art. 17 and CCPA deletion.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given SCN-016, when executed, then the outcomes match.
- **Verification:** SCN, AUDIT
- **Origin:** NEW

### FR-PROOF-074 — Key backups honour erasure
- **Statement:** Key backups MUST expire destroyed keys within a documented window, so that restored backups cannot resurrect erased data after the window.
- **Rationale:** Complete erasure.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a restore from before the erasure but after the window, when read, then the fields are `erased`.
- **Verification:** SCN, AUDIT
- **Origin:** NEW

### FR-PROOF-075 — Erasure respects holds and retention
- **Statement:** Erasure MUST be refused while a legal hold or a mandatory retention covers the data, with a recorded reason.
- **Rationale:** Conflicting obligations are resolved in favour of the law that prevails, and the decision is recorded.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given SCN-016 A1, when executed, then the refusal is recorded.
- **Verification:** SCN
- **Origin:** NEW
