---
id: UBS-REQ-05-19
title: Functional Requirements — STD (Governance of the BPA Standard)
status: complete
phase: PH-0
depends_on: [UBS-REQ-05]
---

# Functional Requirements — STD

The BPA is a product of its own: a published, language-neutral standard with a
conformance suite and a certification programme. This is the basis of the "ARM
architecture of business logic" positioning (EXT-BPU, UBS-META-06 §4).

## CAP-STD-01 — Standard versioning

### FR-STD-011 — Versioned releases of the standard
- **Statement:** The BPA MUST be published in versioned releases (`MAJOR.MINOR.PATCH`). Each release MUST include:
  - the normative text and the grammars and schemas;
  - the conformance vectors;
  - a change log and a compatibility statement.
- **Rationale:** A standard others can implement against.
- **Priority:** Must · **Phase:** PH-0 · **Systems:** BPA
- **Personas:** PER-StandardsSteward, PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given a release, when inspected, then every listed component is present and consistent.
- **Verification:** INSP
- **Origin:** EXT-BPU, EXT-RFC

### FR-STD-012 — Errata process
- **Statement:** Ambiguities and defects MUST be handled through a public errata process with IDs, status and resolution, reflected in PATCH releases.
- **Rationale:** Standards need maintenance.
- **Priority:** Must · **Phase:** PH-0 · **Systems:** BPA
- **Personas:** PER-StandardsSteward
- **Acceptance:**
  1. Given a reported ambiguity, when resolved, then the erratum is published with the clarified text and new vectors.
- **Verification:** INSP
- **Origin:** NEW

### FR-STD-013 — Normative language and structure
- **Statement:** The standard MUST use RFC 2119 keywords, stable clause IDs (`STD-*`), ABNF grammars and JSON Schemas, and MUST mark informative text explicitly.
- **Rationale:** Precision for implementers and AI.
- **Priority:** Must · **Phase:** PH-0 · **Systems:** BPA
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given the standard, when linted, then every normative clause has an ID.
- **Verification:** INSP
- **Origin:** EXT-RFC

### FR-STD-014 — Profiles
- **Statement:** The standard MUST define conformance profiles, at least:
  - `Core`: model, versioning, transactions, instruction set, expressions;
  - `Bitemporal`;
  - `Ledger`;
  - `Governance`: LGS and policy decisions;
  - `Proof`;
  - `Sync`;
  - `Browser`.

  An implementation claims profiles individually.
- **Rationale:** Incremental adoption by implementers.
- **Priority:** Must · **Phase:** PH-0 · **Systems:** BPA
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given the profile list, when inspected, then each profile lists its clauses and vectors.
- **Verification:** INSP
- **Origin:** NEW

## CAP-STD-02 — Conformance vectors

### FR-STD-021 — Language-neutral vectors
- **Statement:** Conformance vectors MUST be data files (inputs, operations, expected outputs and errors) that are independent of implementation language, with a reference runner that drives an implementation through UBTP or an in-process adapter.
- **Rationale:** Anyone can test any implementation.
- **Priority:** Must · **Phase:** PH-0 · **Systems:** BPA, FRG
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given the runner and an adapter, when run against the reference DVM, then the results are reported per vector.
- **Verification:** CONF
- **Origin:** EXT-BPU

### FR-STD-022 — Coverage of Must clauses
- **Statement:** Every Must clause of the standard MUST be covered by at least one positive and one negative vector.
- **Rationale:** Conformance is only as good as its coverage.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** BPA
- **Personas:** PER-StandardsSteward
- **Acceptance:**
  1. Given the traceability index, when checked, then no Must clause lacks vectors.
- **Verification:** INSP
- **Origin:** NEW

### FR-STD-023 — Signed runner reports
- **Statement:** The runner MUST produce reports that include the implementation identity, versions, vectors run and results, signed by the runner's key.
- **Rationale:** Certification evidence (SCN-407).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** FRG
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given a report, when its signature is verified, then it matches the runner key.
- **Verification:** CONF
- **Origin:** NEW

## CAP-STD-03 — Formal models

### FR-STD-031 — Formal models of core semantics
- **Statement:** Formal models (TLA+ or equivalent) MUST exist for commit and CAS, merge, ledger sequencing, bitemporal resolution and, in PH-3, sync. Their safety properties MUST be model-checked for declared bounds.
- **Rationale:** "Pre-silicon verification" (UBS-META-06 §4).
- **Priority:** Must · **Phase:** PH-0 · **Systems:** BPA
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the commit and CAS model, when checked, then "no lost update" and "atomic visibility" hold within the bounds.
- **Verification:** FORM
- **Origin:** EXT-TRI ("formal verification ready")

### FR-STD-032 — Model–vector alignment
- **Statement:** Every property checked in a formal model MUST map to conformance vectors or property tests that exercise it on implementations.
- **Rationale:** Connect models to code.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** BPA
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a model property, when traced, then its vectors are listed.
- **Verification:** INSP
- **Origin:** NEW

## CAP-STD-04 — Compatibility policy

### FR-STD-041 — ABI and format stability
- **Statement:** The compatibility policy MUST guarantee the following:
  - minor releases never break conforming programs, stored data or protocols;
  - major releases have a support window of at least 24 months;
  - deprecations are announced at least one minor release ahead.
- **Rationale:** Long-lived business logic (FR-LOGIC-012).
- **Priority:** Must · **Phase:** PH-0 · **Systems:** BPA
- **Personas:** PER-ThirdPartyImplementer, PER-IsvDeveloper
- **Acceptance:**
  1. Given the vectors of version N.m, when run on N.(m+1), then all pass.
- **Verification:** CONF
- **Origin:** NEW

### FR-STD-042 — Storage format compatibility
- **Statement:** Export formats (FR-TEN-061) MUST remain importable by all later minor versions of the same major version.
- **Rationale:** Portability over time.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** BPA
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an export from 1.0, when imported into 1.5, then the head roots match.
- **Verification:** CONF
- **Origin:** NEW
- **Phase note:** DEC-087: moved from PH-1; importing a 1.0 export into a later minor version is verified when the first minor version after BPA 1.0 final exists. 1.0 archives round-trip since PH-1 (VER-CONF-8403).

### FR-STD-043 — Extension rules
- **Statement:** The standard MUST define how vendors extend instructions, value types, declaration properties and widgets without conflicting with future standard versions (for example vendor-prefixed names).
- **Rationale:** Innovation without fragmentation.
- **Priority:** Must · **Phase:** PH-0 · **Systems:** BPA
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given a vendor extension, when named per the rules, then it cannot collide with standard names.
- **Verification:** INSP
- **Origin:** NEW

## CAP-STD-05 — Certification

### FR-STD-051 — "BPU Compliant" certification
- **Statement:** A certification programme MUST certify implementations per profile, based on reproducible conformance runs, with public listing, version scope and expiry (SCN-407).
- **Rationale:** An ecosystem needs a trust mark (EXT-BPU).
- **Priority:** Must · **Phase:** PH-4 · **Systems:** BPA
- **Personas:** PER-StandardsSteward, PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given SCN-407, when executed, then a certificate is issued for the passing profiles.
- **Verification:** SCN, AUDIT
- **Origin:** EXT-BPU

### FR-STD-052 — Certificate verification
- **Statement:** Certificates MUST be signed and publicly verifiable, and MUST be revocable for cause.
- **Rationale:** Trustworthy marks.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** BPA
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given a revoked certificate, when verified, then revocation is reported.
- **Verification:** CONF
- **Origin:** NEW

### FR-STD-053 — Recertification
- **Statement:** Certification MUST be renewed for each major standard version and at most every 24 months.
- **Rationale:** Currency of claims.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** BPA
- **Personas:** PER-StandardsSteward
- **Acceptance:**
  1. Given an expired certificate, when listed, then it is shown as expired.
- **Verification:** INSP
- **Origin:** NEW

## CAP-STD-06 — Reference licensing

### FR-STD-061 — Embeddable reference DVM
- **Statement:** The reference DVM MUST be packaged as an embeddable library with a stable host API, so that OEMs can embed it in their products.
- **Rationale:** An ARM-style IP licensing model (EXT-BPU).
- **Priority:** Must · **Phase:** PH-4 · **Systems:** DVM, BPA
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given the embedding sample, when built by a third party, then it passes the Core profile.
- **Verification:** CONF, SCN
- **Origin:** EXT-BPU

### FR-STD-062 — Licence terms metadata
- **Statement:** The reference implementation's licence options (open-source edition terms and commercial embedding licences) MUST be documented with machine-readable metadata.
- **Rationale:** Clarity for adopters.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** BPA
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given the repository, when inspected, then the licence metadata is present and consistent.
- **Verification:** INSP
- **Origin:** NEW

## CAP-STD-07 — Extension registry

### FR-STD-071 — Registered extensions
- **Statement:** A public registry MUST record vendor extensions (instructions, value types, declaration properties, widgets) with owner, schema, status and conformance vectors.
- **Rationale:** Discoverability and non-collision.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** BPA
- **Personas:** PER-StandardsSteward
- **Acceptance:**
  1. Given a registered extension, when looked up, then its schema and vectors are available.
- **Verification:** INSP
- **Origin:** NEW

### FR-STD-072 — Promotion into the standard
- **Statement:** Widely adopted extensions SHOULD be promotable into the standard through the change process, with a compatibility mapping.
- **Rationale:** Evolution path.
- **Priority:** Could · **Phase:** PH-4 · **Systems:** BPA
- **Personas:** PER-StandardsSteward
- **Acceptance:**
  1. Given a promoted extension, when the new standard is released, then the mapping from vendor name to standard name is documented.
- **Verification:** INSP
- **Origin:** NEW
