---
id: UBS-REQ-05-18
title: Functional Requirements — NTF (Notifications), FILE (Files), SIGN (Signatures), LOC (Localisation)
status: complete
phase: PH-1
depends_on: [UBS-REQ-05]
---

# Functional Requirements — NTF, FILE, SIGN and LOC

## CAP-NTF-01 — Notification rules

### FR-NTF-011 — Rules from events and deadlines
- **Statement:** Notification rules MUST create notifications from events, due dates and escalations. A rule declares recipients (principals, roles, relationships), template, channels and throttling.
- **Rationale:** SCN-206.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a rule "30, 14 and 3 days before due", when the dates pass, then 3 notifications are sent.
- **Verification:** SCN
- **Origin:** NEW

### FR-NTF-012 — Permission-aware content
- **Statement:** Notification content MUST be rendered per recipient with that recipient's permissions and masking.
- **Rationale:** Notifications are an output channel.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given two recipients with different clearances, when notified, then each sees only permitted fields.
- **Verification:** SEC
- **Origin:** NEW

### FR-NTF-013 — Delivery records
- **Statement:** Notifications MUST record delivery status per channel and recipient.
- **Rationale:** Proof of notification (for example contractual notices).
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given an e-mail notification, when delivered, then the delivery status is recorded.
- **Verification:** CONF
- **Origin:** NEW

## CAP-NTF-02 — Channels

### FR-NTF-021 — Supported channels
- **Statement:** The platform MUST deliver notifications in-app, by e-mail and by mobile push. It SHOULD also deliver them to chat tools (FR-INT-062).
- **Rationale:** Reach users where they work.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a user preferring push, when a task is assigned, then a push notification arrives.
- **Verification:** SCN
- **Origin:** NEW

### FR-NTF-022 — Channel fallbacks
- **Statement:** Rules MAY declare fallbacks (for example e-mail if push is unacknowledged after 1 hour).
- **Rationale:** Critical notices must arrive.
- **Priority:** Could · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given an unacknowledged push, when 1 hour passes, then an e-mail is sent.
- **Verification:** CONF
- **Origin:** NEW

## CAP-NTF-03 — Preferences and digests

### FR-NTF-031 — User preferences
- **Statement:** Users MUST be able to set channel preferences, quiet hours and digests per notification type, within the limits of the tenant policy (mandatory notices cannot be disabled).
- **Rationale:** Reduce noise.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given quiet hours, when a non-urgent notification arrives, then it is delivered after the quiet hours end.
- **Verification:** CONF
- **Origin:** NEW

### FR-NTF-032 — Digests
- **Statement:** The platform MUST combine notifications into daily or weekly digests per user preference.
- **Rationale:** Summaries instead of floods.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given 30 notifications in a day, when a daily digest is chosen, then one digest is delivered.
- **Verification:** CONF
- **Origin:** NEW

## CAP-FILE-01 — Blob storage

### FR-FILE-011 — Content-addressed files
- **Statement:** Files MUST be stored as content-addressed, chunked blobs (FR-PROOF-013), linked to object versions. A file change creates a new version of the linking object.
- **Rationale:** Files participate in history and proofs.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a replaced attachment, when history is viewed, then both files are accessible by version.
- **Verification:** CONF
- **Origin:** L40:SPEC-11
- **Phase note:** PH-2 step 0b (DEC-087): moved from PH-1 for its acceptance test (replacing an attachment and reading both files by version); the content-addressed file service exists since PH-1 step 17a.

### FR-FILE-012 — Size limits and streaming
- **Statement:** File upload and download MUST stream, support resumable uploads, and enforce per-class and per-tenant size limits.
- **Rationale:** Large documents and reliability.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, SDK
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a 2 GB upload interrupted at 50%, when resumed, then it completes without restarting.
- **Verification:** CONF, FAULT
- **Origin:** NEW

### FR-FILE-013 — Encryption and classification
- **Statement:** Files MUST inherit the classification of their linking field and MUST be encrypted accordingly (per-subject keys for personal files).
- **Rationale:** Consistent protection.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a personal file, when the subject is erased, then the file becomes unreadable.
- **Verification:** SEC
- **Origin:** NEW

## CAP-FILE-02 — Content safety

### FR-FILE-021 — Malware scanning
- **Statement:** Uploaded files MUST be scanned for malware before they become available to other users. Infected files MUST be quarantined.
- **Rationale:** Security.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the EICAR test file, when uploaded, then it is quarantined, and the uploader is informed.
- **Verification:** SEC
- **Origin:** NEW

### FR-FILE-022 — Type checks
- **Statement:** Classes MUST be able to restrict allowed file types. Files MUST be validated by content sniffing, not by extension alone.
- **Rationale:** Prevent disguised content.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given an executable renamed to `.pdf`, when uploaded to a PDF-only field, then it is rejected.
- **Verification:** SEC
- **Origin:** NEW

## CAP-FILE-03 — Previews and extraction

### FR-FILE-031 — Previews
- **Statement:** The platform MUST generate previews for common document and image types, in a sandbox.
- **Rationale:** Usability without downloads.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD, WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a DOCX attachment, when viewed, then a preview renders.
- **Verification:** SCN
- **Origin:** NEW

### FR-FILE-032 — Text extraction and indexing
- **Statement:** Text MUST be extracted from supported files (including OCR for scanned PDFs, where enabled) for full-text and semantic search.
- **Rationale:** Searchable documents.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a scanned PDF with OCR enabled, when searched for a phrase in it, then the file is found.
- **Verification:** SCN
- **Origin:** NEW

## CAP-FILE-04 — VFS paths

### FR-FILE-041 — Path aliases over objects
- **Statement:** The platform MUST expose a virtual file system view with the roots `/data`, `/usr`, `/etc`, `/view` and `/lib`. The paths MUST be aliases resolving to canonical URIs (UBS-META-06 §6).
- **Rationale:** Uniform, human-readable addressing (EXT-TRI "everything is a file").
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, SDK
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given `/usr/logic/calc_tax`, when resolved, then it returns the canonical URI of the logic asset.
- **Verification:** CONF
- **Origin:** EXT-TRI, EXT-RFC

### FR-FILE-042 — Mountable views
- **Statement:** The VFS view SHOULD be mountable read-only through WebDAV for document access by external tools, with permissions applied.
- **Rationale:** Interoperability with desktop tools.
- **Priority:** Could · **Phase:** PH-3 · **Systems:** NOD
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a WebDAV mount, when browsing `/data/contracts`, then only permitted documents appear.
- **Verification:** SEC
- **Origin:** NEW

## CAP-SIGN-01 — Electronic signatures

### FR-SIGN-011 — E-SIGN and UETA compliant signing
- **Statement:** Electronic signing MUST capture consent to electronic records, signer identity verification, intent to sign, an association of the signature with the record, and record retention and reproduction for all parties.
- **Rationale:** Legal enforceability in the US (DEC-003).
- **Priority:** Must · **Phase:** PH-5 · **Systems:** WSP, DVM
- **Personas:** PER-ContractManager, PER-ExternalParty
- **Acceptance:**
  1. Given a signing ceremony, when completed, then consent, identity evidence and intent are recorded per signer.
- **Verification:** SCN, AUDIT
- **Origin:** NEW
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### FR-SIGN-012 — Signer authentication levels
- **Statement:** Signing MUST support authentication levels (e-mail link, SMS one-time code, SSO, passkey) configurable per document type.
- **Rationale:** Risk-appropriate assurance.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** WSP, NOD
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given a document type requiring a passkey, when a signer uses only e-mail, then signing is refused.
- **Verification:** SEC
- **Origin:** NEW
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### FR-SIGN-013 — Copies for all parties
- **Statement:** Every signer MUST receive the executed document and its verification data.
- **Rationale:** E-SIGN retention and reproduction requirements.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** WSP, NOD
- **Personas:** PER-ExternalParty
- **Acceptance:**
  1. Given execution, when completed, then each signer receives the PDF and a verification link.
- **Verification:** SCN
- **Origin:** NEW
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

## CAP-SIGN-02 — Document seals

### FR-SIGN-021 — Seal over document and data
- **Statement:** A signature seal MUST be computed over the rendered document hash, the data snapshot (the object versions bound in the document) and the signer's intent statement, and MUST be signed with the signer's or the platform's key according to the authentication level.
- **Rationale:** Bind the signature to both text and data (EXT-TRI "Live Contract").
- **Priority:** Must · **Phase:** PH-5 · **Systems:** DVM, WSP
- **Personas:** PER-ContractManager, PER-RegulatorExaminer
- **Acceptance:**
  1. Given a sealed document, when one data value is changed, then seal verification fails.
- **Verification:** CONF, SEC
- **Origin:** EXT-TRI
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### FR-SIGN-022 — Embedded verification data
- **Statement:** Executed PDFs MUST embed the seal and verification references, and the verifier (CAP-PROOF-06) MUST verify them.
- **Rationale:** Portable proof.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** WSP
- **Personas:** PER-ExternalParty
- **Acceptance:**
  1. Given an executed PDF, when checked with the verifier, then the seal verifies.
- **Verification:** CONF
- **Origin:** NEW
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### FR-SIGN-023 — Executed state is immutable
- **Statement:** Once executed, the signed document version and its bound data versions MUST be immutable. Changes require amendments (SCN-205).
- **Rationale:** Legal certainty.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** DVM
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given an executed agreement, when a direct edit is attempted, then it fails with `SIGN.EXECUTED_IMMUTABLE`.
- **Verification:** CONF
- **Origin:** NEW
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

## CAP-SIGN-03 — Signing ceremonies

### FR-SIGN-031 — Multi-party ordered and parallel signing
- **Statement:** Ceremonies MUST support ordered, parallel and mixed signer groups, with reminders, expiry and decline with reason.
- **Rationale:** Real contracting processes.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** WSP, DVM
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given the order internal → counterparty → internal countersign, when run, then each step waits for the previous one.
- **Verification:** SCN
- **Origin:** NEW
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### FR-SIGN-032 — Void on change
- **Statement:** Any change to the document or bound data during a ceremony MUST void pending signatures and restart the ceremony (SCN-204 A1).
- **Rationale:** Sign what was seen.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** DVM
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given SCN-204 A1, when executed, then the outcomes match.
- **Verification:** SCN
- **Origin:** NEW
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### FR-SIGN-033 — Ceremony audit trail
- **Statement:** Ceremonies MUST produce an audit trail (sent, viewed, consented, authenticated, signed, declined) included in the executed record.
- **Rationale:** Evidence.
- **Priority:** Must · **Phase:** PH-5 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given an executed document, when audited, then all ceremony events are present.
- **Verification:** CONF
- **Origin:** NEW
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

## CAP-SIGN-04 — External signature providers

### FR-SIGN-041 — Provider integration
- **Statement:** The platform SHOULD integrate external signature providers (including advanced and qualified signature services) through connectors, and MUST store their evidence in the executed record.
- **Rationale:** Customer choice and cross-border needs.
- **Priority:** Should · **Phase:** PH-5 · **Systems:** BRG, WSP
- **Personas:** PER-ContractManager
- **Acceptance:**
  1. Given a provider-signed document, when stored, then the provider evidence is attached and verifiable.
- **Verification:** CONF
- **Origin:** NEW
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

### FR-SIGN-042 — Uniform seal model
- **Statement:** External signatures MUST be wrapped in the platform seal model (FR-SIGN-021), so that verification is uniform.
- **Rationale:** One verifier.
- **Priority:** Should · **Phase:** PH-5 · **Systems:** WSP
- **Personas:** PER-RegulatorExaminer
- **Acceptance:**
  1. Given an externally signed document, when verified, then the platform seal also verifies.
- **Verification:** CONF
- **Origin:** NEW
- **Phase note:** moved to PH-5 by DEC-022 (contract or fund scope deferred).

## CAP-LOC-01 — Languages

### FR-LOC-011 — Translatable UI and model labels
- **Statement:** UI texts, class and field labels, descriptions, enum labels and message templates MUST be translatable, with fallbacks per locale.
- **Rationale:** International users.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP, DVM
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a missing German label, when displayed in German, then the English fallback appears.
- **Verification:** CONF
- **Origin:** NEW

### FR-LOC-012 — Translatable content fields
- **Statement:** Classes MUST be able to declare multilingual text fields that store values per language.
- **Rationale:** Multilingual master data (for example product names).
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a multilingual name, when read in French, then the French value is returned.
- **Verification:** CONF
- **Origin:** NEW

### FR-LOC-013 — Right-to-left support
- **Statement:** Clients SHOULD support right-to-left scripts.
- **Rationale:** Global readiness.
- **Priority:** Could · **Phase:** PH-3 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given Arabic locale, when forms render, then the layout mirrors correctly.
- **Verification:** USE
- **Origin:** NEW

## CAP-LOC-02 — Currencies and rates

### FR-LOC-021 — Currency reference data
- **Statement:** ISO 4217 currencies with minor units MUST be provided as reference data, versioned and effective-dated.
- **Rationale:** Correct money handling.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given JPY, when money is stored, then scale 0 is enforced by default.
- **Verification:** CONF
- **Origin:** NEW

### FR-LOC-022 — Bitemporal FX rates
- **Statement:** FX rates MUST be bitemporal Definition objects with source, rate type and fixing time. Conversions MUST reference the rate version used.
- **Rationale:** Reproducible conversions (FR-LEDG-053).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a corrected rate, when a past conversion is re-read `as_originally`, then the old rate is used.
- **Verification:** CONF
- **Origin:** IMP-02

## CAP-LOC-03 — Time zones

### FR-LOC-031 — UTC instants and zone-aware display
- **Statement:** Instants MUST be stored in UTC. Display, input and scheduling MUST use the user's or the object's declared time zone, with IANA zone identifiers and tz database updates.
- **Rationale:** Correct time across regions.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given an instant, when viewed in New York and Tokyo, then local times differ correctly.
- **Verification:** CONF
- **Origin:** L40:SPEC-02

### FR-LOC-032 — DST-safe scheduling
- **Statement:** Schedules and business-date computations MUST handle DST transitions deterministically, including non-existent and repeated local times.
- **Rationale:** No missed or doubled runs.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a 02:30 local schedule on the spring-forward day, when the day passes, then the misfire policy applies as documented.
- **Verification:** CONF
- **Origin:** NEW

## CAP-LOC-04 — Business calendars

### FR-LOC-041 — Holiday calendars and business days
- **Statement:** Calendars MUST be effective-dated objects defining holidays and weekends per market, and composable (for example "NYSE ∪ US bank holidays"). Business-day arithmetic MUST be available in L1 expressions.
- **Rationale:** Dealing, settlement and deadlines (SCN-102, SCN-206).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given the NYSE calendar, when computing T+2 from the day before Thanksgiving, then the result skips the holiday.
- **Verification:** CONF, PROP
- **Origin:** NEW

### FR-LOC-042 — Cut-offs
- **Statement:** Cut-off rules (a local time in a zone, per calendar) MUST be declarable and usable by guards and dealing-date logic.
- **Rationale:** SCN-102 A1.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-InvestorServicesClerk
- **Acceptance:**
  1. Given a 12:00 New York cut-off, when an order arrives at 12:05, then the dealing date moves to the next business day.
- **Verification:** CONF
- **Origin:** NEW

## CAP-LOC-05 — Formats

### FR-LOC-051 — Locale formats
- **Statement:** Numbers, dates, money and addresses MUST be displayed and parsed according to the user's locale (CLDR data).
- **Rationale:** Usability.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given the de-DE locale, when 1234.5 is shown, then it displays as "1.234,5".
- **Verification:** CONF
- **Origin:** NEW

### FR-LOC-052 — Canonical storage independent of locale
- **Statement:** Stored values MUST be locale-independent canonical forms. Locale affects only presentation and parsing.
- **Rationale:** Determinism and portability.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given input in de-DE and en-US, when stored, then the canonical values are identical.
- **Verification:** CONF
- **Origin:** L40:SPEC-02
