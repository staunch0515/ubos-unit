---
id: UBS-REQ-05-09
title: Functional Requirements — IAM (Identity and Access) and TEN (Tenancy and VAEs)
status: complete
phase: PH-1
depends_on: [UBS-REQ-05, UBS-REQ-05-01]
---

# Functional Requirements — IAM and TEN

## CAP-IAM-01 — Local authentication

### FR-IAM-011 — Local credentials
- **Statement:** Box and Server editions MUST support local authentication with passwords (stored with Argon2id), passkeys (WebAuthn) and TOTP second factors. Organisations MAY disable any local method when SSO is enabled.
- **Rationale:** Personal and small installations need identity without an external provider.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PersonalUser, PER-TenantAdministrator
- **Acceptance:**
  1. Given a new Box, when the owner registers a passkey, then subsequent logins succeed with it, and no password is required.
- **Verification:** CONF, SEC
- **Origin:** L40:SPEC-22

### FR-IAM-012 — API keys
- **Statement:** The platform MUST issue API keys that are scoped to principals and permissions, shown once, stored only as salted hashes, and given expiry and rotation dates.
- **Rationale:** Machine access with least privilege.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, CTL
- **Personas:** PER-TenantAdministrator, PER-LogicDeveloper
- **Acceptance:**
  1. Given a new key, when listed later, then only its prefix and metadata are visible.
- **Verification:** CONF, SEC
- **Origin:** L40:SPEC-22

### FR-IAM-013 — Brute-force protection
- **Statement:** Authentication endpoints MUST rate-limit by principal and source, and MUST lock or throttle after configurable failures with alerts.
- **Rationale:** Credential attacks.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given 10 failures in 1 minute, when the 11th attempt arrives, then it is throttled, and a security event is logged.
- **Verification:** SEC
- **Origin:** NEW

### FR-IAM-014 — Local unlock on Box
- **Statement:** The Box MUST protect its local keys with the operating system's key store and MUST require user unlock after a configurable idle time.
- **Rationale:** Device theft protection.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-PersonalUser, PER-FieldWorker
- **Acceptance:**
  1. Given the idle time elapsed, when the app resumes, then unlock is required before data is readable.
- **Verification:** SEC
- **Origin:** NEW
- **Phase note:** moved to PH-4 by DEC-024 and DEC-025 (SQLite, Box, devices and sync).

## CAP-IAM-02 — Enterprise SSO

### FR-IAM-021 — OIDC federation
- **Statement:** The platform MUST federate with OpenID Connect identity providers, including PKCE and discovery, and MUST map claims to principals and groups.
- **Rationale:** Enterprise identity.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a configured provider, when a user signs in, then the principal is resolved, and the group claims map to roles.
- **Verification:** CONF, SEC
- **Origin:** IMP-12

### FR-IAM-022 — SAML federation
- **Statement:** The platform MUST support SAML 2.0 service-provider-initiated login with signed assertions.
- **Rationale:** Many enterprises and regulated firms still use SAML.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an unsigned assertion, when presented, then login fails.
- **Verification:** SEC
- **Origin:** NEW

### FR-IAM-023 — Just-in-time provisioning
- **Statement:** First SSO login MAY create the principal just in time, with roles derived from group mappings. Unmapped users MUST receive no roles.
- **Rationale:** Low admin overhead with default deny.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an unmapped user, when logging in, then the user sees an empty workspace with "no access" guidance.
- **Verification:** CONF
- **Origin:** NEW

### FR-IAM-024 — Step-up authentication
- **Statement:** Policies MUST be able to require a stronger authentication level (for example a phishing-resistant factor within the last 5 minutes) for specific actions. The platform MUST trigger step-up and continue the action after success.
- **Rationale:** Approvals and signatures need strong assurance.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, WSP
- **Personas:** PER-Approver, PER-SecurityOfficer
- **Acceptance:**
  1. Given a policy requiring passkey step-up for approvals over 1M, when approving with a password session, then step-up is requested before the approval commits.
- **Verification:** SCN, SEC
- **Origin:** NEW

## CAP-IAM-03 — Principals and keys

### FR-IAM-031 — Principal kinds
- **Statement:** The platform MUST distinguish the principal kinds `human`, `service`, `agent`, `external` and `node`. Policies MUST be able to condition on the kind.
- **Rationale:** Different risk profiles.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a policy "agents cannot approve", when an agent calls approve, then it is denied.
- **Verification:** CONF
- **Origin:** L40:SPEC-22, IMP-10

### FR-IAM-032 — Decentralised identifiers and signing keys
- **Statement:** Every principal MUST have a DID of the form `did:ubos:<method-specific-id>`, derived from or bound to an Ed25519 public key. Every principal that commits MUST have a signing key pair.
- **Rationale:** Signed commits and federation-ready identity (EXT-RFC 5.3).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer, PER-KernelEngineer
- **Acceptance:**
  1. Given a principal, when its DID document is resolved locally, then it lists the current verification key.
- **Verification:** CONF
- **Origin:** EXT-RFC

### FR-IAM-033 — Key rotation and revocation
- **Statement:** Keys MUST be rotatable and revocable. The DID history MUST keep old keys with their validity periods, so that signatures made while a key was valid remain verifiable.
- **Rationale:** Long-lived evidence.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a key rotated in 2026, when verifying a 2025 commit, then verification succeeds with the old key.
  2. Given a commit signed with a revoked key after its revocation time, when verified, then it fails.
- **Verification:** CONF, SEC
- **Origin:** EXT-RFC

### FR-IAM-034 — Protected key storage
- **Statement:** Signing keys of human principals MUST stay on their devices or in a platform key service. Server-held keys MUST be protected by an HSM or cloud KMS in Server and Cell editions. Private keys MUST never be exported in plaintext.
- **Rationale:** Non-repudiation depends on key custody.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the admin API, when enumerated, then no operation returns a private key.
- **Verification:** SEC, INSP
- **Origin:** NEW

### FR-IAM-035 — Principal lifecycle
- **Statement:** Principals MUST have the states `active`, `suspended` and `deactivated`. Suspended and deactivated principals MUST be denied every action, and their signatures before deactivation MUST remain valid.
- **Rationale:** Offboarding without losing evidence.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a deactivated principal, when any request arrives, then it is rejected.
- **Verification:** CONF
- **Origin:** NEW

## CAP-IAM-04 — Policy authorization

### FR-IAM-041 — Default deny at every enforcement point
- **Statement:** Every enforcement point MUST call the policy decision point. Anything not explicitly allowed MUST be denied. The enforcement points are:
  - instructions and UBTP operations;
  - UI data payloads, exports, sync and agent tools;
  - webhooks and administrative operations.
- **Rationale:** Complete mediation (L40 hard rule).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD, SDK
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a principal without policies, when any operation is attempted, then it is denied.
- **Verification:** SEC, CONF
- **Origin:** L40:SPEC-22

### FR-IAM-042 — Analysable policy language
- **Statement:** Policies MUST be written in an analysable policy language (Cedar-class) supporting roles, principal and resource attributes, context conditions (time, channel, authentication level, branch kind) and explicit `forbid` rules that override `permit`.
- **Rationale:** Policies must be verifiable and explainable (IMP-06).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given `forbid` for agents on `FeeSchedule` writes and a `permit` for the role they hold, when evaluated, then the result is deny.
- **Verification:** CONF, PROP
- **Origin:** IMP-06

### FR-IAM-043 — Not visible means not found
- **Statement:** A resource the principal may not see MUST be reported as not found (`IAM.NOT_FOUND`), never as forbidden, in every channel.
- **Rationale:** No existence leakage (L40 hard rule).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a hidden object, when read by URI, then the response is identical to that for a non-existent object.
- **Verification:** SEC
- **Origin:** L40:SPEC-02

### FR-IAM-044 — Policies are governed objects
- **Statement:** Policies MUST be versioned Definition objects under `change_set` write policy, with tests, and with simulation against recorded decisions before publication.
- **Rationale:** Access changes are high-risk changes.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a policy change, when simulated against the last 7 days of decisions, then the decisions that would change are listed.
- **Verification:** CONF
- **Origin:** NEW

### FR-IAM-045 — Decision logging
- **Statement:** Authorization decisions for sensitive resources, denials and administrative actions MUST be logged with principal, action, resource, result and the determining policies. Other decisions MAY be sampled.
- **Rationale:** Investigation and access reviews.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer, PER-InternalAuditor
- **Acceptance:**
  1. Given a denial, when the log is queried, then the determining policy is recorded.
- **Verification:** CONF
- **Origin:** NEW

## CAP-IAM-05 — Relationship authorization

### FR-IAM-051 — Authorization relations from the model
- **Statement:** Classes MUST be able to declare authorization relations (for example `owner`, `member`, `manager_of`, `counterparty_of`) derived from model relationships. The platform MUST maintain them as a relationship graph.
- **Rationale:** Most business permissions are relational (IMP-06).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect, PER-SecurityOfficer
- **Acceptance:**
  1. Given `Agreement.owner → Person`, when the owner changes, then the relation graph reflects it at the same commit.
- **Verification:** CONF
- **Origin:** IMP-06

### FR-IAM-052 — Relation-based rules
- **Statement:** Policies MUST be able to reference relations and their composition (for example "approver ∈ manager_of(maker)" or "viewer ∈ member(team(owner))") with bounded depth.
- **Rationale:** Expressive, maintainable permissions.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the rule "managers of the owner can view", when a manager views, then it is allowed. A peer is denied.
- **Verification:** CONF, PROP
- **Origin:** IMP-06

### FR-IAM-053 — Consistent relationship checks
- **Statement:** Relationship checks MUST evaluate against the same snapshot as the operation, or against a snapshot no older than the operation's consistency token.
- **Rationale:** The "new enemy" problem: revoked access must not be honoured.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a membership removal committed, when a subsequent request carries the new token, then access is denied.
- **Verification:** SIM, SEC
- **Origin:** IMP-06

## CAP-IAM-06 — Masking and filtering

### FR-IAM-061 — Field masking
- **Statement:** Policies MUST be able to mask fields by classification or by explicit rule, with the modes `hide`, `partial` (for example the last 4 characters) and `hash`. Masking MUST apply in every output: API, UI, exports, logs, events, AI prompts and diffs.
- **Rationale:** Least exposure of personal and confidential data.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given `tax_id` masked `partial`, when read through the API, the export and an agent tool, then all three show the same masked form.
- **Verification:** CONF, SEC
- **Origin:** L40:SPEC-22

### FR-IAM-062 — Row filtering
- **Statement:** Policies MUST be able to restrict visible objects by conditions, and filters MUST be applied inside query execution, not after it.
- **Rationale:** Correct counts and performance.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given "region = user.region", when a user lists customers, then only same-region rows are returned, and the counts agree.
- **Verification:** CONF, SEC
- **Origin:** L40:SPEC-22

### FR-IAM-063 — Masked event payloads
- **Statement:** Event payloads delivered to subscribers whose owners lack clearance MUST be masked by the same rules.
- **Rationale:** Events are an output channel.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a subscriber without `personal` clearance, when a `Person` update event is delivered, then the personal fields are masked.
- **Verification:** SEC
- **Origin:** NEW

## CAP-IAM-07 — Provisioning

### FR-IAM-071 — SCIM 2.0
- **Statement:** The platform MUST expose SCIM 2.0 endpoints for users and groups, including create, update, deactivate and group membership.
- **Rationale:** Automated joiner–mover–leaver.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given the SCIM compliance test suite, when run, then the supported operations pass.
- **Verification:** CONF
- **Origin:** IMP-12

### FR-IAM-072 — Deprovisioning propagation
- **Statement:** Deactivation MUST revoke sessions, tokens and delegations, and MUST reassign or re-route pending tasks and approvals within the NR-SEC propagation bound.
- **Rationale:** SCN-412.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, DVM
- **Personas:** PER-TenantAdministrator, PER-SecurityOfficer
- **Acceptance:**
  1. Given SCN-412, when E is deactivated, then no request by E succeeds after the bound.
- **Verification:** SCN, SEC
- **Origin:** NEW

### FR-IAM-073 — Group-to-role mapping
- **Statement:** Group-to-role mappings MUST be governed objects, and changes to them MUST be audited.
- **Rationale:** Mappings are privilege grants.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a mapping change, when audited, then the actor and the before and after mappings are recorded.
- **Verification:** CONF
- **Origin:** NEW

## CAP-IAM-08 — Policy analysis and access review

### FR-IAM-081 — Who-can and what-can queries
- **Statement:** The platform MUST answer "which principals can perform action A on resource R" and "what can principal P do in VAE V", using policy analysis over roles, relations and conditions. Conditional results MUST state their conditions.
- **Rationale:** Least-privilege evidence (PER-SecurityOfficer success signal).
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, CTL
- **Personas:** PER-SecurityOfficer, PER-InternalAuditor
- **Acceptance:**
  1. Given a fee schedule, when "who can approve" is asked, then every eligible principal is listed with the granting path.
- **Verification:** CONF, PROP
- **Origin:** IMP-06

### FR-IAM-082 — Access review campaigns
- **Statement:** The platform MUST run access review campaigns in which reviewers confirm or revoke grants, and MUST produce a signed completion record.
- **Rationale:** SOX and SOC 2 control evidence (SCN-412).
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, WSP
- **Personas:** PER-SecurityOfficer, PER-Approver
- **Acceptance:**
  1. Given a campaign, when all items are decided, then the revocations are applied, and the record is signed.
- **Verification:** SCN, AUDIT
- **Origin:** NEW

### FR-IAM-083 — Unused permission detection
- **Statement:** The platform SHOULD report grants not used within a configurable period.
- **Rationale:** Privilege reduction.
- **Priority:** Should · **Phase:** PH-3 · **Systems:** DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a role unused for 90 days, when reported, then it is listed with its holders.
- **Verification:** CONF
- **Origin:** NEW

## CAP-IAM-09 — Secrets and keys

### FR-IAM-091 — Secret store and handles
- **Statement:** Secrets (connector credentials, webhook secrets, model API keys) MUST be stored in a secret store and referenced by handles. Logic, agents, views and exports MUST NOT be able to read secret values.
- **Rationale:** Secrets never enter business-logic memory.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a connector using a handle, when logic inspects the connector object, then only the handle is visible.
- **Verification:** SEC
- **Origin:** L40:SPEC-22

### FR-IAM-092 — Envelope encryption per tenant
- **Statement:** Authoritative data at rest MUST be encrypted with per-tenant data keys wrapped by a key-encryption key in the platform KMS. Personal fields MUST additionally use per-subject keys (FR-PROOF-071).
- **Rationale:** Isolation and crypto-shredding.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, DVM
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given raw storage access, when a tenant's rows are read without keys, then the content is ciphertext.
- **Verification:** SEC, INSP
- **Origin:** NEW

### FR-IAM-093 — Customer-managed keys
- **Statement:** Server and Cell editions SHOULD support customer-managed key-encryption keys (BYOK/HYOK). Revoking the customer key MUST render the tenant's data unreadable.
- **Rationale:** Enterprise and regulator expectations.
- **Priority:** Should · **Phase:** PH-4 · **Systems:** NOD, CTL
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a revoked customer key, when the tenant is accessed, then reads fail with `IAM.KEY_UNAVAILABLE`.
- **Verification:** SEC
- **Origin:** NEW

### FR-IAM-094 — Rotation
- **Statement:** Data keys, key-encryption keys and secrets MUST be rotatable without downtime, and rotation MUST be audited.
- **Rationale:** Key hygiene.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a key-encryption-key rotation, when complete, then all data keys are rewrapped, and service was uninterrupted.
- **Verification:** SEC, FAULT
- **Origin:** NEW

## CAP-IAM-10 — Sessions and devices

### FR-IAM-101 — Session policies
- **Statement:** Session lifetime, idle timeout, concurrent-session limits and re-authentication rules MUST be configurable per tenant and principal kind.
- **Rationale:** Security posture varies by organisation.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given an idle timeout of 15 minutes, when idle for 16 minutes, then the next request requires re-authentication.
- **Verification:** SEC
- **Origin:** NEW

### FR-IAM-102 — Immediate revocation
- **Statement:** Revoking a session or token MUST take effect across all nodes within the NR-SEC propagation bound.
- **Rationale:** Incident containment.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a revoked session, when used on another node after the bound, then it is rejected.
- **Verification:** SEC, SIM
- **Origin:** NEW

### FR-IAM-103 — Device registration and remote wipe
- **Statement:** Devices that replicate data (Box, mobile) MUST be registered to principals. Administrators MUST be able to revoke a device. Revocation MUST destroy the device's local data keys at its next contact, and the replicated data MUST become unreadable.
- **Rationale:** Lost devices.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** NOD
- **Personas:** PER-SecurityOfficer, PER-FieldWorker
- **Acceptance:**
  1. Given a revoked tablet, when it next connects, then the local keys are destroyed, and the UI shows "device revoked".
- **Verification:** SEC
- **Origin:** NEW
- **Phase note:** moved to PH-4 by DEC-024 and DEC-025 (SQLite, Box, devices and sync).

### FR-IAM-104 — Delegated client authorization
- **Statement:** External clients, including MCP clients, MUST obtain OAuth 2.1 authorization with explicit scopes, expiry and a named delegating user. Their calls MUST be attributed as "client on behalf of user".
- **Rationale:** SCN-305.
- **Priority:** Must · **Phase:** PH-3 · **Systems:** NOD, AGT
- **Personas:** PER-BusinessUser, PER-AiAgent
- **Acceptance:**
  1. Given SCN-305, when the delegation expires, then calls fail with `IAM.DELEGATION_EXPIRED`.
- **Verification:** SEC, SCN
- **Origin:** IMP-10, IMP-12

## CAP-TEN-01 — Tenants and VAEs

### FR-TEN-011 — Tenant objects
- **Statement:** A tenant MUST be represented with legal owner, plan, home region, key references, contacts and state. Every authoritative record MUST be attributable to exactly one tenant.
- **Rationale:** The unit of ownership, isolation and billing.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD, DVM
- **Personas:** PER-TenantAdministrator, PER-PlatformOperator
- **Acceptance:**
  1. Given any stored object, when inspected at storage level, then its tenant is identifiable.
- **Verification:** CONF, INSP
- **Origin:** L40:SPEC-23

### FR-TEN-012 — VAEs as complete logical worlds
- **Statement:** A tenant MUST own one or more VAEs. Each VAE MUST have its own branches, installed Buks, principals' role assignments, configuration, business time zone and base currency.
- **Rationale:** EXT-TRI "a VAE is a complete, self-contained logical world".
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given VAEs `prod` and `uat` in one tenant, when a Buk is installed in `uat`, then `prod` is unaffected.
- **Verification:** CONF
- **Origin:** EXT-TRI, EXT-RFC

### FR-TEN-013 — VAE addressing authority
- **Statement:** The URI authority of a VAE MUST be `<tenant>.<vae>` for non-default VAEs and `<tenant>` for the default VAE. VAE identifiers MUST NOT exceed 255 characters.
- **Rationale:** Stable, location-independent addresses (EXT-RFC 2.1).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, BPA
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given `ubos://acme.uat/Fund/geq`, when resolved, then it refers to the `uat` VAE of tenant `acme`.
- **Verification:** CONF
- **Origin:** EXT-RFC, L40:SPEC-12

## CAP-TEN-02 — Isolation

### FR-TEN-021 — Storage isolation
- **Statement:** Every storage access MUST be scoped by tenant at the storage layer: row-level security or equivalent in shared databases. Server and Cell editions MUST also support a database-per-tenant placement.
- **Rationale:** Defence in depth.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a deliberately unscoped query injected in a test build, when executed, then the storage layer returns no foreign rows.
- **Verification:** SEC
- **Origin:** L40:SPEC-23

### FR-TEN-022 — Isolation of derived stores
- **Statement:** Caches, queues, search indexes, vector indexes, projections, logs and metrics MUST be partitioned or labelled by tenant, and MUST never return another tenant's data.
- **Rationale:** Derived stores are common leakage paths.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given the cross-tenant isolation suite, when run against every derived store, then no leakage occurs.
- **Verification:** SEC, SIM
- **Origin:** L40:SPEC-23

### FR-TEN-023 — Isolation test suite
- **Statement:** The platform MUST maintain an automated cross-tenant isolation suite that covers every API, instruction and derived store, and it MUST run on every release.
- **Rationale:** Isolation must be continuously proven.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-SecurityOfficer
- **Acceptance:**
  1. Given a release candidate, when the suite runs, then it passes 100%.
- **Verification:** SEC, INSP
- **Origin:** NEW

### FR-TEN-024 — Resource fairness
- **Statement:** Per-tenant quotas and rate limits MUST bound compute, storage growth, jobs, connector calls and AI usage, so that one tenant cannot degrade others beyond the NR-PERF isolation bounds.
- **Rationale:** Noisy neighbours.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given one tenant generating 10× normal load, when the others are measured, then their latency stays within bounds.
- **Verification:** BENCH
- **Origin:** NEW

## CAP-TEN-03 — Overlays

### FR-TEN-031 — Overlay branches for customisation
- **Statement:** A tenant's customisation of a vendor or platform line MUST live on an overlay branch that inherits from the vendor line (CAP-VER-08), holding only the tenant's changes.
- **Rationale:** Solving the multi-tenant customisation dilemma (EXT-TRI).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given an overlay with 3 changes, when its storage is measured, then only 3 object versions are held locally.
- **Verification:** CONF
- **Origin:** EXT-TRI
- **Phase note:** PH-2 step 11 (DEC-087): moved from PH-1; its design item DSN-NOD-407 is PH-2. The kernel's overlay branches exist since PH-1 step 13.

### FR-TEN-032 — Upgrades merge into overlays
- **Statement:** Vendor releases MUST be applied to overlays by merge or by pin update, with conflicts reported per overlay (SCN-013).
- **Rationale:** Upgrades without lost customisations.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given SCN-013, when executed, then the outcomes match.
- **Verification:** SCN
- **Origin:** EXT-TRI

### FR-TEN-033 — Overlay policies
- **Statement:** A vendor MUST be able to declare, per class and member, whether overlays may override it (`overridable`, `extend_only`, `locked`). The DVM MUST enforce these declarations.
- **Rationale:** Vendors protect invariants that their logic relies on.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a `locked` fee-calculation asset, when a tenant overrides it, then the commit fails with `TEN.OVERRIDE_LOCKED`.
- **Verification:** CONF
- **Origin:** L40:SPEC-23

## CAP-TEN-04 — VAE hierarchy on one node

### FR-TEN-041 — Parent and child VAEs
- **Statement:** A VAE MAY have a parent VAE in the same tenant. The child MUST inherit definitions, logic, sheets and views from the parent line. Business data MUST remain in each VAE unless it is explicitly published.
- **Rationale:** Headquarters–subsidiary structures (EXT-TRI "parent–child hierarchy").
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given HQ logic updated, when the child resolves it without an override, then the child uses the new version, and HQ cannot read the child's records.
- **Verification:** CONF, SEC
- **Origin:** EXT-TRI, EXT-WP

### FR-TEN-042 — Cascading policies
- **Statement:** Sheets and policies at the parent MAY be declared `cascading` and then apply in child VAEs at the parent layer. Locked declarations cannot be overridden in children.
- **Rationale:** Group-wide controls.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a cascading locked control at HQ, when a child tries to override it, then the child commit fails.
- **Verification:** CONF
- **Origin:** EXT-TRI

### FR-TEN-043 — Aggregated queries across children
- **Statement:** A parent VAE MUST be able to run declared aggregate queries across child VAEs that return only aggregate results allowed by each child's publication policy.
- **Rationale:** Group reporting without centralising raw data (SCN-408).
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-Executive, PER-TenantAdministrator
- **Acceptance:**
  1. Given a child publishing only monthly totals, when HQ queries raw rows, then it is denied. When HQ asks for totals, they are returned.
- **Verification:** CONF, SEC
- **Origin:** EXT-TRI

### FR-TEN-044 — Child autonomy settings
- **Statement:** Each child VAE MUST declare whether it follows the parent `live` or `pinned`, and which layers it may customise.
- **Rationale:** Subsidiaries differ in autonomy.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a pinned child, when the parent publishes, then the child is unchanged until its pin is moved.
- **Verification:** CONF
- **Origin:** NEW

## CAP-TEN-05 — Tenant lifecycle

### FR-TEN-051 — Tenant creation
- **Statement:** Creating a tenant MUST provision keys, the default VAE, the administrator principal and the genesis Buks (SCN-001) in one idempotent operation.
- **Rationale:** Repeatable onboarding.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a creation retried after a failure, when complete, then exactly one tenant exists.
- **Verification:** CONF, FAULT
- **Origin:** L40:SPEC-28

### FR-TEN-052 — Suspension
- **Statement:** A tenant MUST be suspendable in two modes: `read_only` (reads and exports allowed) and `locked` (only administrator export allowed).
- **Rationale:** Commercial or security holds without data loss.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given `read_only`, when a user commits, then it fails with `TEN.SUSPENDED`. Reads succeed.
- **Verification:** CONF
- **Origin:** NEW

### FR-TEN-053 — Archive and deletion
- **Statement:** Tenant deletion MUST honour retention and legal holds, MUST destroy the tenant's keys, and MUST issue a signed deletion certificate listing scope and time.
- **Rationale:** Contractual and privacy obligations.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, CTL
- **Personas:** PER-TenantAdministrator, PER-ComplianceOfficer
- **Acceptance:**
  1. Given a tenant under hold, when deletion is requested, then it is refused. After release it completes with a certificate.
- **Verification:** CONF, AUDIT
- **Origin:** NEW

### FR-TEN-054 — Tenant configuration
- **Statement:** Tenant and VAE configuration (locale defaults, time zone, base currency, security settings, retention defaults) MUST be versioned objects under governance.
- **Rationale:** Configuration is data.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a time-zone change, when inspected, then the change set and approval are recorded.
- **Verification:** CONF
- **Origin:** EXT-TRI

## CAP-TEN-06 — Portability

### FR-TEN-061 — Full export
- **Statement:** A tenant administrator MUST be able to export all data of a VAE or tenant in an open, documented format. The export MUST include:
  - objects and every version, ledgers, classes and logic;
  - files, signatures and proofs;
  - a manifest with a signature over the export root.
- **Rationale:** Ownership and no lock-in (principle P6).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** NOD, DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an export, when verified with the open verifier, then every object hash and signature checks out.
- **Verification:** CONF
- **Origin:** EXT-TRI (format trap), IMP-12

### FR-TEN-062 — Import into any conforming implementation
- **Statement:** An export MUST be importable into another conforming BPA implementation, producing identical head roots.
- **Rationale:** Portability proves the standard.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** BPA, NOD
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given an export from the reference DVM, when imported into a certified third-party implementation, then the head roots match.
- **Verification:** CONF
- **Origin:** EXT-BPU

### FR-TEN-063 — Scoped export
- **Statement:** Exports MUST be restrictable by classes, branches, time ranges and valid-time ranges, with permission checks.
- **Rationale:** Partial handovers and regulator requests.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a scope "Fund classes, 2026", when exported, then only those objects and versions are included.
- **Verification:** CONF
- **Origin:** NEW

## CAP-TEN-07 — Placement and migration

### FR-TEN-071 — Placement policies
- **Statement:** The Control Plane MUST place each tenant according to a policy covering region, residency constraints, dedicated or shared cell, and size class.
- **Rationale:** Residency and scale (IMP-08).
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given a tenant requiring US residency, when placed, then only US cells are candidates.
- **Verification:** CONF
- **Origin:** IMP-08

### FR-TEN-072 — Online migration between cells
- **Statement:** Tenants MUST be movable between cells while online. Data is copied by Merkle difference, the final cut-over pauses writes for at most the NR-AVAIL bound, and head roots are verified before routing switches (SCN-404).
- **Rationale:** Rebalancing without downtime.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL, NOD
- **Personas:** PER-PlatformOperator
- **Acceptance:**
  1. Given SCN-404, when executed, then the roots match, and the write pause is within the bound.
- **Verification:** SCN, FAULT
- **Origin:** IMP-08

### FR-TEN-073 — Residency enforcement
- **Statement:** Data of a tenant with residency constraints MUST NOT be stored, cached, processed by workers or sent to model providers outside the permitted regions.
- **Rationale:** Contractual and legal residency.
- **Priority:** Must · **Phase:** PH-4 · **Systems:** CTL, NOD, AGT
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a US-only tenant, when a model route outside the US is configured, then the configuration is rejected.
- **Verification:** CONF, AUDIT
- **Origin:** NEW
