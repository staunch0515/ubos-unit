---
id: UBS-SYS-EXC-02
title: Buk Exchange — Design Requirements
status: draft
phase: PH-4
depends_on: [UBS-SYS-EXC-01]
---

# Buk Exchange — Design Requirements (DSN-EXC-*)

### DSN-EXC-001 — Publisher accounts and namespaces
- **Statement:** Publishers MUST register with verified organisation identity and a `did:ubos` publisher key; namespaces MUST be granted to publishers and verified on every upload (`PKG.NAMESPACE_VIOLATION`).
- **Rationale:** FR-MODEL-015, NR-SEC-007.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** EXC
- **Personas:** PER-ExchangeOperator
- **Acceptance:**
  1. Given an upload into another publisher's namespace, when submitted, then it is rejected.
- **Verification:** SEC
- **Origin:** FR-MODEL-015, NR-SEC-007

### DSN-EXC-002 — Content-addressed archive storage
- **Statement:** Archives MUST be stored by root hash in WORM object storage; a published version MUST be immutable (FR-PKG-014 addressing: `buk://publisher/name@version#root`).
- **Rationale:** FR-PKG-012, FR-PKG-014.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** EXC
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a published version, when a re-upload with different content is attempted, then it is refused.
- **Verification:** SEC
- **Origin:** FR-PKG-012, FR-PKG-014

### DSN-EXC-003 — Reproducible verification service
- **Statement:** EXC MUST verify every upload with the Forge engine in isolated workers (DSN-FRG-305), re-run a sample on a second worker to confirm reproducibility (FR-PKG-063), and publish the signed verification record with the listing.
- **Rationale:** FR-PKG-061, FR-PKG-062, FR-PKG-063.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** EXC, FRG
- **Personas:** PER-ExchangeOperator
- **Acceptance:**
  1. Given two verification runs of the same archive, when compared, then records are identical.
- **Verification:** CONF
- **Origin:** FR-PKG-061, FR-PKG-062, FR-PKG-063

### DSN-EXC-004 — Listings and search
- **Statement:** Listings MUST show manifest data, compatibility (ABI range), capabilities requested, verification record, licence terms, changelog, advisories and publisher; search MUST support name, capability, ontology class and industry tags.
- **Rationale:** Informed installation decisions.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** EXC
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a listing, when viewed, then requested capabilities are shown before purchase or install.
- **Verification:** USE
- **Origin:** FR-PKG-033, FR-STD-062

### DSN-EXC-005 — Licence objects
- **Statement:** Licences (FR-BILL-031) MUST be signed documents binding tenant, Buk, version range, term, seats or metered terms, and offline validity (FR-PKG-083), with lifecycle states (FR-BILL-032) synced to CTL (CTR-053).
- **Rationale:** FR-BILL-031, FR-BILL-032, FR-PKG-083.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** EXC, CTL
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an offline node, when a licence file is imported, then the node validates its signature and term without contacting EXC.
- **Verification:** SEC, CONF
- **Origin:** FR-BILL-031, FR-BILL-032, FR-PKG-083

### DSN-EXC-006 — Invoicing in fiat
- **Statement:** EXC MUST invoice customers in fiat currencies (DEC-012) combining licence fees and CTL-rated usage lines (FR-BILL-041), through a PCI-compliant payment processor; EXC MUST NOT store card data.
- **Rationale:** FR-BILL-041, DEC-012.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** EXC
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a month of usage and two licences, when invoiced, then the invoice lines reconcile with CTL rating.
- **Verification:** CONF
- **Origin:** FR-BILL-041, DEC-012

### DSN-EXC-007 — Publisher payouts
- **Statement:** EXC MUST compute publisher revenue shares from paid invoices and issue payouts with statements (FR-BILL-042), keeping a double-entry ledger (on a UBOS VAE operated by EXC) for all money movements.
- **Rationale:** FR-BILL-042; EXC dogfoods the ledger kind.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** EXC
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given paid invoices, when the payout run executes, then the ledger balances, and statements match payouts.
- **Verification:** CONF
- **Origin:** FR-BILL-042, FR-LEDG-051

### DSN-EXC-008 — Deprecation, yanking and advisories
- **Statement:** Publishers MUST be able to deprecate versions with notices (FR-PKG-052); EXC operators MUST be able to yank versions for security reasons and publish advisories that nodes fetch and show to administrators.
- **Rationale:** FR-PKG-052, NR-SEC-008.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** EXC, NOD
- **Personas:** PER-ExchangeOperator
- **Acceptance:**
  1. Given a yanked version, when a node checks advisories, then administrators of affected tenants see the advisory.
- **Verification:** CONF
- **Origin:** FR-PKG-052, NR-SEC-008
- **Phase note:** DEC-132: the node side of publisher deprecations is built in PH-2 step 7e-2 (FR-PKG-052 is a PH-2 requirement): a version's manifest carries its `deprecation` (`message`, `end_of_support`, `replacement`), and a notice signed with the Buk's publisher key (`forge deprecate`) is recorded on an installed Buk (`POST /api/v1/admin/buks/deprecation`); install plans warn, and the registry shows each Buk as `supported`, `deprecated` or `end_of_support`. Publishing notices through EXC, yanking and advisories stay in PH-4.

### DSN-EXC-009 — Data is never held hostage
- **Statement:** Licence expiry MUST never block reads, exports or uninstalls with data retention (FR-PKG-082, FR-PKG-051).
- **Rationale:** FR-PKG-082.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** EXC, NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an expired licence, when the tenant exports, then the export succeeds.
- **Verification:** CONF
- **Origin:** FR-PKG-082, FR-PKG-051

### DSN-EXC-010 — Private registries and mirrors
- **Statement:** The same EXC software MUST run as a private registry inside a customer's Server or as a mirror for air-gapped deployments, syncing signed archives and advisories.
- **Rationale:** FR-OPS-013, sovereign deployments.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** EXC
- **Phase note:** moved to PH-4 by DEC-023 (no design partners in PH-3).
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a mirror, when synced from the public Exchange, then archives and verification records are identical.
- **Verification:** CONF
- **Origin:** FR-OPS-013, FR-PKG-083
