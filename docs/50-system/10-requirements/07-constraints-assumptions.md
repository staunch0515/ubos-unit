---
id: UBS-REQ-07
title: Constraints and Assumptions
status: complete
phase: ALL
depends_on: [UBS-META-00, UBS-META-06]
---

# Constraints and Assumptions

**Constraints** (`CST-*`) are fixed conditions that the design MUST respect. They come from
decisions, principles or the environment.

**Assumptions** (`ASM-*`) are conditions believed true that the plan depends on. Each
assumption names what happens if it turns out false.

## 1. Constraints

| ID | Constraint | Source |
|---|---|---|
| CST-001 | The BPA standard is language-neutral. The reference implementation is Rust (kernel, servers, CLI) and TypeScript (web clients), with Tauri 2 for desktop and mobile. | DEC-007 |
| CST-002 | Authoritative storage uses PostgreSQL 16+ (Server, Cell) and SQLite (Box, browser, mobile; from PH-4, DEC-024). The runtime store is non-authoritative and may be lost at any time without data loss. | DEC-007, FR-EVT-032 |
| CST-003 | Authoritative state is written only by the commit path. Projections, caches and indexes are non-authoritative and rebuildable. | DEC-011 |
| CST-004 | Business classes do not create database tables by default. Typed tables exist only as declared projections. | DEC-011 |
| CST-005 | The kernel contains no business rules. Business behaviour ships as Buks. | EXT-TRI, principle P1 |
| CST-006 | Clients contain no business rules and use only the SDK and UBTP. | FR-UX-011, L40:SPEC-26 |
| CST-007 | Logic reaches external systems only through connectors and the instruction set. | FR-LOGIC-014 |
| CST-008 | No tokens, cryptocurrencies, public compute markets, intent-economy features or organisational-governance protocols are specified or built. Public blockchains may be used only as timestamp anchors. | DEC-012 |
| CST-009 | All money is fiat. Licensing and billing are in USD first. | DEC-012, FR-BILL-041 |
| CST-010 | The first regulatory focus is the United States (SOC 2, SEC, FINRA, SOX, E-SIGN), with GDPR designed in. | DEC-003 |
| CST-011 | External interfaces use open standards: JSON Schema, OpenAPI, CloudEvents, OpenTelemetry, OIDC, SAML, SCIM, OAuth 2.1, MCP, Parquet, Iceberg, PDF/A, ISO 20022. | IMP-12 |
| CST-012 | Canonical hashing uses RFC 8785 JSON canonicalisation and SHA-256, with algorithm identifiers. Signatures use Ed25519. | EXT-RFC, FR-PROOF-011 |
| CST-013 | Every normative change to kernel semantics requires a BPA standard change with conformance vectors. | FR-STD-011 |
| CST-014 | Agents never merge their own work and never hold raw secrets. | FR-AI-025, FR-IAM-091 |
| CST-015 | Phases end only when every exit criterion is evidenced (not on a date). | DEC-006 |
| CST-016 | Documentation of this volume is in English. Product-owner reports are in Chinese. | UBS-META-00 |
| CST-017 | Operators of hosted Cells must not be able to read tenant business data through platform tooling. | FR-OPS-081, FR-OPS-042 |
| CST-018 | Customer data must always be exportable in open formats, regardless of licence state. | FR-PKG-082, FR-TEN-061 |

## 2. Assumptions

| ID | Assumption | If false |
|---|---|---|
| ASM-001 | A core team of 8–12 engineers is available from PH-0, growing to 15–20 by PH-4, with at least 3 senior Rust engineers and 1 formal-methods-capable engineer. | Durations stretch; PH-3 AI and PH-5 federation scope is reduced by DEC. |
| ASM-002 | An independent US CPA (or a firm) is available in PH-3 to run the audit drill on the sample company's books (SCN-511), replacing the design-partner pilot (DEC-022, DEC-023). | G3 is evidenced by an internal audit drill run by a qualified accountant under the same protocol, and the external drill moves to the first PH-4 release. |
| ASM-003 | PostgreSQL 16+ on NVMe meets the NR-PERF commit targets with the designed storage layout. | The storage engine design is revisited in PH-1 (for example a custom log-structured object store in front of PostgreSQL). |
| ASM-004 | Commercial and self-hostable LLMs with structured output and tool use remain available at acceptable cost and with US data residency options. | AI features degrade to self-hosted models with narrower capabilities; G4 targets are revised. |
| ASM-005 | An RFC 3161 time-stamp authority acceptable to US regulators and auditors is available for anchoring. | Anchoring uses multiple commercial TSAs or a notarisation partner. |
| ASM-006 | WORM-capable object storage (compliance-mode object lock) is available in the target clouds and on-premises appliances. | SEC 17a-4 compliance uses the audit-trail alternative and a designated third party. |
| ASM-007 | Customers accept a metadata-driven platform with progressive disclosure, and business users do not need to learn branching. | UX investment increases; the builder layer takes more of the burden. |
| ASM-008 | Rhai and a WASM component runtime (wasmtime class) meet the determinism and sandboxing requirements with the planned hardening. | The L2 language is replaced or forked; the ABI remains unchanged. |
| ASM-009 | Tauri 2 supports the desktop and mobile targets with an embedded Rust kernel. | Mobile uses a thinner client without an embedded kernel until support matures. |
| ASM-010 | Third parties are interested in implementing or embedding the BPA once a vertical proves value. | The ecosystem strategy focuses on Buk ISVs rather than engine implementers; certification scope narrows. |
| ASM-011 | Browsers provide origin-private file system storage and WebAssembly performance sufficient for the browser kernel. | The browser kernel is limited to read caching (PH-5 scope reduction). |
| ASM-012 | The sample-company dataset (Northwind Components, Inc.) with independently computed expected results is sufficient for benchmarks, simulations and acceptance; no production data is needed (DEC-022). | Larger synthetic workloads are generated (UBS-REQ-06, WL-FIN-M) and expected results are recomputed with the reference spreadsheet workbook. |
| ASM-013 | Accountants familiar with US GAAP are available as usability-study participants in PH-2 (EXIT-2-18). | Studies use finance students or bookkeepers with a documented experience profile; the thresholds stay unchanged. |
| ASM-014 | The owner's Java system implements the same finance requirements (`docs/finance-requirements/`, version 1.0 copied unchanged into its own repository, DEC-029) in the same period, so that the comparison protocol (DEC-026) has two comparable data sets. | The comparison reports UBOS data only, against the protocol's absolute metrics, and is repeated when the Java data become available. |
