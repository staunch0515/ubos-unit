---
id: UBS-REQ-05-13
title: Functional Requirements — PKG (Buks) and INT (Integration)
status: complete
phase: PH-1
depends_on: [UBS-REQ-05, UBS-REQ-05-02]
---

# Functional Requirements — PKG and INT

## CAP-PKG-01 — Buk format

### FR-PKG-011 — Buk manifest and content
- **Statement:** A Buk MUST consist of a manifest and content.
  - The manifest contains: name, publisher DID, semantic version, ABI range, dependencies, owned namespaces, declared capabilities, required configuration, licence reference and description.
  - The content contains: classes, extensions, lifecycles, ports, sheets, decision tables, logic assets, views, knowledge, seed data, migrations and lenses, and scenarios.
- **Rationale:** The unit of feature delivery (EXT-TRI "Buk", L40:SPEC-28).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG, DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a Buk lacking a publisher DID, when packed, then it fails with `PKG.MANIFEST_INVALID`.
- **Verification:** CONF
- **Origin:** EXT-TRI, L40:SPEC-28

### FR-PKG-012 — Canonical, content-addressed archive
- **Statement:** A Buk archive MUST list every content item with its hash. The archive root hash MUST be signed by the publisher.
- **Rationale:** Supply-chain integrity.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given one tampered byte in any item, when the archive is verified, then verification fails.
- **Verification:** CONF, SEC
- **Origin:** IMP-03

### FR-PKG-013 — Seed data with temporal attributes
- **Statement:** Seed data in a Buk MUST declare the valid time and the target branch kinds for each seed object.
- **Rationale:** Reference data (calendars, currencies) is effective-dated.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG, DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given holiday calendars for 2027, when installed, then they are valid from their declared dates.
- **Verification:** CONF
- **Origin:** NEW

### FR-PKG-014 — Buk addressing
- **Statement:** Installed Buk items MUST be addressable by URI, including the Buk name and version for inspection.
- **Rationale:** Traceability from runtime objects to the package that supplied them.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given an object supplied by a Buk, when inspected, then the Buk name and version are shown.
- **Verification:** CONF
- **Origin:** NEW

## CAP-PKG-02 — Dependencies

### FR-PKG-021 — Semantic version ranges
- **Statement:** Dependencies MUST be declared with semantic version ranges, and the ABI compatibility range MUST be declared.
- **Rationale:** Composability ("Math Buk", "Currency Buk").
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a range `^1.2`, when 1.4.0 and 2.0.0 are available, then 1.4.0 is selected.
- **Verification:** CONF
- **Origin:** EXT-TRI

### FR-PKG-022 — Deterministic resolution and lock
- **Statement:** Dependency resolution MUST be deterministic and MUST produce a lock record stored with the installation.
- **Rationale:** Reproducible installations.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG, DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given the same inputs, when resolved twice, then the lock records are identical.
- **Verification:** CONF, PROP
- **Origin:** NEW

### FR-PKG-023 — Conflict detection
- **Statement:** Resolution MUST detect namespace conflicts, incompatible version requirements and ABI incompatibility, and MUST report them all before any change.
- **Rationale:** Fail early and completely.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given two Buks claiming one namespace, when resolved, then the conflict is reported, and nothing is installed.
- **Verification:** CONF
- **Origin:** L40:SPEC-28

## CAP-PKG-03 — Installation

### FR-PKG-031 — Install through a loader branch
- **Statement:** Installation MUST:
  1. stage the Buk on a `loader` branch;
  2. run validation and the Buk's scenarios in a sandbox copy;
  3. present an install plan;
  4. merge into the target line as a governed change set.
- **Rationale:** Installation is a governed change (L40:SPEC-28).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a failing scenario, when installed, then the merge is blocked, and the target is unchanged.
- **Verification:** CONF, SCN
- **Origin:** L40:SPEC-28

### FR-PKG-032 — Install permissions
- **Statement:** Installing, upgrading and removing Buks MUST require dedicated permissions and MUST be audited.
- **Rationale:** Buks change behaviour broadly.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a user without `buk.install`, when installing, then it is denied.
- **Verification:** SEC
- **Origin:** NEW

### FR-PKG-033 — Install plan
- **Statement:** The install plan MUST list the classes, extensions, sheets, logic, views, seed data, subscriptions, schedules, connectors and required configuration to be added or changed, with counts.
- **Rationale:** Informed decisions.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, STU
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a Buk, when planned, then every category is listed, and the plan equals the resulting diff.
- **Verification:** CONF
- **Origin:** NEW

### FR-PKG-034 — Post-install configuration
- **Statement:** Buks MUST be able to declare required configuration (for example connector credentials or calendars). The platform MUST guide administrators through it and MUST keep the affected features disabled until it is complete.
- **Rationale:** Working installations.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** STU
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a missing credential, when a dependent action is invoked, then it fails with `PKG.CONFIGURATION_INCOMPLETE`.
- **Verification:** CONF
- **Origin:** NEW

## CAP-PKG-04 — Upgrade

### FR-PKG-041 — Upgrades merge into overlays
- **Statement:** Upgrading a Buk MUST stage the new version on a loader branch, show the diff from the installed version, merge it into each overlay with conflicts reported per overlay, and preserve overlay changes (SCN-013).
- **Rationale:** Customisations survive upgrades.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator, PER-IsvDeveloper
- **Acceptance:**
  1. Given SCN-013, when executed, then the outcomes match.
- **Verification:** SCN, PROP
- **Origin:** EXT-TRI, L40:SPEC-28

### FR-PKG-042 — Migrations shipped with Buks
- **Statement:** Buk versions MUST ship lenses and migration processes for their class changes (CAP-MODEL-08), and the platform MUST apply them during upgrade.
- **Rationale:** Schema evolution across customers.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, FRG
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a Buk that renames a field with a lens, when upgraded, then old versions read in the new shape.
- **Verification:** CONF
- **Origin:** IMP-04

### FR-PKG-043 — Staged rollout
- **Statement:** Upgrades MUST support staged rollout across VAEs (for example `uat` before `prod`) with gates.
- **Rationale:** Operational safety.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** CTL, STU
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a two-stage plan, when stage 1 fails its gate, then stage 2 does not start.
- **Verification:** SCN
- **Origin:** NEW

### FR-PKG-044 — Upgrade rollback
- **Statement:** An upgrade MUST be revertible by reverting its merge. Data written after the upgrade under the new schema MUST be retained and readable through the lenses.
- **Rationale:** Recovery from a bad upgrade.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an upgrade followed by 10 new records, when rolled back, then the 10 records remain readable.
- **Verification:** SCN
- **Origin:** NEW

## CAP-PKG-05 — Removal and deprecation

### FR-PKG-051 — Uninstall with data retention
- **Statement:** Uninstalling a Buk MUST remove its behaviour (ports, sheets, subscriptions, schedules, views). Its data classes and data MUST be kept read-only until retention allows disposal.
- **Rationale:** Never lose records by uninstalling software.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an uninstalled Buk, when its records are read, then they are available read-only.
- **Verification:** CONF
- **Origin:** NEW

### FR-PKG-052 — Deprecation notices
- **Statement:** Publishers MUST be able to deprecate Buk versions with a message and an end-of-support date, shown to installers and administrators.
- **Rationale:** Lifecycle communication.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** EXC, STU
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a deprecated version, when installed, then a warning is shown.
- **Verification:** CONF
- **Origin:** NEW

### FR-PKG-053 — Dependency protection
- **Statement:** Removing a Buk that others depend on MUST be refused until the dependants are removed or upgraded.
- **Rationale:** Integrity.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a dependant, when removal is requested, then it fails with the dependants listed.
- **Verification:** CONF
- **Origin:** NEW

## CAP-PKG-06 — Verification

### FR-PKG-061 — Buk verification
- **Statement:** Buk verification MUST pass all of the following:
  1. Forge static checks, including documentation completeness and ontology lint;
  2. lens verification;
  3. all declared scenarios in a clean VAE;
  4. declared-capability checks;
  5. ABI compatibility.

  Verification MUST produce a signed record.
- **Rationale:** Only verified Buks reach production (EXT-RFC VERIFIED state).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG, EXC
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a Buk with a failing scenario, when verified, then the record lists the failure, and the status is "not verified".
- **Verification:** CONF, SCN
- **Origin:** EXT-RFC, L40:SPEC-28

### FR-PKG-062 — Production installs need verification
- **Statement:** Installing into `main` or `release` lines MUST require a verified Buk version. Unverified Buks MAY be installed only in sandbox VAEs.
- **Rationale:** Safety.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an unverified Buk, when installed into `prod`, then it is refused.
- **Verification:** CONF
- **Origin:** EXT-RFC

### FR-PKG-063 — Reproducible verification
- **Statement:** Anyone MUST be able to re-run verification from the archive and obtain the same result.
- **Rationale:** Trust in verification badges.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** FRG
- **Personas:** PER-ExchangeOperator
- **Acceptance:**
  1. Given an archive, when re-verified, then the record matches the original.
- **Verification:** CONF
- **Origin:** NEW

## CAP-PKG-07 — Genesis Buks

### FR-PKG-071 — Embedded genesis Buks
- **Statement:** Every node MUST embed the `system` and `ontology` Buks, signed by the platform publisher, and MUST install them on bootstrap (SCN-001).
- **Rationale:** Bootstrapping without external dependencies.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given an offline node, when bootstrapped, then both Buks are installed.
- **Verification:** SCN
- **Origin:** L40:SPEC-28
- **Phase note:** PH-1 step 16 (DEC-077): the genesis Buks have a PH-1 form, an embedded manifest signed by the platform publisher and installed by one signed commit on `main`; `ontology` carries the party–role core, and its full content (FR-MODEL-091) follows in PH-2 with the Buk format.

### FR-PKG-072 — Idempotent genesis
- **Statement:** Genesis installation MUST be idempotent and resumable (SCN-001 A1).
- **Rationale:** Robust setup.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given an interrupted genesis, when rerun, then no duplicates exist.
- **Verification:** FAULT
- **Origin:** L40:SPEC-28

### FR-PKG-073 — Genesis upgrades
- **Statement:** Genesis Buks MUST upgrade through the normal Buk upgrade process.
- **Rationale:** No special paths.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a new `system` Buk version, when a node upgrades, then it merges as an upgrade change set.
- **Verification:** SCN
- **Origin:** NEW

## CAP-PKG-08 — Licence hooks

### FR-PKG-081 — Licence-check logic
- **Statement:** Buks MAY declare licence-check logic and licensed features. The platform MUST evaluate entitlements at install time and at invocation time of licensed ports.
- **Rationale:** Smart licences (EXT-TRI).
- **Priority:** Must · **Phase:** PH-4 · **Systems:** DVM, EXC
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given an expired licence, when a licensed port is invoked, then it fails with `BILL.LICENCE_INACTIVE`.
- **Verification:** CONF
- **Origin:** EXT-TRI

### FR-PKG-082 — Data is never held hostage
- **Statement:** Licence enforcement MUST NOT block reading, exporting or deleting the customer's own data.
- **Rationale:** Ownership principle P6.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a lapsed licence, when the customer exports the Buk's data, then it succeeds.
- **Verification:** CONF
- **Origin:** NEW

### FR-PKG-083 — Offline licences
- **Statement:** Server and Box editions MUST support signed offline licence files with expiry and grace periods.
- **Rationale:** Air-gapped deployments.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an offline licence, when validated without a network, then it succeeds.
- **Verification:** CONF
- **Origin:** NEW

## CAP-INT-01 — UBTP protocol

### FR-INT-011 — Protocol messages
- **Statement:** UBTP MUST define the message types `Handshake`, `Mutate` (invoke ports and commits), `Query`, `Subscribe` (live results and events), `Emit` (server-to-client pushes) and `Sync` (from PH-3), with versioned schemas and error objects.
- **Rationale:** One protocol for every client (L40:SPEC-24).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, SDK, BPA
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the protocol vector suite, when run, then all messages validate against their schemas.
- **Verification:** CONF
- **Origin:** L40:SPEC-24

### FR-INT-012 — Bindings
- **Statement:** UBTP MUST have the bindings TCP, WebSocket, HTTP (request and response, with server-sent events for subscriptions) and in-process. A QUIC binding is added in PH-5.
- **Rationale:** Every host and network environment.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, SDK
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the scenario suite, when run over each binding, then the results are identical.
- **Verification:** SCN, CONF
- **Origin:** L40:SPEC-24, EXT-RFC

### FR-INT-013 — Version negotiation
- **Statement:** The handshake MUST negotiate protocol version, ABI version, features and authentication. Incompatible peers MUST fail with an explicit error.
- **Rationale:** Evolution without breakage.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, SDK
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a client requiring an unsupported version, when connecting, then the handshake fails with `INT.VERSION_UNSUPPORTED`.
- **Verification:** CONF
- **Origin:** L40:SPEC-24

### FR-INT-014 — Resumable subscriptions
- **Statement:** Subscriptions MUST deliver changes in commit order with resumable cursors. A client reconnecting with a cursor MUST receive every change it missed, within the retention window.
- **Rationale:** Live UIs and integrations over unreliable networks.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, SDK
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a disconnection during 50 changes, when resumed, then all 50 are delivered exactly once to the client.
- **Verification:** SIM, CONF
- **Origin:** L40:SPEC-24

### FR-INT-015 — Binding-independent semantics
- **Statement:** Message semantics, errors and authorization MUST be identical across bindings.
- **Rationale:** Portability.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, SDK, BPA
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given the cross-binding vectors, when compared, then they are identical.
- **Verification:** CONF
- **Origin:** L40:SPEC-24

## CAP-INT-02 — REST facade

### FR-INT-021 — Generated REST endpoints
- **Statement:** The platform MUST generate REST endpoints per class (read, query, create where permitted) and per port (invoke), mapped to UBTP semantics.
- **Rationale:** Integration with tools that expect REST.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, BRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a new port, when merged, then its endpoint is available without deployment.
- **Verification:** CONF
- **Origin:** IMP-12

### FR-INT-022 — OpenAPI documentation
- **Statement:** The REST facade MUST publish OpenAPI 3.1 documents per VAE (FR-MODEL-103) filtered by the caller's permissions.
- **Rationale:** Self-describing APIs.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a caller, when fetching the document, then only permitted operations are described.
- **Verification:** CONF
- **Origin:** IMP-12

### FR-INT-023 — Problem details
- **Statement:** REST errors MUST use RFC 9457 problem details, carrying the platform error code and explanation.
- **Rationale:** Standard error handling.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a validation failure, when returned, then the body is `application/problem+json` with the code.
- **Verification:** CONF
- **Origin:** IMP-12

## CAP-INT-03 — Webhooks

### FR-INT-031 — Signed outbound webhooks
- **Statement:** Outbound webhooks MUST be signed with HMAC-SHA256 over timestamp and body, MUST include event IDs, and MUST retry with backoff.
- **Rationale:** Secure integration.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, BRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a receiver verifying signatures, when a webhook arrives, then the signature validates, and replays older than the tolerance are rejectable.
- **Verification:** CONF, SEC
- **Origin:** L40:SPEC-20

### FR-INT-032 — Verified inbound webhooks
- **Statement:** Inbound webhook endpoints MUST verify signatures or tokens, map payloads to ports through declared mappings, and deduplicate by event ID.
- **Rationale:** Safe ingestion.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, BRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given an unsigned request, when received, then it is rejected, and a security event is logged.
- **Verification:** SEC
- **Origin:** NEW

### FR-INT-033 — Webhook management
- **Statement:** Administrators MUST be able to view webhook deliveries, failures and retries, and to re-drive failed deliveries.
- **Rationale:** Operability.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** CTL, STU
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a failed delivery, when re-driven, then a new attempt is logged.
- **Verification:** SCN
- **Origin:** NEW

## CAP-INT-04 — Connector framework

### FR-INT-041 — Connector types
- **Statement:** The connector framework MUST support HTTP (REST and SOAP), SFTP, SMTP and IMAP, Kafka-compatible brokers and cloud queues, and read-only relational database access, each with typed operations.
- **Rationale:** Real-world integration (SCN-108).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG, DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given an SFTP connector, when polled, then new files are ingested and recorded.
- **Verification:** CONF, SCN
- **Origin:** L40:SPEC-17

### FR-INT-042 — Mapping definitions
- **Statement:** Mappings between external payloads and classes MUST be versioned declarative objects with L1 transformations, and MUST be testable with samples.
- **Rationale:** Maintainable integration.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given a sample statement file, when the mapping test runs, then the expected lines are produced.
- **Verification:** CONF
- **Origin:** NEW

### FR-INT-043 — Rate limits and quotas
- **Statement:** Connectors MUST respect declared rate limits and external quotas, queueing or failing predictably.
- **Rationale:** Good citizenship with external APIs.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given 10 requests per second allowed, when 50 are issued, then the throughput stays at or below 10 per second.
- **Verification:** CONF
- **Origin:** NEW

### FR-INT-044 — Credential lifecycle
- **Statement:** Connector credentials MUST be managed as secrets (FR-IAM-091), with rotation reminders and test-connection operations.
- **Rationale:** Operability and security.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG, NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an expiring credential, when 14 days remain, then administrators are notified.
- **Verification:** CONF
- **Origin:** NEW

## CAP-INT-05 — Event streaming out

### FR-INT-051 — Publish to external brokers
- **Statement:** Subscriptions MUST be able to publish CloudEvents to Kafka-compatible brokers and major cloud queues, with the object ID as the partition key.
- **Rationale:** Enterprise event backbones.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-DataAnalyst, PER-LogicDeveloper
- **Acceptance:**
  1. Given a Kafka sink, when events flow, then per-object order is preserved in the partition.
- **Verification:** CONF
- **Origin:** IMP-12

### FR-INT-052 — Delivery guarantees
- **Statement:** External publication MUST be at-least-once, with event IDs for consumer deduplication.
- **Rationale:** Reliable streaming.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given a broker outage, when it recovers, then every event is delivered.
- **Verification:** FAULT
- **Origin:** NEW

### FR-INT-053 — Schema export
- **Statement:** Event schemas MUST be exportable to schema registries (JSON Schema).
- **Rationale:** Consumer contracts.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-DataAnalyst
- **Acceptance:**
  1. Given an event type, when exported, then its schema validates the published events.
- **Verification:** CONF
- **Origin:** IMP-12

## CAP-INT-06 — E-mail and chat

### FR-INT-061 — Inbound e-mail to records
- **Statement:** VAEs MUST be able to receive e-mail at configured addresses, parse it into objects or threads by rules, and store attachments as files.
- **Rationale:** Business still runs on e-mail.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-InvestorServicesClerk
- **Acceptance:**
  1. Given a dealing request e-mail, when received, then a draft order is created with the attachment linked.
- **Verification:** SCN
- **Origin:** NEW

### FR-INT-062 — Chat integration
- **Statement:** Action Messages and notifications MUST be deliverable to Slack and Microsoft Teams, with in-chat actions following FR-OFFICE-062.
- **Rationale:** Work where people are.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** BRG, WSP
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given an approval in Teams, when approved in chat, then step-up authentication and governance apply.
- **Verification:** SCN, SEC
- **Origin:** EXT-WP

### FR-INT-063 — Outbound e-mail templates
- **Statement:** Outbound e-mail MUST use versioned templates with bindings, and MUST be sent through configured providers with delivery tracking.
- **Rationale:** Consistent communication.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** BRG
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a template change, when merged, then the next e-mails use it.
- **Verification:** CONF
- **Origin:** NEW

## CAP-INT-07 — Financial adapters

### FR-INT-071 — ISO 20022 statements
- **Statement:** The Bridge MUST parse ISO 20022 `camt.053` and `camt.054` into statement ledger entries, with file hashes recorded.
- **Rationale:** Custodian and bank reconciliation (SCN-108).
- **Priority:** Must · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given sample camt.053 files, when ingested, then the lines match the file totals.
- **Verification:** CONF
- **Origin:** NEW

### FR-INT-072 — Legacy bank formats
- **Statement:** The Bridge SHOULD parse SWIFT MT940 statements delivered as files.
- **Rationale:** Still widely used.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given an MT940 sample, when ingested, then the balances reconcile to the file.
- **Verification:** CONF
- **Origin:** NEW

### FR-INT-073 — Generic fund-platform files
- **Statement:** The Bridge MUST support configurable CSV and fixed-width file adapters with mapping definitions for transfer-agent, custodian and pricing feeds.
- **Rationale:** Heterogeneous fund ecosystems.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given a price-feed CSV mapping, when a file arrives, then prices are committed with the source recorded.
- **Verification:** CONF
- **Origin:** NEW

### FR-INT-074 — Outbound payment files
- **Statement:** The Bridge SHOULD generate ISO 20022 `pain.001` payment initiation files from approved payment objects, with the file hash recorded against each payment.
- **Rationale:** Straight-through settlement.
- **Priority:** Could · **Phase:** PH-3 · **Systems:** BRG
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given 20 approved payments, when a file is generated, then it validates against the schema and references each payment.
- **Verification:** CONF
- **Origin:** NEW

## CAP-INT-08 — API credentials

### FR-INT-081 — OAuth client credentials
- **Statement:** Service integrations MUST authenticate with OAuth 2.1 client credentials or API keys bound to service principals.
- **Rationale:** Standard machine authentication.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given client credentials, when a token is requested, then it carries the configured scopes.
- **Verification:** CONF, SEC
- **Origin:** IMP-12

### FR-INT-082 — Scopes
- **Statement:** Credentials MUST carry scopes that restrict the classes, ports and VAEs they can use, in addition to the principal's policies.
- **Rationale:** Defence in depth.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a read-only scope, when a write is attempted, then it is denied.
- **Verification:** SEC
- **Origin:** NEW

### FR-INT-083 — Rotation without downtime
- **Statement:** Credentials MUST support overlapping validity for rotation.
- **Rationale:** Zero-downtime rotation.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given two valid credentials during rotation, when either is used, then it succeeds until the old one expires.
- **Verification:** CONF
- **Origin:** NEW
