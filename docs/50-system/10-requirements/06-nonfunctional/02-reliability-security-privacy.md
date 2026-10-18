---
id: UBS-REQ-06-02
title: Non-Functional Requirements — AVAIL, DUR, DET, SEC, PRIV
status: complete
phase: PH-1
depends_on: [UBS-REQ-06]
---

# Non-Functional Requirements — Availability, Durability, Determinism, Security, Privacy

## AVAIL — Availability

### NR-AVAIL-001 — Cell service level
- **Statement:** The hosted Cell service MUST meet a monthly availability objective.
- **Target:** 99.9% monthly availability of UBTP and Workspace endpoints from PH-4, and 99.95% from PH-5, excluding customer-caused outages. The availability is measured by external synthetic probes every minute.
- **Rationale:** SaaS commitments.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given 3 consecutive months in the pilot Cell, when measured, then each month meets the target.
- **Verification:** BENCH, AUDIT
- **Origin:** NEW

### NR-AVAIL-002 — Server high-availability configuration
- **Statement:** A Server cluster in HA configuration MUST survive the loss of any single node.
- **Target:** Loss of one application node causes no failed requests beyond client retries. Loss of the database primary causes ≤ 60 s of write unavailability with automatic failover.
- **Rationale:** Enterprise availability.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given node kills in the fault matrix, when observed, then the targets hold.
- **Verification:** FAULT
- **Origin:** NEW

### NR-AVAIL-003 — Zero-downtime maintenance
- **Statement:** Planned maintenance (upgrades, configuration, Buk installs, projection rebuilds) MUST not require downtime.
- **Target:** 0 minutes of planned unavailability per quarter.
- **Rationale:** Business continuity.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a quarter of maintenance, when availability is analysed, then no planned downtime is recorded.
- **Verification:** AUDIT, SCN
- **Origin:** NEW

### NR-AVAIL-004 — Tenant migration pause
- **Statement:** Online tenant migration MUST keep the write pause short.
- **Target:** Write pause ≤ 30 s at cut-over for tenants up to 1 TB; reads uninterrupted.
- **Rationale:** FR-TEN-072.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given SCN-404, when measured, then the pause ≤ 30 s.
- **Verification:** BENCH
- **Origin:** IMP-08

### NR-AVAIL-005 — Graceful degradation
- **Statement:** The failure of optional dependencies MUST NOT stop core business operations.
- **Target:** With the model gateway, search index, analytics engine or the Notary unavailable, commits, reads and workflows continue. Affected features return explicit errors.
- **Rationale:** Resilience.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NOD, AGT, NTY
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given each dependency down, when the core scenario suite runs, then it passes.
- **Verification:** FAULT
- **Origin:** NEW

### NR-AVAIL-006 — Read-only mode on storage degradation
- **Statement:** When the authoritative store cannot accept writes but reads work, nodes MUST switch to read-only mode with a clear status.
- **Target:** The switch happens within 10 s of detection. Recovery is automatic after writes resume.
- **Rationale:** Partial service beats no service.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a primary made read-only, when observed, then nodes report read-only within 10 s.
- **Verification:** FAULT
- **Origin:** NEW

## DUR — Durability and recovery

### NR-DUR-001 — No loss of acknowledged commits
- **Statement:** An acknowledged commit MUST survive crashes.
- **Target:** 0 acknowledged commits lost across 10,000 injected crashes (process kill and power-loss simulation) on single-node and HA configurations.
- **Rationale:** Integrity of records.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the crash campaign, when completed, then 0 losses are recorded.
- **Verification:** FAULT, SIM
- **Origin:** NEW

### NR-DUR-002 — Recovery point objective
- **Statement:** Disaster recovery MUST limit data loss.
- **Target:** RPO ≤ 60 s with asynchronous standby; RPO = 0 with synchronous standby (configurable); ENV-REF-SERVER.
- **Rationale:** FR-OPS-061.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given SCN-406, when measured, then the RPO meets the configured mode.
- **Verification:** FAULT
- **Origin:** NEW

### NR-DUR-003 — Recovery time objective
- **Statement:** Disaster recovery MUST restore service quickly.
- **Target:** RTO ≤ 30 min for Server with a standby site; RTO ≤ 15 min for Cell.
- **Rationale:** Business continuity.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the DR drill, when timed, then the RTO is met.
- **Verification:** FAULT, AUDIT
- **Origin:** NEW

### NR-DUR-004 — Backup retention
- **Statement:** Point-in-time recovery MUST be available over a window.
- **Target:** Default 35-day PITR window, configurable up to 365 days; restore verification (SCN-017) passes for sampled points.
- **Rationale:** Operational mistakes are found late.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a restore to day 34, when verified, then the roots match.
- **Verification:** SCN
- **Origin:** NEW

### NR-DUR-005 — Continuous integrity audit
- **Statement:** Stored content MUST be re-verified periodically.
- **Target:** 100% of object versions hash-verified at least once every 30 days by background audit; any mismatch alerts within 5 minutes of detection.
- **Rationale:** Detect silent corruption.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an injected bit flip, when the audit reaches it, then an alert fires.
- **Verification:** FAULT
- **Origin:** NEW

### NR-DUR-006 — Effect delivery after recovery
- **Statement:** Outbox effects MUST be delivered after crashes.
- **Target:** 100% of pending effects released within 5 minutes after node recovery.
- **Rationale:** FR-TXN-052.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given crashes between commit and release, when recovered, then all effects are delivered within 5 minutes.
- **Verification:** FAULT
- **Origin:** NEW

## DET — Determinism

### NR-DET-001 — Replay fidelity
- **Statement:** Recorded processes MUST replay identically.
- **Target:** 100% identical results for a random sample of at least 10,000 production-like processes per release candidate.
- **Rationale:** FR-LOGIC-073.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the sample, when replayed, then there are 0 divergences.
- **Verification:** PROP, SCN
- **Origin:** EXT-BPU

### NR-DET-002 — Cross-implementation equivalence
- **Statement:** Conforming implementations MUST produce identical results.
- **Target:** 100% of determinism vectors identical across platforms for the reference DVM on PostgreSQL (PH-1), and from PH-4 on SQLite (DEC-024) and any certified implementation.
- **Rationale:** FR-LOGIC-074.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** BPA, DVM
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given the vector suite, when run on each, then the results are identical.
- **Verification:** CONF
- **Origin:** EXT-BPU

### NR-DET-003 — History-independent roots
- **Statement:** Merkle roots MUST depend only on state.
- **Target:** Property tests over at least 1,000,000 generated history pairs reaching equal states produce equal roots in 100% of cases.
- **Rationale:** FR-PROOF-031.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the property run, when completed, then there are 0 counter-examples.
- **Verification:** PROP
- **Origin:** IMP-03

### NR-DET-004 — Rendering determinism
- **Statement:** Document renderings MUST be deterministic.
- **Target:** 100% byte-identical outputs across 3 nodes and 2 operating systems for the rendering vector set.
- **Rationale:** FR-OFFICE-013.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given the rendering vectors, when rendered on each platform, then the hashes match.
- **Verification:** CONF
- **Origin:** NEW

## SEC — Security

### NR-SEC-001 — Penetration test outcome
- **Statement:** Releases MUST pass independent penetration testing.
- **Target:** 0 open critical or high findings at each phase exit from PH-2; medium findings remediated within 90 days.
- **Rationale:** Enterprise and regulated customers.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, WSP, AGT
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the pen-test report, when reviewed, then no critical or high findings are open.
- **Verification:** SEC, AUDIT
- **Origin:** NEW

### NR-SEC-002 — Sandbox containment
- **Statement:** The logic sandbox MUST contain all escape attempts.
- **Target:** 100% of the sandbox-escape suite blocked (at least 300 cases, extended with every discovered technique) for L2 in PH-1 and for L3 in PH-3.
- **Rationale:** FR-LOGIC-014.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the suite, when run, then all cases are blocked.
- **Verification:** SEC
- **Origin:** NEW

### NR-SEC-003 — Tenant isolation
- **Statement:** Cross-tenant isolation MUST be absolute.
- **Target:** 100% pass of the isolation suite (FR-TEN-023) on every release; 0 cross-tenant leakage incidents.
- **Rationale:** Trust.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the suite, when run, then it passes 100%.
- **Verification:** SEC
- **Origin:** NEW

### NR-SEC-004 — Cryptography baseline
- **Statement:** Transport and storage MUST use strong, standard cryptography.
- **Target:** TLS 1.3 only for external and node-to-node traffic; AES-256-GCM or ChaCha20-Poly1305 at rest; Ed25519 signatures; SHA-256 hashing; FIPS 140-3 validated modules available as a Cell and Server option.
- **Rationale:** Baseline assurance.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a configuration scan, when run, then no weaker protocol or cipher is enabled.
- **Verification:** SEC, INSP
- **Origin:** EXT-RFC
- **Phase note:** PH-2 step 11 (DEC-087): moved from PH-1; PH-1 provides ChaCha20-Poly1305 at rest, Ed25519 signatures and SHA-256 content identifiers; TLS 1.3 for external and node-to-node traffic comes with the HA Server and is required before any deployment beyond the development workstation.

### NR-SEC-005 — Revocation propagation
- **Statement:** Access revocations MUST take effect quickly everywhere.
- **Target:** Session or token revocation effective on all nodes within 60 s; SCIM deactivation fully effective within 5 minutes.
- **Rationale:** FR-IAM-072, FR-IAM-102.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given revocations under load, when probed, then no success occurs after the bounds.
- **Verification:** SEC, BENCH
- **Origin:** NEW

### NR-SEC-006 — Key custody
- **Statement:** Server-held private keys MUST be protected by hardware-backed or managed key services.
- **Target:** 100% of node and platform signing keys in HSM or KMS for Cell; Server supports KMS or HSM integration and documents software-key risk.
- **Rationale:** FR-IAM-034.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a Cell configuration audit, when inspected, then all keys are hardware- or KMS-backed.
- **Verification:** AUDIT
- **Origin:** NEW

### NR-SEC-007 — Supply-chain integrity
- **Statement:** Build and distribution MUST be verifiable.
- **Target:** SBOM for every artifact; all artifacts signed; SLSA Build Level 3 provenance by PH-4 (Level 2 by PH-2).
- **Rationale:** Enterprise procurement and security.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, FRG, SDK
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a release, when checked, then the SBOM, signatures and provenance verify.
- **Verification:** AUDIT, INSP
- **Origin:** NEW

### NR-SEC-008 — Vulnerability remediation
- **Statement:** Known vulnerabilities MUST be fixed within set times.
- **Target:** Critical ≤ 7 days, high ≤ 30 days, medium ≤ 90 days from triage, for supported versions.
- **Rationale:** Security hygiene.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the vulnerability tracker, when audited, then 100% of closed items meet the SLAs.
- **Verification:** AUDIT
- **Origin:** NEW

### NR-SEC-009 — No secrets in logs
- **Statement:** Secrets and credentials MUST never appear in logs, traces, errors or diagnostics.
- **Target:** 0 matches from automated secret scanning over the log and trace corpus of the full scenario suite.
- **Rationale:** FR-IAM-091.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the scan, when run, then there are 0 matches.
- **Verification:** SEC
- **Origin:** NEW

### NR-SEC-010 — Security event coverage
- **Statement:** All defined security event types MUST be logged.
- **Target:** 100% of the event types in FR-AUD-012 produced by the security scenario suite are present in the security log.
- **Rationale:** Detection capability.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the suite, when the log is checked, then every expected event is present.
- **Verification:** SEC, CONF
- **Origin:** NEW

### NR-SEC-011 — Strong authentication for privileged roles
- **Statement:** Administrative and approver roles above policy thresholds MUST use phishing-resistant multi-factor authentication.
- **Target:** 100% of tenant-admin, platform-operator and security-officer sessions use passkeys or equivalent from PH-2.
- **Rationale:** Privileged-access protection.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the authentication logs, when audited, then no privileged session used a weaker factor.
- **Verification:** AUDIT
- **Origin:** NEW

## PRIV — Privacy

### NR-PRIV-001 — Erasure timeliness
- **Statement:** Erasure requests MUST complete promptly.
- **Target:** Technical completion ≤ 24 h after approval; full effect including key backups ≤ 35 days; request-to-response cycle supports the 30-day GDPR and 45-day CCPA deadlines.
- **Rationale:** GDPR Art. 17, CCPA.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given erasure requests, when processed, then the timings meet the targets.
- **Verification:** SCN, AUDIT
- **Origin:** NEW

### NR-PRIV-002 — Subject access and portability
- **Statement:** Personal data of a subject MUST be exportable.
- **Target:** A subject export (all objects and versions linked to the subject, in JSON) produced ≤ 1 h after request approval.
- **Rationale:** GDPR Art. 15 and 20, CCPA right to know.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a subject with 1,000 linked objects, when exported, then the export is complete within 1 h.
- **Verification:** SCN
- **Origin:** NEW

### NR-PRIV-003 — Classification coverage
- **Statement:** Every field MUST have an effective classification.
- **Target:** 100% of fields in verified Buks have a declared or inherited classification; Forge blocks verification otherwise.
- **Rationale:** FR-MODEL-047.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given the first-party Buks (Basic Finance from PH-2), when checked, then 100% coverage.
- **Verification:** INSP, CONF
- **Origin:** NEW

### NR-PRIV-004 — Residency enforcement
- **Statement:** Residency constraints MUST never be violated.
- **Target:** 0 storage, processing or model calls outside permitted regions in residency tests and in production audits.
- **Rationale:** FR-TEN-073.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL, NOD, AGT
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given residency tests, when run, then there are 0 violations.
- **Verification:** SEC, AUDIT
- **Origin:** NEW

### NR-PRIV-005 — Personal-data read logging
- **Statement:** Reads of personal data MUST be logged.
- **Target:** 100% of reads of `personal` and `sensitive_personal` fields through any channel are logged (FR-AUD-051).
- **Rationale:** Accountability.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given the read suite across channels, when checked, then every read is logged.
- **Verification:** SEC
- **Origin:** NEW

### NR-PRIV-006 — Privacy by default
- **Statement:** New classes and features MUST default to the most private settings.
- **Target:** Default classification `internal`; personal fields are masked for agents and exports by default; read logging is on for personal data by default.
- **Rationale:** GDPR Art. 25.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a new class, when inspected, then the defaults are as stated.
- **Verification:** INSP
- **Origin:** NEW
- **Phase note:** PH-1 step 19b (DEC-086): PH-1 meets the default classification `internal` and the class default; masking personal fields for agents and exports by default follows DSN-DVM-821 and read logging of personal data by default follows DSN-DVM-823 in PH-2.
