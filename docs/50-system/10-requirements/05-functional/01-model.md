---
id: UBS-REQ-05-01
title: Functional Requirements — MODEL (Classes, Inheritance, Polymorphism)
status: complete
phase: PH-1
depends_on: [UBS-REQ-05, UBS-REQ-03]
---

# Functional Requirements — MODEL

## CAP-MODEL-01 — Class definition

### FR-MODEL-011 — Classes are versioned objects
- **Statement:** The DVM MUST store every class definition as a versioned object of the meta-class `Class`, created and changed only by commits, with the same history, branch, diff, merge and proof behaviour as business objects.
- **Rationale:** Data–logic isomorphism: the model evolves under the same governance as data (EXT-TRI Book II).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect, PER-KernelEngineer
- **Acceptance:**
  1. Given a class `Agreement` created on a draft branch, when the draft is merged, then `Agreement` appears in the history of `main` as a commit with author, process and signature.
  2. Given a class changed on `main`, when it is read at the previous commit, then the previous definition is returned.
- **Verification:** CONF, SCN
- **Origin:** L40:SPEC-13, EXT-TRI, EXT-BPU

### FR-MODEL-012 — Mandatory class declaration content
- **Statement:** A class MUST declare `name`, `namespace`, `kind`, `parent`, `fields`, `label`, `description` and `schema_version`. It MAY declare `traits`, `keys`, `ports`, `lifecycle`, `views`, `knowledge`, `projections`, `classification_default` and `abstract`.
- **Rationale:** A fixed, complete declaration lets tools, AI and conformance vectors reason about any class.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, FRG
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a class declaration without `kind`, when it is committed, then the commit is rejected with `MODEL.DECLARATION_INCOMPLETE`, naming the missing members.
  2. Given a complete declaration, when it is exported (FR-MODEL-102), then every declared member appears in the export.
- **Verification:** CONF
- **Origin:** L40:SPEC-13, IMP-01

### FR-MODEL-013 — Natural keys and slugs
- **Statement:** A class MAY declare one or more unique keys over its fields. The DVM MUST enforce key uniqueness per branch at commit time, including across subclasses that inherit the key. Every object MUST have a human-readable `slug` that is unique per class and branch.
- **Rationale:** Business identifiers (ISIN, contract number) must be unique and addressable.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect, PER-BusinessUser
- **Acceptance:**
  1. Given key `(fund_code)` on `Fund`, when two funds with the same code are committed on one branch, then the second commit fails with `MODEL.KEY_CONFLICT`.
  2. Given the same key value on two different draft branches, when each is committed, then both succeed. When both are merged into `main`, the second merge reports a key conflict.
- **Verification:** CONF, PROP
- **Origin:** L40:SPEC-12

### FR-MODEL-014 — Stable identity independent of slug
- **Statement:** Every object MUST have an immutable UUID assigned at creation and a locator `(vae, class, slug)`. Changing a slug MUST preserve the UUID and history, and the old locator MUST resolve to the object with a redirect indication for at least the retention period of the old slug.
- **Rationale:** References and proofs must survive renames.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessUser, PER-LogicDeveloper
- **Acceptance:**
  1. Given object `Fund/geq` renamed to `Fund/global-equity`, when `Fund/geq` is read, then the object is returned with `moved_to: Fund/global-equity`.
  2. Given a reference stored before the rename, when it is resolved, then it resolves to the same UUID.
- **Verification:** CONF
- **Origin:** L40:SPEC-12

### FR-MODEL-015 — Namespaces owned by Buks
- **Statement:** Every class MUST belong to exactly one namespace, and a namespace MUST be owned by exactly one Buk or by the tenant. A class MUST NOT be changed by a Buk other than its namespace owner. Other Buks change it only through extensions (CAP-MODEL-07).
- **Rationale:** Clear ownership keeps upgrades and merges tractable.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, FRG
- **Personas:** PER-IsvDeveloper, PER-BusinessArchitect
- **Acceptance:**
  1. Given Buk `crm` that tries to change class `contracts:Agreement`, when it is installed, then the installation fails with `PKG.NAMESPACE_VIOLATION`.
- **Verification:** CONF
- **Origin:** L40:SPEC-28

### FR-MODEL-016 — Abstract classes
- **Statement:** A class declared `abstract: true` MUST NOT have direct instances. Queries and selectors over it MUST include instances of its concrete subclasses.
- **Rationale:** Base concepts such as `Party` exist only through specialisations.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given abstract `Party`, when an instance of `Party` is committed, then the commit fails with `MODEL.ABSTRACT_INSTANTIATION`.
- **Verification:** CONF
- **Origin:** L40:SPEC-29

### FR-MODEL-017 — Documentation is part of the model
- **Statement:** Every class, field, port and lifecycle state MUST carry a `description`. Buk verification (FR-PKG-061) MUST fail when any description is missing or empty.
- **Rationale:** The model is also the knowledge base for users and agents (BPU knowledge element).
- **Priority:** Should · **Phase:** PH-1 · **Systems:** DVM, FRG
- **Personas:** PER-BusinessArchitect, PER-AiAgent
- **Acceptance:**
  1. Given a field without description, when `forge check` runs, then a finding `FRG.DOC_MISSING` names the field.
- **Verification:** CONF, INSP
- **Origin:** EXT-BPU, IMP-10

## CAP-MODEL-02 — Inheritance

### FR-MODEL-021 — Single-rooted hierarchy with base classes
- **Statement:** All classes MUST descend from the root class `Object`. The genesis model MUST provide the abstract base classes `DataObject`, `LogicAsset`, `ViewAsset`, `ConfigAsset` and `SystemObject` directly under `Object`.
- **Rationale:** One root gives uniform behaviour. The four input kinds from EXT-RFC become base classes (UBS-META-06 §6).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer, PER-BusinessArchitect
- **Acceptance:**
  1. Given any class, when its linearization is requested, then it ends with `Object`.
- **Verification:** CONF
- **Origin:** EXT-RFC, L40:SPEC-13

### FR-MODEL-022 — Single parent, traits and C3 linearization
- **Statement:** A class MUST have exactly one parent class (except `Object`) and MAY include ordered traits. The DVM MUST compute a deterministic linearization using the C3 algorithm and MUST reject any declaration that creates a cycle or has no consistent linearization.
- **Rationale:** Traits allow reuse (for example `Addressable`, `Approvable`) without the ambiguity of unrestricted multiple inheritance.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given traits that make C3 inconsistent, when the class is committed, then it fails with `MODEL.LINEARIZATION_INCONSISTENT`.
  2. Given the same declarations on two conforming implementations, when linearization is computed, then the results are identical.
- **Verification:** CONF, PROP
- **Origin:** L40:SPEC-13

### FR-MODEL-023 — Narrowing only
- **Statement:** A subclass MAY add fields and ports and MAY narrow inherited constraints (tighter ranges or lengths, optional to required, enum subset, more specific reference target). It MUST NOT remove or widen any inherited member. Violations MUST be rejected with `MODEL.NARROWING_VIOLATION`.
- **Rationale:** Substitutability: every instance of a subclass is a valid instance of each ancestor, so rules and queries over ancestors stay correct.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given parent field `title` required with max length 200, when a subclass declares it optional, then the commit fails with `MODEL.NARROWING_VIOLATION`.
  2. Given a subclass narrowing max length to 120, when it is committed, then it succeeds, and the effective constraint is 120.
- **Verification:** CONF, PROP
- **Origin:** L40:SPEC-13

### FR-MODEL-024 — Kind is inherited
- **Statement:** A class MUST have the same kind as its parent, except direct children of `DataObject`, which choose their kind. A trait MUST declare the kinds it may be applied to.
- **Rationale:** Mixing append-only and mergeable semantics in one hierarchy would break both.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a Ledger class `CashMovement`, when a subclass declares kind Definition, then the commit fails with `MODEL.KIND_MISMATCH`.
- **Verification:** CONF
- **Origin:** IMP-01

### FR-MODEL-025 — Own declarations stored, effective definition computed
- **Statement:** A class version MUST store only its own declarations. The DVM MUST compute the effective definition from the linearization at read time or from a cache that is invalidated by every commit that changes any class in the linearization.
- **Rationale:** Storing effective copies would make parent changes silently diverge from subclasses.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a parent change that adds a field, when a subclass effective definition is read after the commit, then the new field is present without any commit to the subclass.
- **Verification:** CONF, PROP
- **Origin:** L40:SPEC-13

### FR-MODEL-026 — Hierarchy limits
- **Statement:** A conforming DVM MUST support inheritance depth of at least 32 and at least 16 traits per class. The effective definition of a cached class MUST resolve within the NR-PERF targets.
- **Rationale:** Deep vertical specialisations (industry → region → tenant) must not hit limits.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a chain of 32 classes, when an instance of the deepest class is committed and validated, then it succeeds.
- **Verification:** CONF, BENCH
- **Origin:** NEW — explicit minimums make conformance testable.

### FR-MODEL-027 — Impact analysis of class changes
- **Statement:** Before a class change is merged, the platform MUST report the affected subclasses, extensions, sheets, views, logic assets, projections and instance counts per branch.
- **Rationale:** Parents affect every descendant, so the reviewer must see the blast radius.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, STU, FRG
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a change to `Agreement`, when impact analysis runs, then `ServiceAgreement`, `NDA`, the views and sheets selecting them, and the instance counts per subclass are listed.
- **Verification:** SCN, CONF
- **Origin:** NEW — required for safe governance of shared parents.

## CAP-MODEL-03 — Polymorphism

### FR-MODEL-031 — Most-specific port resolution
- **Statement:** When a port is invoked on an object, the DVM MUST execute the implementation declared by the first class in the object's linearization that implements the port. The execution record MUST name the resolved class and logic version.
- **Rationale:** Polymorphism is the DVM's analogue of virtual dispatch (EXT-BPU).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper, PER-BusinessArchitect
- **Acceptance:**
  1. Given `terminate` on `Agreement` and an override on `ServiceAgreement`, when it is invoked on a `ServiceAgreement`, then the override runs, and the record names `ServiceAgreement`.
- **Verification:** CONF, SCN
- **Origin:** EXT-BPU

### FR-MODEL-032 — Override compatibility
- **Statement:** An override MUST accept every parameter value the overridden port accepts, MUST return a result type that is the same or narrower, and MUST NOT declare error codes outside the overridden port's declared errors. Otherwise the class commit fails with `MODEL.PORT_SIGNATURE_INCOMPATIBLE`.
- **Rationale:** Callers that target the parent must remain correct.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, FRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a parent parameter `reason: text`, when the override declares `reason: enum`, then the commit fails.
- **Verification:** CONF
- **Origin:** NEW — standard substitutability rule.

### FR-MODEL-033 — Calling the next implementation
- **Statement:** An override MUST be able to invoke the next implementation in the linearization with an explicit instruction (`run.super`). It MUST NOT be able to bypass intermediate implementations.
- **Rationale:** Specialisations usually extend behaviour, not replace it.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given three levels implementing `validate`, when the lowest calls `run.super`, then the middle implementation runs, and calling `run.super` from it runs the top one.
- **Verification:** CONF
- **Origin:** NEW

### FR-MODEL-034 — Queries are polymorphic by default
- **Statement:** A query over a class MUST include instances of all its subclasses unless the query sets `only: true`. Each result MUST carry its concrete class.
- **Rationale:** "All agreements" must mean every kind of agreement.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessUser, PER-DataAnalyst
- **Acceptance:**
  1. Given 3 `NDA` and 2 `ServiceAgreement` instances, when `Agreement` is queried, then 5 results are returned with their concrete classes. With `only: true`, 0 results are returned.
- **Verification:** CONF
- **Origin:** L40:SPEC-21

### FR-MODEL-035 — Selectors are polymorphic
- **Statement:** A governance-sheet type selector MUST match instances of the named class and of all its subclasses. Specificity MUST increase with the depth of the named class in the hierarchy.
- **Rationale:** A control written for `MoneyMovement` must apply to every movement type (SCN-009).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given sheets for `MoneyMovement` and `Refund` setting the same property, when a `Refund` is evaluated, then the `Refund` sheet wins on specificity.
- **Verification:** CONF
- **Origin:** EXT-WP, EXT-RFC

### FR-MODEL-036 — Views are resolved polymorphically
- **Statement:** For a given object and mode (view, edit, create, list, card, print), the platform MUST select the view declared by the most specific class in the linearization that declares a view for that mode. When no class declares one, a view generated from the effective definition MUST be used.
- **Rationale:** Subclasses show their extra fields without duplicating every view.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, WSP
- **Personas:** PER-BusinessArchitect, PER-BusinessUser
- **Acceptance:**
  1. Given an edit view on `Agreement` only, when a `ServiceAgreement` is edited, then the `Agreement` edit view is used, extended with generated sections for the subclass fields.
- **Verification:** SCN, CONF
- **Origin:** L40:SPEC-25

## CAP-MODEL-04 — Value types and constraints

### FR-MODEL-041 — Standard value types
- **Statement:** The DVM MUST support these value types:
  - `text`, `integer`, `decimal`, `money`, `quantity`, `boolean`;
  - `date`, `time`, `instant`, `period`, `duration`;
  - `enum`, `reference`, `composite`, `list`, `map`;
  - `document` (rich text), `blob` (file reference), `json` (untyped).

  Each type MUST have a canonical JSON representation defined in the BPA standard.
- **Rationale:** A closed, standard type set makes objects portable between implementations.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect, PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given one field of every type, when an object is committed and read back on another conforming implementation, then the canonical JSON is byte-identical.
- **Verification:** CONF
- **Origin:** L40:SPEC-13, EXT-RFC

### FR-MODEL-042 — Exact decimal arithmetic
- **Statement:** `decimal` and `money` values MUST be stored and computed as exact decimals with declared scale. Binary floating point MUST NOT be used for them in storage, logic or transport. Each field MUST declare a rounding mode from the standard set (half-even, half-up, down, up, ceiling, floor).
- **Rationale:** Financial correctness (first vertical).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, SDK
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given `0.1 + 0.2` in logic on decimals, when evaluated, then the result is exactly `0.3`.
  2. Given `money` with scale 2 and half-even, when 2.345 is assigned, then 2.34 is stored.
- **Verification:** CONF, PROP
- **Origin:** L40:SPEC-02

### FR-MODEL-043 — Declarative field constraints
- **Statement:** Fields MUST support the constraints `required`, `min`, `max`, `min_length`, `max_length`, `pattern`, `scale`, `allowed_values`, `unique`, `default` and `computed` (an L1 expression). The DVM MUST evaluate them on every commit that creates or changes the object.
- **Rationale:** Most validation must be declarative (IMP-05).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect, PER-BusinessAnalyst
- **Acceptance:**
  1. Given `pattern: ^[A-Z]{2}[A-Z0-9]{9}[0-9]$` on `isin`, when an invalid value is committed, then the commit fails with `MODEL.CONSTRAINT_VIOLATED` and an explanation that names the field and constraint.
- **Verification:** CONF
- **Origin:** L40:SPEC-13, IMP-05

### FR-MODEL-044 — Absent versus null
- **Statement:** In any object or overlay snapshot, an absent member MUST mean "not set here; inherit or default", and an explicit `null` MUST mean "cleared". `null` MUST be rejected for required fields.
- **Rationale:** Overlays and extensions need to distinguish "no opinion" from "remove".
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ImplementationConsultant, PER-KernelEngineer
- **Acceptance:**
  1. Given an overlay that omits `label`, when read, then the parent branch label is effective. Given an overlay with `label: null`, then the effective label is empty.
- **Verification:** CONF, PROP
- **Origin:** L40:SPEC-02

### FR-MODEL-045 — Typed references with referential rules
- **Statement:** A `reference` field MUST name a target class (subclasses allowed) and a referential rule for when the target ends: `restrict`, `nullify` or `cascade_end`. The rule MUST be enforced at commit.
- **Rationale:** Integrity without foreign-key tables per class.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given `restrict` from `Order.customer` to `Customer`, when a referenced customer is terminated, then the commit fails with `MODEL.REFERENCE_RESTRICTED`, listing the referencing objects (up to a limit) and their count.
- **Verification:** CONF
- **Origin:** L40:SPEC-13

### FR-MODEL-046 — Custom value types
- **Statement:** Builders MUST be able to define named value types (for example ISIN, LEI, IBAN, CUSIP) that combine a base type, constraints, an L1 validation expression, a normaliser and a display format. Custom value types MUST be reusable across classes and Buks.
- **Rationale:** Domain identifiers are validated the same way everywhere.
- **Priority:** Should · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given the value type `ISIN` with checksum validation, when an ISIN with a wrong check digit is committed in any class, then the commit fails.
- **Verification:** CONF
- **Origin:** NEW — domain identifiers are pervasive in the first vertical.

### FR-MODEL-047 — Data classification on fields
- **Statement:** Every field MUST carry a data classification from the set `public`, `internal`, `confidential`, `personal` and `sensitive_personal`. It inherits the class default when not declared. Classification MUST be available to authorization, masking, sync, AI, export and retention.
- **Rationale:** Privacy (DEC-003) and safe AI and worker usage depend on field-level classification.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer, PER-SecurityOfficer
- **Acceptance:**
  1. Given `Person.email` classified `personal`, when the model is exported, then the classification appears. When an agent without `personal` clearance reads it, then the value is masked.
- **Verification:** CONF, SEC
- **Origin:** NEW — required by CR-GDPR and CR-HIPAA items.
- **Phase note:** PH-1 step 19b (DEC-086): PH-1 carries the classification, its class default and its export with the model; the masking part of the acceptance follows DSN-DVM-821 in PH-2.

## CAP-MODEL-05 — State kinds

### FR-MODEL-051 — Four state kinds
- **Statement:** Every concrete class MUST have exactly one kind: `Definition`, `Ledger`, `System` or `RawDocument`. Kind semantics MUST be enforced by the kernel, not by application logic.
- **Rationale:** Git-style semantics fit definitions; accounting semantics fit facts (IMP-01).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect, PER-KernelEngineer
- **Acceptance:**
  1. Given a class without a resolvable kind, when committed, then it fails with `MODEL.DECLARATION_INCOMPLETE`.
- **Verification:** CONF
- **Origin:** IMP-01

### FR-MODEL-052 — Definition-kind semantics
- **Statement:** Objects of Definition kind MUST be versioned per branch, MUST participate in diff and three-way merge, and MUST carry a valid-time interval on every version.
- **Rationale:** Configuration, master data, rules and documents need review, branching and effective dating.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a Definition object changed on two branches, when merged, then field-level three-way merge applies (FR-VER-051).
- **Verification:** CONF
- **Origin:** IMP-01, IMP-02

### FR-MODEL-053 — Ledger-kind semantics
- **Statement:** Objects of Ledger kind (entries) MUST be immutable once committed. Update, delete and field merge of entries MUST be rejected. Entries MUST carry a sequence number, a transaction time and a valid time.
- **Rationale:** Facts are corrected by new facts, never by rewriting (IMP-01, EXT-TRI Book II).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant, PER-InternalAuditor
- **Acceptance:**
  1. Given a committed entry, when an update is attempted, then it fails with `LEDG.IMMUTABLE_ENTRY`.
- **Verification:** CONF, PROP
- **Origin:** IMP-01

### FR-MODEL-054 — System-kind semantics
- **Statement:** System-kind objects (for example jobs, schedules, subscriptions, sessions and sync cursors) MUST be written only by kernel services and by dedicated instructions. General commit instructions from business logic MUST NOT write them.
- **Rationale:** Kernel bookkeeping stays inspectable as objects without becoming a business-writable surface.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, NOD
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given an L2 script that uses a generic commit on a `Job` object, when executed, then it fails with `LOGIC.FORBIDDEN_TARGET`.
- **Verification:** CONF, SEC
- **Origin:** NEW — the "jobs are entities" principle from L40:SPEC-20 needs a write boundary.

### FR-MODEL-055 — RawDocument kind (schema-on-read)
- **Statement:** A class of kind RawDocument MUST accept any valid JSON content within size limits and MUST validate only its envelope fields. Typed rules, derived projections and ports MUST NOT read RawDocument content except through a declared parser that produces typed objects.
- **Rationale:** Imports and unstructured inputs are stored faithfully without weakening the typed model (UBS-META-06 §6).
- **Priority:** Should · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given a RawDocument with arbitrary nested content, when committed, then it succeeds. When a rule selector references its content fields, then the sheet commit fails with `RULE.UNTYPED_ACCESS`.
- **Verification:** CONF
- **Origin:** EXT-RFC

### FR-MODEL-056 — Kind is fixed once instances exist
- **Statement:** The kind of a class MUST NOT change after the class has any instance on any branch. Changing kind requires a new class and a migration process.
- **Rationale:** Existing objects cannot switch between merge and append-only semantics.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a class with instances, when a kind change is committed, then it fails with `MODEL.KIND_IMMUTABLE`.
- **Verification:** CONF
- **Origin:** IMP-01

## CAP-MODEL-06 — Relationships

### FR-MODEL-061 — Declared relationships
- **Statement:** A relationship declaration MUST name the source class, target class, cardinality (`0..1`, `1`, `0..n`, `1..n`, `m..n`), inverse name and ownership (`composition` or `association`). Composition MUST end or terminate owned objects with their owner.
- **Rationale:** The business graph is explicit and navigable in both directions.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given `Agreement.parties` of cardinality `2..n`, when an agreement with one party is committed, then it fails with `MODEL.CARDINALITY_VIOLATED`.
- **Verification:** CONF
- **Origin:** L40:SPEC-13

### FR-MODEL-062 — Relationships with attributes and validity
- **Statement:** A relationship MAY declare attributes, in which case each link MUST be stored as a relationship object with its own versions and valid-time interval.
- **Rationale:** Party roles, ownership percentages and signatory rights change over time.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given `Organisation.owners` with attribute `percent`, when the percentage changes with valid time from a date, then `asof` queries return the percentage effective at each date.
- **Verification:** CONF
- **Origin:** L40:SPEC-29, IMP-02

### FR-MODEL-063 — Referential integrity at the valid time
- **Statement:** At commit, the DVM MUST verify that every reference and link target exists on the same branch and is valid at the referencing version's valid time. Violations MUST fail with `MODEL.REFERENCE_INVALID`.
- **Rationale:** Bitemporal models can otherwise reference things that did not yet exist.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a share class valid from 2026-11-02, when an order with dealing date 2026-10-30 references it, then the commit fails.
- **Verification:** CONF, PROP
- **Origin:** IMP-02

### FR-MODEL-064 — Cross-VAE references
- **Statement:** A reference to an object in another VAE MUST be stored as a canonical URI. It MUST be allowed only when the target VAE publishes the target class or object to the source VAE.
- **Rationale:** Federation and VAE trees need references without breaking isolation.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, FED
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an unpublished target, when a cross-VAE reference is committed, then it fails with `IAM.NOT_FOUND`.
- **Verification:** CONF, SEC
- **Origin:** EXT-RFC, L40:SPEC-12

### FR-MODEL-065 — Graph traversal
- **Statement:** The platform MUST support traversal queries along declared relationships with a depth limit, direction and class filters. Every traversal step MUST apply authorization.
- **Rationale:** Obligations → agreements → parties → roles is a common navigation.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessUser, PER-AiAgent
- **Acceptance:**
  1. Given depth 3 from an `Obligation`, when traversed to `Party`, then only permitted parties are returned.
- **Verification:** CONF
- **Origin:** L40:SPEC-21

## CAP-MODEL-07 — Class extension

### FR-MODEL-071 — Extensions without forking
- **Statement:** A tenant or Buk MUST be able to extend a class it does not own with an extension object that adds fields, ports, views and narrowing constraints. The extended class's owner MUST NOT need to change.
- **Rationale:** Customisation without forks (EXT-TRI Book II, "branch inheritance").
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ImplementationConsultant, PER-IsvDeveloper
- **Acceptance:**
  1. Given an extension adding `cost_center` to `contracts:Agreement`, when the vendor upgrades `Agreement`, then `cost_center` remains effective (SCN-013).
- **Verification:** CONF, SCN
- **Origin:** L40:SPEC-13

### FR-MODEL-072 — Extensions obey narrowing
- **Statement:** An extension MUST NOT remove or widen members of the extended class. It MUST be validated with the same rules as a subclass (FR-MODEL-023).
- **Rationale:** Extensions must not break the owner's logic.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given an extension that makes a required field optional, when committed, then it fails with `MODEL.NARROWING_VIOLATION`.
- **Verification:** CONF
- **Origin:** L40:SPEC-13

### FR-MODEL-073 — Namespaced extension members
- **Statement:** Members added by extensions MUST be namespaced by the extension owner (`<namespace>.<member>`) in storage and in the canonical representation. User interfaces MAY show short names when they are unambiguous.
- **Rationale:** Two Buks adding the same field name must never collide.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given Buks `a` and `b` both adding `priority`, when both are installed, then `a.priority` and `b.priority` coexist.
- **Verification:** CONF
- **Origin:** NEW — collision-free extension is required for an open Buk ecosystem.

### FR-MODEL-074 — Extension scope
- **Statement:** An extension MUST be scoped to the branch or overlay where it is committed. It becomes visible elsewhere only through merge or branch inheritance.
- **Rationale:** One tenant's customisation must never leak into another tenant.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given an extension on tenant A's overlay, when tenant B reads the class, then the extension is absent.
- **Verification:** CONF, SEC
- **Origin:** L40:SPEC-23

## CAP-MODEL-08 — Schema evolution

### FR-MODEL-081 — Schema versions and change classification
- **Statement:** Every class change MUST increment the class `schema_version`. Forge and the DVM MUST classify the change as `additive` (no lens needed), `lens_required` or `breaking`. Classes changed in a `lens_required` way MUST carry a lens.
- **Rationale:** Evolution must be explicit to keep old data readable (IMP-04).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, FRG
- **Personas:** PER-BusinessArchitect, PER-IsvDeveloper
- **Acceptance:**
  1. Given a field rename without a lens, when committed, then it fails with `MODEL.LENS_REQUIRED`.
  2. Given a new optional field, when committed, then it is classified `additive`, and no lens is required.
- **Verification:** CONF
- **Origin:** IMP-04

### FR-MODEL-082 — Declarative lenses
- **Statement:** A lens MUST be expressible with the declarative operations `rename`, `split`, `join`, `convert`, `default`, `nest`, `unnest`, `map_enum` and `drop_with_archive`, plus an optional L2 function for residual cases. Each lens MUST define forward and backward directions, or be marked one-way with a reason.
- **Rationale:** Declarative lenses are analysable and verifiable. Most evolution fits them.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, FRG
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given the split lens of SCN-014, when applied forward and then backward to a sample, then the original value is restored.
- **Verification:** CONF, PROP
- **Origin:** IMP-04

### FR-MODEL-083 — Reading old versions through lenses
- **Statement:** Reading an object version stored in an older schema version MUST return it in the requested schema version (default: current) by applying the lens chain. Stored bytes MUST NOT change.
- **Rationale:** History stays usable, and its proofs stay valid.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-DataAnalyst, PER-InternalAuditor
- **Acceptance:**
  1. Given a 2025 version stored in schema 3, when read with current schema 5, then the lenses 3→4→5 are applied, and the version hash equals the stored hash.
- **Verification:** CONF, PROP
- **Origin:** IMP-04

### FR-MODEL-084 — Writes use the current schema
- **Statement:** Every new commit MUST store objects in the class schema version current on the target branch at commit time.
- **Rationale:** It avoids mixed-version writes after an upgrade.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a client that submits an old-shape payload with a declared schema version, when committed, then the payload is lensed forward before storage, or rejected if no forward lens exists.
- **Verification:** CONF
- **Origin:** IMP-04

### FR-MODEL-085 — Schema alignment before merge
- **Statement:** Before a three-way merge, the DVM MUST align base, source and target versions of every object to the target branch's schema version through lenses.
- **Rationale:** Long-lived drafts must merge after the model has evolved (SCN-014, SCN-401).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect, PER-FieldWorker
- **Acceptance:**
  1. Given a draft editing `name` and `main` now in split schema, when merged, then the edit appears as changes to `given_name` and `family_name` or as a reported conflict.
- **Verification:** CONF, PROP
- **Origin:** IMP-04

### FR-MODEL-086 — Lens verification
- **Statement:** Forge MUST verify each lens by round-trip tests on generated values and on a sample of stored versions. It MUST report any non-total case. A lens with unhandled cases MUST NOT merge into a release branch.
- **Rationale:** An unverified lens corrupts reads of history.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-BusinessArchitect, PER-LogicDeveloper
- **Acceptance:**
  1. Given a split lens failing on single-word names, when `forge check` runs, then the failing samples are reported, and the release merge is blocked.
- **Verification:** CONF, PROP
- **Origin:** IMP-04

### FR-MODEL-087 — Breaking changes need a migration process
- **Statement:** A `breaking` class change MUST be applied only together with a migration process that rewrites affected current objects in one approved change set. History MUST remain readable in its original schema.
- **Rationale:** Some changes (for example a type change without a total mapping) cannot be lensed.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, STU
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a breaking change without a migration, when merged, then it fails with `MODEL.MIGRATION_REQUIRED`.
- **Verification:** CONF, SCN
- **Origin:** IMP-04

## CAP-MODEL-09 — Base ontology

### FR-MODEL-091 — Ontology Buk content
- **Statement:** The genesis `ontology` Buk MUST provide at least these classes:
  - `Party` (abstract), `Person`, `Organisation`, `Role`, `PartyRole`;
  - `Address`, `ContactPoint`, `Identifier` (LEI, tax ID, registration number);
  - `Money` (value type), `Account`, `Calendar`, `Document`, `Agreement` (abstract), `Location`;

  and the traits `Addressable`, `Approvable`, `Classifiable` and `Attachable`.
- **Rationale:** Shared foundations make Buks interoperable (L40:SPEC-29).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect, PER-IsvDeveloper
- **Acceptance:**
  1. Given a freshly bootstrapped VAE, when the model is listed, then every listed class and trait exists with descriptions.
- **Verification:** CONF, INSP
- **Origin:** L40:SPEC-29

### FR-MODEL-092 — Party–role model
- **Statement:** A party MUST be able to play any number of roles (for example investor, counterparty, employee, signatory). Roles MUST be `PartyRole` relationship objects with attributes and valid time.
- **Rationale:** The same organisation is customer, supplier and investor at different times.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given `Acme` as supplier since 2020 and investor since 2026-06, when roles are queried `asof=2026-01-01`, then only supplier is returned.
- **Verification:** CONF
- **Origin:** L40:SPEC-29

### FR-MODEL-093 — Extend, do not redefine
- **Statement:** Forge MUST warn when a Buk declares a class whose fields overlap an ontology class by more than a configurable threshold (default 60%) and does not extend it.
- **Rationale:** Duplicate party models fragment data (SCN-304 A1).
- **Priority:** Should · **Phase:** PH-2 · **Systems:** FRG
- **Personas:** PER-IsvDeveloper, PER-AiAgent
- **Acceptance:**
  1. Given a Buk class `Customer` with name, address and email and no parent in the ontology, when checked, then warning `FRG.ONTOLOGY_DUPLICATE` suggests extending `Organisation` or `Person`.
- **Verification:** CONF
- **Origin:** L40:SPEC-29

### FR-MODEL-094 — Ontology evolves as a Buk
- **Statement:** The ontology MUST be versioned and upgraded with the Buk mechanisms (CAP-PKG-04), including lenses for its class changes.
- **Rationale:** The ontology is not frozen kernel code.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given ontology 1.1 installed, when 1.2 is released with an additive change, then the upgrade merges into tenants with overlays preserved.
- **Verification:** SCN
- **Origin:** L40:SPEC-28

## CAP-MODEL-10 — Model introspection and export

### FR-MODEL-101 — Reflection
- **Statement:** The platform MUST provide read operations that return, for any class on any branch and time:
  - the effective definition and the linearization;
  - ports, lifecycle and views;
  - extensions applied;
  - governance sheets whose selectors can match the class.
- **Rationale:** Tools, UIs and agents rely on the model at run time.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, SDK
- **Personas:** PER-LogicDeveloper, PER-AiAgent
- **Acceptance:**
  1. Given `ServiceAgreement`, when reflected, then the result includes inherited fields marked with their declaring class.
- **Verification:** CONF
- **Origin:** L40:SPEC-13

### FR-MODEL-102 — JSON Schema export
- **Statement:** The platform MUST export the effective definition of any class version as JSON Schema draft 2020-12, including constraints, classification annotations and descriptions.
- **Rationale:** Open standards at the edges (IMP-12).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, FRG
- **Personas:** PER-LogicDeveloper, PER-DataAnalyst
- **Acceptance:**
  1. Given an exported schema, when validated with a standard JSON Schema validator against committed objects, then every committed object validates.
- **Verification:** CONF
- **Origin:** IMP-12

### FR-MODEL-103 — OpenAPI export
- **Statement:** The platform MUST export an OpenAPI 3.1 document for the ports and queries of a selected set of classes. Operation IDs MUST be stable across exports of the same schema versions.
- **Rationale:** Integration and client generation (IMP-12).
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, BRG
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given the fund-operations classes, when exported twice without model changes, then the documents are byte-identical.
- **Verification:** CONF
- **Origin:** IMP-12

### FR-MODEL-104 — Model graph export
- **Statement:** The platform MUST export the class graph (inheritance, traits, relationships) as a machine-readable graph (JSON) and as a Mermaid diagram.
- **Rationale:** Humans and AI need a compact view of large models.
- **Priority:** Could · **Phase:** PH-2 · **Systems:** STU, FRG
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a Buk, when exported, then every class appears exactly once, and every relationship appears with its cardinality.
- **Verification:** INSP
- **Origin:** NEW

### FR-MODEL-105 — Model as tool metadata
- **Statement:** The effective model (descriptions, ports, parameter schemas, examples) MUST be retrievable in a form suitable for tool definitions for AI agents, respecting the caller's permissions.
- **Rationale:** The meta-model is the agent's world model (IMP-10).
- **Priority:** Must · **Phase:** PH-3 · **Systems:** DVM, AGT
- **Personas:** PER-AiAgent
- **Acceptance:**
  1. Given an agent scoped to three classes, when it requests tool metadata, then only those classes' ports and queries are described.
- **Verification:** CONF, SEC
- **Origin:** IMP-10
