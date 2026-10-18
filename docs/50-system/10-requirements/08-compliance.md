---
id: UBS-REQ-08
title: Compliance Requirements
status: complete
phase: ALL
depends_on: [UBS-REQ-05, UBS-REQ-06, DEC-003]
---

# Compliance Requirements

This catalogue translates regulations and frameworks into platform obligations. It follows
DEC-003: US first, GDPR designed in.

- The **Control reference** field cites the regulation or control.
- UBOS provides *capabilities* that let customers comply. Customers remain responsible for
  their own compliance programmes.
- Items marked `AUDIT` require third-party attestation.

> Informative note: the control references summarise the regulations for requirements
> purposes. Legal counsel MUST confirm interpretations before customer-facing claims are
> made.

## SOC2 — SOC 2 Trust Services Criteria

### CR-SOC2-001 — SOC 2 Type II attestation for hosted services
- **Statement:** The hosted Cell service MUST obtain a SOC 2 Type I report by the end of PH-4 and a SOC 2 Type II report (Security, Availability, Confidentiality, Processing Integrity) within 12 months after that.
- **Control reference:** AICPA TSC 2017 (revised points of focus 2022), CC1–CC9, A1, C1, PI1
- **Rationale:** Enterprise procurement prerequisite.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the audit period, when completed, then an unqualified report is issued.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-SOC2-002 — Logical access controls
- **Statement:** The platform MUST provide least-privilege access, provisioning, deprovisioning and periodic access reviews for customer and operator access.
- **Control reference:** TSC CC6.1, CC6.2, CC6.3
- **Rationale:** Core access controls.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NOD, CTL, DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given FR-IAM-041, FR-IAM-071, FR-IAM-072 and FR-IAM-082, when tested, then the control evidence is producible.
- **Verification:** AUDIT, SEC
- **Origin:** DEC-003

### CR-SOC2-003 — Encryption in transit and at rest
- **Statement:** All data in transit MUST be encrypted, and all data at rest MUST be encrypted with managed keys.
- **Control reference:** TSC CC6.1, CC6.7
- **Rationale:** Confidentiality.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given NR-SEC-004, when verified, then the control is met.
- **Verification:** AUDIT, SEC
- **Origin:** DEC-003
- **Phase note:** PH-2 step 11 (DEC-087): moved from PH-1; PH-1 encrypts at rest (DSN-DVM-002, DEC-048) and seals backups; TLS for data in transit comes with the HA Server and is required before any deployment beyond the development workstation.

### CR-SOC2-004 — Malware and vulnerability management
- **Statement:** Uploaded content MUST be scanned for malware, and vulnerabilities MUST be managed within SLAs.
- **Control reference:** TSC CC6.8, CC7.1
- **Rationale:** Threat protection.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given FR-FILE-021 and NR-SEC-008, when evidenced, then the control is met.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-SOC2-005 — Monitoring and incident response
- **Statement:** Security events MUST be monitored, alerted and handled under a documented incident-response process with post-incident reviews.
- **Control reference:** TSC CC7.2, CC7.3, CC7.4, CC7.5
- **Rationale:** Detection and response.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NOD, CTL
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a tabletop exercise, when run, then the process and evidence meet the criteria.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-SOC2-006 — Change management
- **Statement:** Changes to platform software and infrastructure MUST be authorised, tested, approved and traceable. Customer business changes MUST be traceable through change sets.
- **Control reference:** TSC CC8.1
- **Rationale:** Integrity of change.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, CTL, DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a sample of releases and change sets, when audited, then approvals and tests are evidenced.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-SOC2-007 — Availability, backup and recovery
- **Statement:** Capacity, backups, recovery testing and availability monitoring MUST meet the NR-AVAIL and NR-DUR targets.
- **Control reference:** TSC A1.1, A1.2, A1.3
- **Rationale:** Availability criteria.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given the DR drill reports and availability data, when audited, then the criteria are met.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-SOC2-008 — Confidential information handling
- **Statement:** Confidential information MUST be identified by classification, protected, retained and disposed of according to policy.
- **Control reference:** TSC C1.1, C1.2
- **Rationale:** Confidentiality criteria.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given FR-MODEL-047, FR-IAM-061 and FR-AUD-072, when evidenced, then the criteria are met.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-SOC2-009 — Processing integrity
- **Statement:** Processing MUST be complete, valid, accurate, timely and authorised, with inputs, processing and outputs traceable.
- **Control reference:** TSC PI1.1–PI1.5
- **Rationale:** The core value of the platform.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given lineage (FR-AUD-021), validation (CAP-RULE-03) and replay (FR-LOGIC-073), when audited, then the evidence satisfies the criteria.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-SOC2-010 — Vendor and subprocessor management
- **Statement:** Subprocessors (cloud providers, model providers, TSAs) MUST be inventoried, assessed and disclosed to customers.
- **Control reference:** TSC CC9.2
- **Rationale:** Supply-chain risk.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL, AGT
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the subprocessor list, when reviewed, then each has an assessment on file.
- **Verification:** AUDIT
- **Origin:** DEC-003

## SEC — US Securities and Exchange Commission recordkeeping

### CR-SEC-001 — Non-rewriteable, non-erasable or audit-trail storage
- **Statement:** Records designated as regulatory records MUST be preserved either in non-rewriteable, non-erasable (WORM) form, or with a complete time-stamped audit trail that allows the original record to be recreated after any modification or deletion.
- **Control reference:** SEC Rule 17a-4(f)(2) (as amended 2022); Investment Advisers Act Rule 204-2(g); Investment Company Act Rule 31a-2(f)
- **Rationale:** Broker-dealer, adviser and fund recordkeeping (first vertical).
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer, PER-RegulatorExaminer
- **Acceptance:**
  1. Given FR-VER-012, FR-AUD-073 and the audit-trail semantics of commits, when examined by an assessor, then both compliance paths are demonstrably supported.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-SEC-002 — Verification of recording and time-dating
- **Statement:** The platform MUST verify the completeness and accuracy of each recording and MUST time-date and serialise records.
- **Control reference:** SEC Rule 17a-4(f)(2)(ii)–(iii) (verification, serialization and time-date requirements)
- **Rationale:** Accuracy of stored records.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given FR-TIME-011, FR-PROOF-012 and FR-LEDG-041, when examined, then recording verification and serialisation are evidenced.
- **Verification:** AUDIT, CONF
- **Origin:** DEC-003

### CR-SEC-003 — Prompt production in readable form
- **Statement:** The platform MUST produce requested records promptly, in human-readable and reasonably usable electronic form, including indexes to locate them.
- **Control reference:** SEC Rule 17a-4(f)(3)(i)–(iii) (download and transfer, index requirements); Rule 204-2(g)
- **Rationale:** Examination readiness (SCN-109).
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NTY, NOD
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given an examination request, when an evidence pack and a readable export are produced, then they are delivered within the deadline defined by the customer's policy (target 24 h).
- **Verification:** SCN, AUDIT
- **Origin:** DEC-003

### CR-SEC-004 — Retention periods
- **Statement:** Retention schedules MUST support the SEC periods (for example 3 and 6 years, with the first 2 years easily accessible) per record class. They MUST NOT allow disposal before the period ends.
- **Control reference:** SEC Rule 17a-4(a), (b); Rule 204-2(e); Rule 31a-2(a)
- **Rationale:** Mandatory retention.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a record class with 6-year retention, when disposal is attempted earlier, then it is refused.
- **Verification:** CONF, AUDIT
- **Origin:** DEC-003

### CR-SEC-005 — Third-party undertaking support
- **Statement:** For hosted services, the platform operator MUST be able to act as, or support, the designated third party (or designated executive officer arrangement), including independent access to records for regulators.
- **Control reference:** SEC Rule 17a-4(f)(3)(iii) (as amended: designated executive officer or designated third party)
- **Rationale:** Regulatory access independent of the firm.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** NTY, CTL
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given the undertaking process, when reviewed by counsel, then the platform supports it.
- **Verification:** AUDIT, INSP
- **Origin:** DEC-003

### CR-SEC-006 — Books and records for fund operations
- **Statement:** The fund-operations Buk MUST maintain the records required for registered funds and advisers that fall in its scope (journals, ledgers, NAV computations, shareholder records, trade records) with bitemporal corrections and lineage.
- **Control reference:** Investment Company Act Rule 31a-1(b); Advisers Act Rule 204-2(a)
- **Rationale:** First-vertical fitness.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-ComplianceOfficer
- **Acceptance:**
  1. Given the record map of the Buk against the rules, when reviewed by a compliance expert, then every in-scope record type is mapped.
- **Verification:** INSP, PILOT
- **Origin:** DEC-004
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### CR-SEC-007 — Electronic communications capture (integration)
- **Statement:** Action Messages and object threads that constitute business communications MUST be retainable as records under the same rules.
- **Control reference:** SEC Rule 17a-4(b)(4); FINRA Rule 4511
- **Rationale:** Communications recordkeeping.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** WSP, DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given threads designated as records, when retention is applied, then they follow the record class policy.
- **Verification:** CONF
- **Origin:** DEC-003

## FINRA — FINRA rules

### CR-FINRA-001 — Preservation format
- **Statement:** Records for broker-dealer customers MUST be preservable in a format and media that comply with SEC Rule 17a-4.
- **Control reference:** FINRA Rule 4511(c)
- **Rationale:** FINRA members.
- **Priority:** Should · **Phase:** PH-5 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given CR-SEC-001 compliance, when mapped, then FINRA 4511(c) is covered.
- **Verification:** INSP
- **Origin:** DEC-003
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### CR-FINRA-002 — Default retention
- **Statement:** Record classes without a specified period MUST default to at least 6 years in FINRA-profile tenants.
- **Control reference:** FINRA Rule 4511(b)
- **Rationale:** Default retention.
- **Priority:** Should · **Phase:** PH-5 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given the FINRA profile, when a record class without a period is created, then the retention defaults to 6 years.
- **Verification:** CONF
- **Origin:** DEC-003
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### CR-FINRA-003 — Supervisory review evidence
- **Statement:** Approvals, reviews and supervisory actions MUST be evidenced with reviewer, time and the reviewed content hash.
- **Control reference:** FINRA Rule 3110 (supervision)
- **Rationale:** Supervisory evidence.
- **Priority:** Should · **Phase:** PH-5 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given FR-FLOW-044, when examined, then the evidence elements are present.
- **Verification:** INSP
- **Origin:** DEC-003
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

## SOX — Sarbanes-Oxley IT general controls

### CR-SOX-001 — Access to financially significant data
- **Statement:** Access to financially significant classes MUST follow least privilege, with quarterly access reviews and evidence.
- **Control reference:** SOX §404; COSO 2013 Principle 11; ITGC access controls
- **Rationale:** Listed-company customers.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, NOD
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given FR-IAM-082, when an audit samples reviews, then the evidence is complete.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-SOX-002 — Segregation of duties
- **Statement:** Incompatible duties MUST be enforced and conflicts reported.
- **Control reference:** SOX §404; ITGC segregation of duties
- **Rationale:** Fraud prevention.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given FR-FLOW-043 and FR-AUD-041, when tested, then the SoD enforcement and reports are evidenced.
- **Verification:** AUDIT, CONF
- **Origin:** DEC-003

### CR-SOX-003 — Change management for financial logic
- **Statement:** Changes to financial rules and logic MUST pass review, testing and approval before they are effective, with a complete trail.
- **Control reference:** SOX §404; ITGC program change
- **Rationale:** Integrity of financial processing.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given change sets to fee logic, when sampled, then review, tests and approvals are present.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-SOX-004 — Computer operations
- **Statement:** Jobs, schedules and interfaces affecting financial data MUST be monitored, with failures handled and evidenced.
- **Control reference:** SOX §404; ITGC computer operations
- **Rationale:** Completeness of processing.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a failed NAV job, when handled, then the retry or resolution is evidenced.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-SOX-005 — Period close controls
- **Statement:** Period close and reopen MUST be controlled and evidenced (FR-LEDG-061 … FR-LEDG-063).
- **Control reference:** SOX §404; financial close controls
- **Rationale:** Reliable reporting.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a close and a governed reopen, when audited, then the evidence is complete.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-SOX-006 — Report reproducibility
- **Statement:** Financial reports MUST be reproducible from recorded parameters and commits.
- **Control reference:** SOX §302, §404 (reliability of reported information)
- **Rationale:** Management certification support.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given FR-ANL-032, when re-run, then the output is identical.
- **Verification:** CONF
- **Origin:** DEC-003

## HIPAA — HIPAA Security Rule (for a later healthcare vertical)

### CR-HIPAA-001 — Access control
- **Statement:** The platform MUST support unique user identification, emergency access procedures, automatic logoff and encryption of ePHI.
- **Control reference:** 45 CFR 164.312(a)(1), (a)(2)(i)–(iv)
- **Rationale:** Healthcare readiness.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** NOD, DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the HIPAA control mapping, when reviewed, then every implementation specification is mapped to a feature.
- **Verification:** INSP, AUDIT
- **Origin:** DEC-003

### CR-HIPAA-002 — Audit controls
- **Statement:** The platform MUST record and examine activity in systems that contain ePHI, including reads.
- **Control reference:** 45 CFR 164.312(b)
- **Rationale:** Accountability.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given FR-AUD-051, when mapped, then the requirement is covered.
- **Verification:** INSP
- **Origin:** DEC-003

### CR-HIPAA-003 — Integrity
- **Statement:** The platform MUST protect ePHI from improper alteration or destruction, and MUST be able to show that it was not altered.
- **Control reference:** 45 CFR 164.312(c)(1), (c)(2)
- **Rationale:** Integrity.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given the proofs (CAP-PROOF-03), when mapped, then the requirement is covered.
- **Verification:** INSP
- **Origin:** DEC-003

### CR-HIPAA-004 — Authentication and transmission security
- **Statement:** The platform MUST verify person or entity identity and protect ePHI in transmission.
- **Control reference:** 45 CFR 164.312(d), (e)(1)
- **Rationale:** Security.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given FR-IAM-021 and NR-SEC-004, when mapped, then the requirement is covered.
- **Verification:** INSP
- **Origin:** DEC-003

### CR-HIPAA-005 — Business associate agreements
- **Statement:** The hosted service MUST be able to enter business associate agreements, with subprocessors flowing down the obligations.
- **Control reference:** 45 CFR 164.308(b), 164.314(a)
- **Rationale:** Contractual prerequisite.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** CTL
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given counsel review, when complete, then a BAA template and the subprocessor terms exist.
- **Verification:** INSP
- **Origin:** DEC-003

## GDPR — EU General Data Protection Regulation

### CR-GDPR-001 — Data protection by design and by default
- **Statement:** Privacy MUST be built in: classification, minimisation, masking by default and purpose limitation for AI processing.
- **Control reference:** GDPR Art. 25
- **Rationale:** Design obligation.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given NR-PRIV-006, when verified, then the defaults are met.
- **Verification:** INSP
- **Origin:** DEC-003

### CR-GDPR-002 — Right of access and portability
- **Statement:** Controllers MUST be able to fulfil access and portability requests.
- **Control reference:** GDPR Art. 15, Art. 20
- **Rationale:** Data-subject rights.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given NR-PRIV-002, when tested, then it passes.
- **Verification:** SCN
- **Origin:** DEC-003

### CR-GDPR-003 — Right to rectification
- **Statement:** Personal data MUST be correctable, with the correction recorded and with bitemporal effect where appropriate.
- **Control reference:** GDPR Art. 16
- **Rationale:** Data-subject rights.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a correction, when committed, then the current value is corrected, and history records the correction.
- **Verification:** CONF
- **Origin:** DEC-003

### CR-GDPR-004 — Right to erasure with immutable history
- **Statement:** Erasure MUST be achievable despite immutable history, by crypto-shredding, subject to legal holds and retention exceptions.
- **Control reference:** GDPR Art. 17(1), 17(3)(b), (e)
- **Rationale:** The principle-level conflict is resolved by design (DEC-003).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given SCN-016, when executed, then the outcomes match, and a DPA-ready explanation of the technique exists.
- **Verification:** SCN, AUDIT
- **Origin:** DEC-003

### CR-GDPR-005 — Records of processing
- **Statement:** The platform MUST provide the information controllers need for records of processing activities: classes with personal data, purposes (declared per class), recipients (connectors, agents), transfers and retention.
- **Control reference:** GDPR Art. 30
- **Rationale:** Accountability.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a VAE, when the processing report is generated, then every personal-data class is listed with its purpose, recipients and retention.
- **Verification:** SCN
- **Origin:** DEC-003

### CR-GDPR-006 — Security of processing
- **Statement:** Appropriate technical measures MUST be in place (encryption, integrity, availability, resilience, regular testing).
- **Control reference:** GDPR Art. 32
- **Rationale:** Security obligation.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given NR-SEC and NR-DUR items, when mapped, then Art. 32 measures are covered.
- **Verification:** INSP
- **Origin:** DEC-003

### CR-GDPR-007 — Breach detection and notification support
- **Statement:** The platform MUST support breach detection and provide the information needed for notification within 72 hours: scope, affected subjects and data categories.
- **Control reference:** GDPR Art. 33, Art. 34
- **Rationale:** Breach response.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** NOD, CTL
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a simulated breach of one class, when investigated, then the affected subjects are listed within 24 hours.
- **Verification:** SCN
- **Origin:** DEC-003

### CR-GDPR-008 — International transfer controls
- **Statement:** Transfers of personal data outside permitted regions MUST be controllable per tenant: residency, model-provider regions and subprocessor locations.
- **Control reference:** GDPR Chapter V (Art. 44–49)
- **Rationale:** Transfer restrictions.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** CTL, AGT
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given FR-TEN-073, when tested, then no transfers occur outside the configuration.
- **Verification:** SEC, AUDIT
- **Origin:** DEC-003

## CCPA — California Consumer Privacy Act (as amended by CPRA)

### CR-CCPA-001 — Right to know and access
- **Statement:** Businesses MUST be able to disclose categories and specific pieces of personal information collected about a consumer.
- **Control reference:** Cal. Civ. Code §1798.100, §1798.110
- **Rationale:** US privacy law.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given NR-PRIV-002, when applied, then the disclosure is producible.
- **Verification:** SCN
- **Origin:** DEC-003

### CR-CCPA-002 — Right to delete and correct
- **Statement:** Deletion and correction requests MUST be supported through the erasure and rectification mechanisms.
- **Control reference:** Cal. Civ. Code §1798.105, §1798.106
- **Rationale:** US privacy law.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given CR-GDPR-003 and CR-GDPR-004 mechanisms, when applied, then CCPA requests are fulfilled.
- **Verification:** SCN
- **Origin:** DEC-003

### CR-CCPA-003 — Limit use of sensitive personal information
- **Statement:** Uses of `sensitive_personal` data (for example AI processing and exports) MUST be restrictable by policy per purpose.
- **Control reference:** Cal. Civ. Code §1798.121
- **Rationale:** Sensitive-data limits.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** DVM, AGT
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a purpose restriction, when an agent requests sensitive data, then it is denied.
- **Verification:** SEC
- **Origin:** DEC-003

### CR-CCPA-004 — No sale or sharing by the platform
- **Statement:** The platform operator MUST NOT sell or share customer personal information, and model providers MUST be contractually barred from training on it.
- **Control reference:** Cal. Civ. Code §1798.120, §1798.140 (service provider)
- **Rationale:** Service-provider status.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** AGT, CTL
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given the model-provider contracts, when reviewed, then no-training terms are present.
- **Verification:** INSP
- **Origin:** DEC-003

## ESIGN — E-SIGN Act and UETA

### CR-ESIGN-001 — Consumer consent to electronic records
- **Statement:** Where consumers sign, the platform MUST obtain and record affirmative consent after the required disclosures, including the right to paper copies and withdrawal.
- **Control reference:** 15 U.S.C. §7001(c)
- **Rationale:** Enforceability.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** WSP
- **Personas:** PER-ExternalParty
- **Acceptance:**
  1. Given a consumer ceremony, when completed, then the consent record includes the disclosures shown.
- **Verification:** SCN, INSP
- **Origin:** DEC-003
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### CR-ESIGN-002 — Accurate, reproducible retention
- **Statement:** Signed records MUST be retained in a form that accurately reflects them and remains accessible and reproducible for all entitled parties.
- **Control reference:** 15 U.S.C. §7001(d), (e); UETA §12
- **Rationale:** Record retention.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** DVM, WSP
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given FR-OFFICE-013 and FR-SIGN-013, when verified, then the requirement is met.
- **Verification:** CONF
- **Origin:** DEC-003
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### CR-ESIGN-003 — Attribution
- **Statement:** Signatures MUST be attributable to the signer through recorded authentication evidence.
- **Control reference:** UETA §9
- **Rationale:** Attribution.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** WSP
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given a signature, when inspected, then the authentication evidence is present.
- **Verification:** CONF
- **Origin:** DEC-003
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### CR-ESIGN-004 — Intent and association
- **Statement:** The signing process MUST capture the intent to sign and logically associate the signature with the record (the seal).
- **Control reference:** 15 U.S.C. §7006(5); UETA §2(8), §7
- **Rationale:** Validity.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** WSP, DVM
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given FR-SIGN-021, when verified, then intent and association are provable.
- **Verification:** CONF
- **Origin:** DEC-003
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

## WCAG — Accessibility

### CR-WCAG-001 — WCAG 2.2 Level AA
- **Statement:** User interfaces MUST conform to WCAG 2.2 Level AA.
- **Control reference:** W3C WCAG 2.2 (Level A and AA success criteria)
- **Rationale:** Accessibility and procurement.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given NR-ACC-001, when audited, then it passes.
- **Verification:** AUDIT
- **Origin:** DEC-003

### CR-WCAG-002 — Section 508 and ACR
- **Statement:** An Accessibility Conformance Report (VPAT 2.x, Section 508 edition) MUST be published for each major release.
- **Control reference:** 36 CFR Part 1194 (Revised 508 Standards); VPAT 2.x
- **Rationale:** US public-sector and enterprise procurement.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a release, when published, then the ACR is available.
- **Verification:** INSP
- **Origin:** DEC-003

### CR-WCAG-003 — Accessible documents
- **Statement:** Generated documents intended for external parties SHOULD meet PDF/UA.
- **Control reference:** ISO 14289-1 (PDF/UA-1)
- **Rationale:** Accessible records.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-ExternalParty
- **Acceptance:**
  1. Given NR-ACC-004, when verified, then it passes.
- **Verification:** CONF
- **Origin:** DEC-003

## NIST — Control vocabulary (NIST SP 800-53 Rev. 5, moderate baseline)

### CR-NIST-001 — Control mapping
- **Statement:** The platform's security capabilities MUST be mapped to NIST SP 800-53 Rev. 5 moderate-baseline controls, at least AC-2, AC-3, AC-6, AU-2, AU-3, AU-9, AU-10, IA-2, IA-5, SC-8, SC-12, SC-13, SC-28, SI-7, CM-3 and CP-9. The mapping MUST be kept current.
- **Control reference:** NIST SP 800-53 Rev. 5
- **Rationale:** A common control vocabulary for regulated customers and future FedRAMP work.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** NOD, DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the mapping document, when reviewed, then every listed control has implementing features and evidence sources.
- **Verification:** INSP
- **Origin:** DEC-003

### CR-NIST-002 — Non-repudiation
- **Statement:** The platform MUST provide non-repudiation for commits, approvals and signatures.
- **Control reference:** NIST SP 800-53 AU-10
- **Rationale:** Evidence value.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given FR-PROOF-021 and FR-FLOW-044, when mapped, then AU-10 is covered.
- **Verification:** INSP
- **Origin:** DEC-003

### CR-NIST-003 — Software and information integrity
- **Statement:** The platform MUST detect unauthorised changes to software and information.
- **Control reference:** NIST SP 800-53 SI-7
- **Rationale:** Integrity monitoring.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given NR-DUR-005 and NR-SEC-007, when mapped, then SI-7 is covered.
- **Verification:** INSP
- **Origin:** DEC-003

### CR-NIST-004 — Protection of audit information
- **Statement:** Audit information MUST be protected from unauthorised access, modification and deletion.
- **Control reference:** NIST SP 800-53 AU-9
- **Rationale:** Trustworthy audit.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given FR-AUD-012 and FR-AUD-014, when mapped, then AU-9 is covered.
- **Verification:** INSP, SEC
- **Origin:** DEC-003
