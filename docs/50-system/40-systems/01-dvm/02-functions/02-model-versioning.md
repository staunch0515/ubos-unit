---
id: UBS-SYS-DVM-02-02
title: DVM — Model and Versioning Design
status: draft
phase: PH-1
depends_on: [UBS-SYS-DVM-02-01, UBS-STD-03, UBS-STD-05]
---

# DVM — Model and Versioning Design (DSN-DVM-1xx, DSN-DVM-2xx)

This file designs the crates `ubos-model` (the class system) and `ubos-version`
(commits, branches, change sets, diff, merge, revert, inheritance). Reading order for an
AI implementer:
1. model registry and effective definitions (101–109);
2. validation plans (110–114);
3. lenses and schema evolution (120–126);
4. reflection and exports (130–133);
5. commits and branches (201–212);
6. change sets (220–226);
7. diff (230–233);
8. merge (240–249);
9. revert (250–252);
10. inheritance between branches (260–264);
11. tags and releases (270–272).

## Model registry and effective definitions

### DSN-DVM-101 — Model registry per snapshot
- **Statement:** `ubos-model` MUST expose a `ModelRegistry` that is built for a given `(branch, cseq)` snapshot from the class, extension, lens and port-implementation objects visible at that snapshot. The registry MUST be immutable once built. It MUST be cached by the content hash of its input versions (the "model hash"), so that two snapshots with the same model share one registry instance.
- **Rationale:** Classes are versioned objects (FR-MODEL-011). Most commits do not change the model, so the model hash makes the registry reusable across thousands of snapshots.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given 1,000 business commits with no model change, when registries are requested for each snapshot, then exactly one registry is built.
  2. Given a class change in commit N, when requesting registries at N−1 and N, then they differ, and each matches a from-scratch build.
- **Verification:** PROP, BENCH
- **Origin:** FR-MODEL-011, FR-MODEL-025, STD-CLS-012, AR-028

### DSN-DVM-102 — Model hash computation
- **Statement:** The model hash MUST be the domain-tagged hash (`ubos:model:v1`) of the sorted list of `(object_id, version_id)` pairs of all model objects in the snapshot. The kernel MUST maintain it incrementally in the flush: a commit that touches no model object copies its parent's model hash into the commit's derived metadata.
- **Rationale:** Constant-time model identity per snapshot.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a random sequence of model and non-model commits, when the model hash is recomputed from scratch for every commit, then it equals the maintained value.
- **Verification:** PROP
- **Origin:** STD-FND-011, AR-015, AR-016

### DSN-DVM-103 — Linearization and hierarchy checks
- **Statement:** When a class object is created or updated, the model checker MUST, before the flush:
  - compute the C3 linearization for the class and every descendant (STD-CLS-011);
  - reject cycles with `MODEL.INHERITANCE_CYCLE`;
  - reject inconsistent orders with `MODEL.LINEARIZATION_INCONSISTENT`;
  - enforce the depth and trait limits (STD-CLS-015, FR-MODEL-026);
  - enforce kind inheritance (STD-CLS-014) and the fixed-kind rule (`MODEL.KIND_IMMUTABLE`).

  The checker MUST work on the process's pending model plus the snapshot model, so several interdependent class changes in one process are checked together.
- **Rationale:** Model errors are caught at commit, not at run time.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect, PER-KernelEngineer
- **Acceptance:**
  1. Given the linearization vectors of the conformance suite, when checked, then results equal the vectors exactly.
  2. Given one process that adds a trait and a class using it, when committed, then it succeeds; the same class alone fails.
- **Verification:** CONF, PROP
- **Origin:** FR-MODEL-021, FR-MODEL-022, FR-MODEL-024, FR-MODEL-026, FR-MODEL-056, STD-CLS-011, STD-CLS-014, FR-MODEL-051

### DSN-DVM-104 — Narrowing checker
- **Statement:** The narrowing checker MUST implement the narrowing matrix of STD-CLS-013 as a table-driven function `narrows(parent_field, child_field) → Ok | Violation(rule, path)`. It MUST be applied:
  - to every field redeclared by a subclass or trait;
  - to every extension member (STD-CLS-030);
  - to every descendant when a parent class is changed (re-check of the subtree).

  Violations MUST be reported all at once with `MODEL.NARROWING_VIOLATION` and one detail per offending path.
- **Rationale:** The Liskov guarantee is enforced mechanically.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given every row of the narrowing matrix, when tested with a positive and a negative case, then the checker agrees with the matrix.
  2. Given a parent change that breaks three descendants, when committed, then all three violations are reported in one error.
- **Verification:** CONF, PROP
- **Origin:** FR-MODEL-023, FR-MODEL-072, STD-CLS-013, STD-CLS-030

### DSN-DVM-105 — Effective definition builder
- **Statement:** The effective definition of a class MUST be built by folding own declarations over the linearization, from the root to the class, then applying active extensions in declaration order (STD-CLS-012). The result MUST record, for each member, the declaring class and version (provenance). Effective definitions MUST be cached by `(model hash, class id)`.
- **Rationale:** Own values stored, effective values computed (FR-MODEL-025), with explainable provenance.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect, PER-LogicDeveloper
- **Acceptance:**
  1. Given the effective-definition vectors, when built, then canonical JSON output is byte-identical to the vectors.
  2. Given any member of an effective definition, when inspected, then its declaring class is reported.
- **Verification:** CONF
- **Origin:** FR-MODEL-025, FR-MODEL-101, STD-CLS-012

### DSN-DVM-106 — Port dispatch table
- **Statement:** For each class the registry MUST precompute a dispatch table `port name → [implementations in linearization order]`. `run.port` MUST select the first entry; `run.super` MUST select the next entry after the calling implementation (STD-CLS-020, STD-ISA-051). Override compatibility (STD-CLS-021) MUST be checked when an implementation is registered, raising `MODEL.PORT_SIGNATURE_INCOMPATIBLE`. Calling a port with no implementation MUST raise `MODEL.PORT_NOT_IMPLEMENTED`.
- **Rationale:** Constant-time polymorphic dispatch.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a three-level hierarchy with overrides at levels 1 and 3, when a level-3 object calls the port and the override calls `run.super`, then the level-1 implementation runs.
- **Verification:** CONF
- **Origin:** FR-MODEL-031, FR-MODEL-032, FR-MODEL-033, STD-CLS-020, STD-CLS-021

### DSN-DVM-107 — Class subtree index
- **Statement:** The registry MUST assign every class a pre-order interval `(lo, hi)` over the single-parent tree, so that "instance of C or its subclasses" becomes `lo(C) ≤ class_ord ≤ hi(C)`. The kernel head and temporal indexes MUST store `class_ord` per row, keyed per model hash. When the model changes, a background task MUST re-stamp affected rows; until finished, queries MUST fall back to an explicit list of subclass IDs.
- **Rationale:** Polymorphic queries by default (FR-MODEL-034) without a join per level.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a 5-level hierarchy and 1,000,000 instances, when querying the root class, then latency meets NR-PERF-008.
  2. Given a class inserted in the middle of the hierarchy, when querying during re-stamping, then results are complete and correct.
- **Verification:** PROP, BENCH
- **Origin:** FR-MODEL-034, FR-MODEL-035, STD-CLS-022, NR-PERF-008

### DSN-DVM-108 — Model impact analysis
- **Statement:** The model checker MUST provide `impact(change) → ImpactReport` listing, for a proposed class change:
  - its change classification (STD-CLS-050);
  - affected descendants, extensions, sheets, views and logic assets (by static reference);
  - the count of affected instances per branch (from the subtree index);
  - whether a lens or a migration process is required.

  The report MUST be computed without executing business logic.
- **Rationale:** Builders see consequences before they commit (FR-MODEL-027).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, STU
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a field type change in a base class, when analysed, then every dependent artefact in the fixture is listed, and the classification is "breaking".
- **Verification:** CONF, PROP
- **Origin:** FR-MODEL-027, FR-MODEL-081, FR-MODEL-087, STD-CLS-050

### DSN-DVM-109 — Relationship and reference registry
- **Statement:** The registry MUST index declared relationships (STD-CLS-040) by source class, target class and name, including cardinality, validity and referential action. The flush MUST check referential integrity at the valid time (STD-CLS-041) using `relationship_index`. Violations MUST raise `MODEL.REFERENCE_INVALID`, `MODEL.REFERENCE_RESTRICTED` or `MODEL.CARDINALITY_VIOLATED`.
- **Rationale:** Integrity in the kernel, not in each app.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a reference whose target ends before the source's valid period ends, when committed, then `MODEL.REFERENCE_INVALID` names both periods.
- **Verification:** CONF, PROP
- **Origin:** FR-MODEL-045, FR-MODEL-061, FR-MODEL-062, FR-MODEL-063, STD-CLS-040, STD-CLS-041

## Validation plans

### DSN-DVM-110 — Compiled validation plans
- **Statement:** For each effective definition the DVM MUST compile a validation plan: a flat, ordered list of checks covering types, requiredness, value constraints, currency rules, classification tags, unique keys and cross-field expressions. Plans MUST be cached by `(model hash, class id)` and evaluated on every staged create or update before the flush. All failures of one object MUST be collected into one error with one detail per path.
- **Rationale:** Fast, complete validation (FR-MODEL-043, NR-USE-006).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessUser, PER-KernelEngineer
- **Acceptance:**
  1. Given an object with five invalid fields, when staged, then one `MODEL.CONSTRAINT_VIOLATED` error with five details is returned.
  2. Given 10,000 validations of a 50-field object, when benchmarked, then the mean cost is below 20 µs per object on ENV-REF-SERVER.
- **Verification:** CONF, BENCH
- **Origin:** FR-MODEL-043, FR-MODEL-044, STD-FND-020, STD-FND-021, NR-USE-006

### DSN-DVM-111 — Unique keys
- **Statement:** Unique keys (natural keys and declared unique constraints) MUST be enforced by `unique_key_index`, scoped per branch and per valid period where declared. The flush MUST insert index rows under the branch sequencer lock, so uniqueness is exact. Keys over encrypted fields MUST use the deterministic lookup token (STD-OBJ-012, AR-021).
- **Rationale:** Exact uniqueness without table-per-class DDL (AR-018).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given two concurrent processes creating the same natural key, when both flush, then exactly one succeeds, and the other receives `MODEL.KEY_CONFLICT`.
- **Verification:** SIM, CONF
- **Origin:** FR-MODEL-013, STD-OBJ-012, AR-018, AR-021

### DSN-DVM-112 — Abstract classes and declaration completeness
- **Statement:** The staging step MUST reject instantiation of abstract classes (`MODEL.ABSTRACT_INSTANTIATION`). The model checker MUST reject class declarations missing mandatory content (FR-MODEL-012) with `MODEL.DECLARATION_INCOMPLETE`.
- **Rationale:** Mechanical enforcement of model hygiene.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a class without documentation or kind, when committed, then `MODEL.DECLARATION_INCOMPLETE` lists every missing item.
- **Verification:** CONF
- **Origin:** FR-MODEL-012, FR-MODEL-016, FR-MODEL-017

### DSN-DVM-113 — Size limits and chunking at staging
- **Statement:** The staging step MUST compute the canonical size of every staged version. It MUST reject objects over the limits of STD-OBJ-009 with `MODEL.OBJECT_TOO_LARGE`, and chunk large attachments per STD-OBJ-008 before the flush.
- **Rationale:** Predictable memory and storage behaviour.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given an object one byte over the limit, when staged, then it is rejected before any storage write.
- **Verification:** CONF
- **Origin:** STD-OBJ-008, STD-OBJ-009

### DSN-DVM-114 — Custom value types
- **Statement:** Custom value types (STD-FND-026) MUST be compiled into the validation plan as a base type, a constraint list and an optional canonical-form normaliser written in the expression language. Normalisers MUST be total and deterministic (STD-EXPR-004).
- **Rationale:** Domain types (ISIN, LEI, IBAN) without kernel code.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given an ISIN type with a check-digit constraint, when an invalid ISIN is staged, then it is rejected with the type name in the error detail.
- **Verification:** CONF
- **Origin:** FR-MODEL-046, STD-FND-026, STD-EXPR-004

## Lenses and schema evolution

### DSN-DVM-120 — Schema version stamping
- **Statement:** Every stored object version MUST record the version ID of its class at write time (the schema version). Writes MUST always use the current schema of the snapshot (FR-MODEL-084).
- **Rationale:** Old data stays readable under the schema it was written with.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a version written under schema S1, when read after S2 is committed, then its envelope still names S1.
- **Verification:** CONF
- **Origin:** FR-MODEL-081, FR-MODEL-084, STD-OBJ-002

### DSN-DVM-121 — Lens compiler
- **Statement:** The lens language (STD-CLS-060) MUST be compiled into a list of pure field operations: rename, move, default, convert, split, join, drop-with-archive. The compiler MUST reject lenses that are not total for the source schema (STD-CLS-062). A lens chain S1 → S2 → … → Sn MUST be composed into one operation list and cached by the pair of schema versions.
- **Rationale:** Constant-cost reading of old versions.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given the lens vectors, when compiled and applied, then outputs are byte-identical to the vectors.
  2. Given a lens that does not handle a nullable field, when compiled, then the totality error names the field.
- **Verification:** CONF, PROP
- **Origin:** FR-MODEL-082, FR-MODEL-086, STD-CLS-060, STD-CLS-062

### DSN-DVM-122 — Read-time lens application
- **Statement:** When a version's schema differs from the reading snapshot's schema, the reader MUST apply the composed lens (STD-CLS-061) before returning it and MUST mark the payload with the source schema. Lens application MUST NOT change stored bytes or content IDs. Where no lens path exists, the read MUST fail with `MODEL.LENS_REQUIRED`.
- **Rationale:** Writes are never rewritten for schema changes.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given 1,000 versions written under three schemas, when read under the latest schema, then all values are in the latest shape, and stored IDs are unchanged.
- **Verification:** CONF, PROP
- **Origin:** FR-MODEL-083, STD-CLS-061

### DSN-DVM-123 — Breaking changes and migration processes
- **Statement:** A breaking class change without a total lens MUST be rejected with `MODEL.MIGRATION_REQUIRED` unless it is committed together with a migration process (a bulk process, DSN-DVM-560) that rewrites all instances on the same branch. The migration process MUST be resumable and MUST produce one commit per batch with a common reason code.
- **Rationale:** Breaking changes are explicit and auditable.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a breaking change and 100,000 instances, when migrated, then every instance is rewritten, and the migration survives a node restart mid-way.
- **Verification:** FAULT, CONF
- **Origin:** FR-MODEL-087, FR-TXN-015

### DSN-DVM-124 — Schema alignment before merge
- **Statement:** Before a merge whose two sides have different model hashes, the merge engine MUST:
  1. merge the model objects first;
  2. build the merged registry;
  3. bring both sides' changed instances to the merged schema through lenses;
  4. then merge instances.

  If model merge conflicts, instance merge MUST NOT start.
- **Rationale:** Instances are merged under one schema (FR-MODEL-085).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a branch that renamed a field and a main that edited that field, when merged, then the edit appears under the new name.
- **Verification:** CONF, PROP
- **Origin:** FR-MODEL-085, STD-VER-050

### DSN-DVM-125 — Extension scope resolution
- **Statement:** Extensions (STD-CLS-030) MUST be applied only within their declared scope (tenant, VAE or branch). The registry MUST build effective definitions per scope and include the extension set in the cache key. Overlay policies (STD-CLS-031) MUST be checked when an extension is committed.
- **Rationale:** Customer extensions do not leak across scopes.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given an extension scoped to VAE A, when a class is read in VAE B, then the extension member is absent.
- **Verification:** CONF, SEC
- **Origin:** FR-MODEL-071, FR-MODEL-073, FR-MODEL-074, STD-CLS-030, STD-CLS-031

### DSN-DVM-126 — Namespace ownership
- **Statement:** The model checker MUST reject a model object whose namespace is not owned by the Buk or tenant committing it (`PKG.NAMESPACE_VIOLATION`). Namespace ownership MUST be read from installed Buk manifests in the snapshot.
- **Rationale:** No collisions between packages (FR-MODEL-015).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given Buk A trying to declare a class in Buk B's namespace, when installed, then installation fails.
- **Verification:** CONF, SEC
- **Origin:** FR-MODEL-015, FR-MODEL-073

## Reflection and exports

### DSN-DVM-130 — Reflection API
- **Statement:** The host API MUST expose reflection over a snapshot: classes, effective definitions with provenance, relationships, ports, sheets that apply to a class, and lenses. Reflection MUST be subject to authorization (model visibility) and MUST be deterministic for a snapshot.
- **Rationale:** Tools, Studio and agents understand the model (FR-MODEL-101, FR-MODEL-105).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper, PER-AiAgent
- **Acceptance:**
  1. Given the same snapshot twice, when reflected, then outputs are byte-identical.
- **Verification:** CONF
- **Origin:** FR-MODEL-101, FR-MODEL-105

### DSN-DVM-131 — JSON Schema export
- **Statement:** The DVM MUST export each effective definition as JSON Schema 2020-12, mapping value types per a fixed table (decimal and money as patterned strings, time values as formatted strings), including `x-ubos-*` annotations for kind, classification and provenance.
- **Rationale:** Interoperability (FR-MODEL-102).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given every value type, when exported, then a standard JSON Schema validator accepts valid canonical values and rejects invalid ones.
- **Verification:** CONF
- **Origin:** FR-MODEL-102, STD-FND-020

### DSN-DVM-132 — OpenAPI and model graph export
- **Statement:** The DVM MUST export an OpenAPI 3.1 description of the ports and standard operations of a model scope, and a model graph (classes, inheritance, relationships) in a documented JSON graph format.
- **Rationale:** External tools and documentation.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a fixture model, when exported, then the OpenAPI document passes a standard linter with no errors.
- **Verification:** CONF
- **Origin:** FR-MODEL-103, FR-MODEL-104

### DSN-DVM-133 — Genesis bootstrap
- **Statement:** A new VAE MUST be created by a genesis commit that contains the kernel root and base classes (STD-CLS-010), the kernel property registry (STD-LGS-011) and the system-kind objects (STD-KIND-040). The genesis content MUST be a fixed, versioned kernel artefact, so that all conformant kernels of the same ABI produce the same genesis root.
- **Rationale:** Identical starting state everywhere (NR-DET-002).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ThirdPartyImplementer
- **Acceptance:**
  1. Given two implementations of ABI 1.0, when each creates a VAE, then the genesis state roots are equal.
- **Verification:** CONF
- **Origin:** STD-CLS-010, STD-LGS-011, STD-KIND-040, NR-DET-002, FR-TEN-012

### DSN-DVM-134 — Buk installation registry and dependency protection
- **Statement:** The DVM MUST keep, per VAE, an installation registry of Buks (name, version, archive root, namespace, installing change set, overlay branch) as System-kind objects. Uninstalling or downgrading a Buk MUST be refused while installed Buks depend on it (FR-PKG-053); uninstalling MUST keep data per the retention choice (FR-PKG-051) and remove only model, sheets and logic that no other Buk references.
- **Rationale:** FR-PKG-051, FR-PKG-053, CAP-PKG-05.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given Buk B depending on A, when A is uninstalled, then the operation is refused naming B.
  2. Given an uninstall with data retention, when completed, then the Buk's objects remain readable and exportable.
- **Verification:** CONF
- **Origin:** FR-PKG-051, FR-PKG-053, FR-PKG-031

## Commits and branches

### DSN-DVM-201 — Commit construction
- **Statement:** The commit builder MUST produce the commit record of STD-VER-001 in this order:
  1. collect staged changes, sorted by object ID;
  2. compute version IDs;
  3. apply changes to the tree and compute the state root;
  4. compute the change list hash;
  5. assign transaction time (DSN-DVM-301) and `cseq`;
  6. build the canonical commit body and its ID;
  7. sign it (DSN-DVM-520).

  Steps 3 to 7 MUST run under the branch sequencer lock.
- **Rationale:** Deterministic, verifiable commits (FR-VER-011).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the commit vectors, when built, then commit IDs are byte-identical to the vectors.
- **Verification:** CONF
- **Origin:** FR-VER-011, FR-VER-013, STD-VER-001, STD-VER-002

### DSN-DVM-202 — Commit metadata and reason codes
- **Statement:** Every commit MUST carry the author, the acting principal and delegation chain, the process ID, the channel, the reason code, and the rule bindings used (STD-TIME-040). Automated commits (jobs, flows, agents, migrations) MUST carry a reason code from the reason-code registry, and a missing code MUST fail the flush.
- **Rationale:** Every change is attributable (FR-VER-014, FR-TXN-044).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a job commit without a reason code, when flushed, then it fails.
- **Verification:** CONF
- **Origin:** FR-VER-014, FR-TXN-044, FR-TIME-061, STD-TIME-040

### DSN-DVM-203 — History listing
- **Statement:** The DVM MUST provide paged history listing per branch, per object and per class. It MUST be ordered by `cseq` descending, filterable by author, reason code and time range, and MUST use the `cseq` index for constant-cost paging.
- **Rationale:** FR-VER-015.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given 10,000,000 commits, when fetching page 10,000, then the latency equals that of page 1 within 20%.
- **Verification:** BENCH
- **Origin:** FR-VER-015, STD-VER-003

### DSN-DVM-204 — Zero-copy branch creation
- **Statement:** Creating a branch MUST write only a `branch_ref` row that points to the source commit, plus an inheritance record where the branch reads through (DSN-DVM-260). No object, index row or tree node MUST be copied. The new branch's `cseq` MUST start at the source commit's `cseq` + 1 in a new ordinal space identified by the branch.
- **Rationale:** NR-PERF-004 constant-time branching.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given a VAE with 10,000,000 objects, when a branch is created, then it completes within NR-PERF-004, and storage grows by less than 4 KiB.
- **Verification:** BENCH
- **Origin:** FR-VER-021, FR-VER-027, NR-PERF-004, NR-SCAL-006

### DSN-DVM-205 — Branch index overlay
- **Statement:** Kernel indexes of a child branch MUST hold only rows for objects changed on the child. Reads MUST resolve through the chain `child → parent@fork point → …` (DSN-DVM-261). The DVM MUST maintain a per-branch Bloom filter of changed object IDs (rebuilt from the index, never authoritative), to skip lookups on branches that did not change an object.
- **Rationale:** Branch reads cost O(chain depth) at worst and O(1) typically (AR-019).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a 4-deep branch chain, when reading unchanged objects, then point-read latency meets NR-PERF-001.
- **Verification:** BENCH, PROP
- **Origin:** AR-019, NR-PERF-001, FR-VER-081

### DSN-DVM-206 — Branch kinds and policies
- **Statement:** Branch kinds (main, release, draft, feature, agent, overlay, loader, evolution, correction, sandbox, device; STD-VER-011, STD-VER-013) MUST map to a policy record (STD-VER-011) that decides: whether direct commits are allowed, whether a change set is required (`VER.CHANGE_SET_REQUIRED`), which merge targets are allowed, TTL and archival, and which write policy per class applies (STD-VER-012).
- **Rationale:** Governance by branch kind (FR-VER-022, FR-VER-024). Informal terms elsewhere in this volume map to these kinds: a work branch is a `draft` or `feature` branch, a simulation branch a `sandbox` branch, an import branch a `loader` branch (DEC-032, OQ-006).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a direct commit to protected `main` for a class that requires review, when flushed, then `VER.CHANGE_SET_REQUIRED` is returned.
- **Verification:** CONF, SEC
- **Origin:** FR-VER-022, FR-VER-024, FR-VER-026, STD-VER-011, STD-VER-012

### DSN-DVM-207 — Branch names and lifecycle
- **Statement:** Branch names MUST be validated against STD-VER-013 (`VER.INVALID_BRANCH_NAME`). Branch lifecycle states (active, frozen, merged, archived, deleted-soft) MUST be stored on the branch object and enforced on write. Archived branches remain readable; soft-deleted branches become invisible but their commits remain for retention.
- **Rationale:** FR-VER-023, FR-VER-025.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-TenantAdministrator
- **Acceptance:**
  1. Given a frozen branch, when a process tries to flush to it, then it fails without partial effects.
- **Verification:** CONF
- **Origin:** FR-VER-023, FR-VER-025, STD-VER-013

### DSN-DVM-208 — Head compare-and-set
- **Statement:** Every flush MUST perform `cas_branch_head(branch, expected_head, new_head)` in the storage transaction. A mismatch MUST abort the transaction and return `TXN.HEAD_MOVED` to the sequencer, which retries base validation (DSN-DVM-510) against the new head. A sequencer MUST NOT validate a process against a view (expected head plus the commits already in its group) that is older than the process's snapshot; such a process waits for the next group (found by VER-FORM-0001, DEC-031).
- **Rationale:** Single-writer order per branch without trusting any node.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given two nodes flushing to the same branch without a leader, when run 10,000 times, then no commit is lost, and history is linear.
- **Verification:** SIM, FORM
- **Origin:** STD-TXN-021, STD-VER-010

## Change sets

### DSN-DVM-220 — Change-set store
- **Statement:** A change set MUST be a System-kind object pointing to a source branch, a target branch, and a base commit. It MUST carry a lifecycle (draft, in review, approved, merging, merged, rejected, abandoned per STD-VER-020), items (the diff), comments, approvals and a preview hash. Its items MUST be derived from the diff and never edited directly.
- **Rationale:** Reviews are data (FR-VER-031).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, STU
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a change set, when read at any past `cseq`, then its state and items at that time are returned.
- **Verification:** CONF
- **Origin:** FR-VER-031, STD-VER-020

### DSN-DVM-221 — Preview hash and approval invalidation
- **Statement:** The preview hash MUST be the domain-tagged hash of `(source head, target head, merge result root, conflict list)`. Each approval MUST record the preview hash it approved. Any change to the source or target that changes the preview hash MUST mark earlier approvals as stale (STD-VER-021). Merge MUST require approvals whose hash equals the current preview hash.
- **Rationale:** What was approved is what is merged (FR-VER-032).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given an approved change set, when the source changes one field, then the approval becomes stale, and merge is refused.
  2. Given a target change that does not affect the merge result, when recomputed, then the approval stays valid.
- **Verification:** CONF, PROP
- **Origin:** FR-VER-032, FR-VER-034, STD-VER-021

### DSN-DVM-222 — Review summary
- **Statement:** The DVM MUST compute a review summary for a change set: counts by class and change type, the ledger effects, the decisions and sheets that changed, risk flags (breaking model change, retroactive valid time, locked-property override attempts), and the approvers required by the applicable approval rule.
- **Rationale:** NR-USE-003 review efficiency.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, STU
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a change set with a retroactive change, when summarised, then the retroactive flag and affected period are present.
- **Verification:** CONF, USE
- **Origin:** FR-VER-033, NR-USE-003

### DSN-DVM-223 — Target drift
- **Statement:** When the target head moves, the DVM MUST recompute the preview lazily (on the next read of the change set or before merge), MUST mark the change set "drifted" if new conflicts appear, and MUST NOT rebase the source automatically.
- **Rationale:** FR-VER-034.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a target commit that conflicts with the change set, when the change set is read, then it is marked drifted with the conflicts listed.
- **Verification:** CONF
- **Origin:** FR-VER-034

### DSN-DVM-224 — Partial acceptance
- **Statement:** Partial acceptance (STD-VER-022) MUST create a derived change set containing only the selected items and their closure (items they depend on by reference or derivation). Selections that are not closed MUST be rejected with `VER.SELECTION_NOT_CLOSED` listing the missing items.
- **Rationale:** Safe cherry-picking.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given an item that references a new object in another item, when only the first is selected, then `VER.SELECTION_NOT_CLOSED` names the second.
- **Verification:** CONF
- **Origin:** FR-VER-035, STD-VER-022

### DSN-DVM-225 — Item comments
- **Statement:** Comments on change-set items MUST be stored as System-kind objects that anchor to `(object id, field path, source version)`. When the item changes, anchors MUST be kept and marked "outdated".
- **Rationale:** FR-VER-036.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, STU
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a comment on a field that later changes, when viewed, then it is shown as outdated with the old value.
- **Verification:** CONF
- **Origin:** FR-VER-036

### DSN-DVM-226 — Merge of a change set
- **Statement:** Merging a change set MUST run as one process: verify approvals against the current preview hash; run the merge engine (DSN-DVM-240); validate the result (DSN-DVM-246); flush one merge commit; and set the change set to merged in the same flush.
- **Rationale:** Atomic merge (FR-VER-056).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a crash between merge validation and flush, when recovered, then neither the merge commit nor the merged state exists.
- **Verification:** FAULT
- **Origin:** FR-VER-056, STD-VER-056

## Diff

### DSN-DVM-230 — Tree-based diff
- **Statement:** Diff between two states MUST walk the two prolly trees in parallel, skipping subtrees with equal hashes (STD-VER-030, STD-VER-031). Its cost MUST be O(changes × log n). Diff output MUST be a stream of `DiffEntry` records ordered by object ID.
- **Rationale:** NR-PERF-005.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given two states of 10,000,000 objects differing in 100, when diffed, then no more than ~100 × tree height node reads occur.
- **Verification:** BENCH, PROP
- **Origin:** FR-VER-041, FR-VER-045, STD-VER-030, STD-VER-031, NR-PERF-005

### DSN-DVM-231 — Semantic diff
- **Statement:** For each changed object the DVM MUST compute a field-level semantic diff: added, removed and changed fields, moved list items by identity key, rich-text changes per block, and valid-time segment changes. Semantic diffs MUST be computed after lens alignment when schemas differ.
- **Rationale:** FR-VER-042.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a reordered list of line items with one edited, when diffed, then one move group and one field change are reported.
- **Verification:** CONF
- **Origin:** FR-VER-042

### DSN-DVM-232 — Large and permission-aware diffs
- **Statement:** Diffs MUST be paged by a cursor that holds both commit IDs and the last object ID. Entries for objects the caller cannot read MUST be replaced by a redacted counter per class; masked fields MUST be marked "changed" without values.
- **Rationale:** FR-VER-043, FR-VER-044.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a diff containing restricted objects, when read by a user without access, then counts are shown, and no restricted value is present.
- **Verification:** SEC, CONF
- **Origin:** FR-VER-043, FR-VER-044

### DSN-DVM-233 — Diff at valid time
- **Statement:** The DVM MUST support a diff of two valid-time points on the same commit (what changed in effect between two dates), computed from `temporal_index`.
- **Rationale:** Business users ask "what changed between these dates".
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given a fee schedule with three future segments, when diffed across two dates, then only the segment changes in between are reported.
- **Verification:** CONF
- **Origin:** FR-TIME-033, FR-VER-041

## Merge

### DSN-DVM-240 — Merge engine pipeline
- **Statement:** The merge engine MUST run these steps:
  1. find the merge base (DSN-DVM-241);
  2. merge the model (DSN-DVM-124);
  3. compute the two diffs from the base;
  4. classify each object as one-sided or two-sided;
  5. merge two-sided Definition objects per field (DSN-DVM-242);
  6. append ledger entries (DSN-DVM-244);
  7. apply kind-specific rules (DSN-DVM-245);
  8. apply recorded resolutions (DSN-DVM-247);
  9. validate (DSN-DVM-246);
  10. produce either a `MergePreview` or a merge commit.

  The engine MUST be a pure function of the three states and the resolutions.
- **Rationale:** Deterministic, previewable merge.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Phase note:** PH-1 step 9a (DEC-062, finding 1): the merge preview of the kernel builds the full states of both branches and is linear in the objects of the VAE (36 s at 20,000 objects on the workstation); it is replaced by a walk of the tree diff of the two heads against the base (STD-VER-031) before the ENV-REF-SERVER benchmark (MET-PERF-014). Done in step 9b: the preview reads the objects whose tree keys differ between each merge base and the source head, and the ledger keys of the entries the source commits appended, from the indexes in bulk, and merges only those (steps 3 and 4 of the pipeline); every other object keeps the target's version without being read. Workstation, 1,000 changed objects on 20,000: p50 0.37 s, p95 0.40 s (36 s before); 300 on 300: 0.12 s (0.4 s before). The full states are still built when a branch has a parent line (STD-VER-080) or a commit record lacks its appended entries. The best-common-ancestor search (DSN-DVM-241) marks the dominated common ancestors in one walk instead of one walk per pair, so a long history no longer dominates the preview (0.45 s on a 96-commit history of 300 objects before, 0.12 s after).
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the merge vectors, when merged, then results and conflict lists are byte-identical to the vectors.
- **Verification:** CONF, PROP, FORM
- **Origin:** FR-VER-051, STD-VER-050, STD-VER-056

### DSN-DVM-241 — Merge base search
- **Statement:** The merge base MUST be found by a bidirectional walk over first-parent and merge parents using commit generation numbers (stored per commit) to prune the search. Criss-cross cases MUST use the rule of STD-VER-040.
- **Rationale:** Correct base in bounded time.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the merge-base vectors including criss-cross histories, when computed, then the base equals the vectors.
- **Verification:** CONF, PROP
- **Origin:** STD-VER-040

### DSN-DVM-242 — Field merge
- **Statement:** For two-sided Definition objects the engine MUST merge per field using the declared strategy (STD-VER-051): conflict (default), prefer-target, prefer-source, max, min, sum-of-deltas, union-of-set, or a named merge port. Timeline segments MUST be merged per segment boundary. Lists with identity keys MUST be merged per item.
- **Rationale:** Fewer false conflicts, explicit policy (FR-VER-053).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a field with sum-of-deltas and edits +5 and +3 from base 10, when merged, then the result is 18.
- **Verification:** CONF, PROP
- **Origin:** FR-VER-053, STD-VER-051

### DSN-DVM-243 — Complete conflict report
- **Statement:** The engine MUST NOT stop at the first conflict. It MUST return every conflict as a `MergeConflict` with object, field path, base, source and target values (masked per caller), and the allowed resolutions. Merge MUST fail with `VER.MERGE_CONFLICT` carrying the full list.
- **Rationale:** FR-VER-052.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given 250 conflicts, when merged, then all 250 are reported in one response with paging.
- **Verification:** CONF
- **Origin:** FR-VER-052, STD-VER-052

### DSN-DVM-244 — Ledger append on merge
- **Statement:** Ledger entries created on the source since the base MUST be appended on the target in source commit order, receiving new target sequence numbers (STD-KIND-022). The merge MUST keep a mapping `source entry id → target entry id` in the merge commit. Period locks on the target (FR-LEDG-061) MUST be checked; a closed period MUST produce a conflict, not a silent move.
- **Rationale:** Ledgers are never merged by value (FR-VER-054).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundAccountant
- **Acceptance:**
  1. Given 1,000 postings on a work branch and 500 on main, when merged, then main has 1,500 postings with gap-free sequences, and balances equal the sums.
  2. Given a source posting into a closed target period, when merged, then a conflict naming the period is reported.
- **Verification:** CONF, PROP
- **Origin:** FR-VER-054, FR-LEDG-041, FR-LEDG-061, STD-VER-054, STD-KIND-022

### DSN-DVM-245 — Kind-specific merge rules
- **Statement:** System-kind objects MUST merge per their kind rule (STD-VER-053): lifecycle objects by transition legality, counters by sum of deltas, sets by union. RawDocument objects MUST conflict on any two-sided change unless a merge port is declared.
- **Rationale:** Correct merge semantics per kind.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given both sides moving a lifecycle to different states, when merged, then a conflict lists both transitions.
- **Verification:** CONF
- **Origin:** STD-VER-053

### DSN-DVM-246 — Validation of the merged result
- **Statement:** The merged state MUST be validated like a process flush: validation plans, unique keys, referential integrity at valid time, sheet invariants and ledger balance rules. Violations MUST be reported as merge conflicts of type "validation".
- **Rationale:** A merge never produces invalid state (FR-VER-055).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given two branches that each add an object with the same natural key, when merged, then a validation conflict is reported.
- **Verification:** CONF, PROP
- **Origin:** FR-VER-055, STD-VER-055

### DSN-DVM-247 — Recorded resolutions
- **Statement:** Resolutions MUST be stored as `Resolution` objects keyed by `(object id, field path, base value hash, source value hash, target value hash)`. They MUST be re-applied automatically when the same conflict recurs (STD-VER-058). A re-applied resolution MUST be reported in the preview.
- **Rationale:** FR-VER-058.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a resolved conflict and a re-merge after unrelated changes, when previewed, then the resolution is re-applied and shown.
- **Verification:** CONF
- **Origin:** FR-VER-058, STD-VER-058

### DSN-DVM-248 — Merge preview
- **Statement:** A merge preview MUST run the full pipeline without flushing and return a `MergePreview` with the result root, conflicts, validation outcomes, ledger mapping and preview hash. Previews MUST be cached by `(source head, target head, resolution set hash)`.
- **Rationale:** NR-PERF-006.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a preview and a merge with no intervening change, when compared, then the merge commit's state root equals the preview's result root.
- **Verification:** CONF, BENCH
- **Origin:** FR-VER-057, STD-VER-057, NR-PERF-006

### DSN-DVM-249 — Merge formal model
- **Statement:** The merge pipeline MUST be modelled in TLA+ or an equivalent formal tool, with properties: determinism; no lost ledger entries; conflicts are complete; merge commit parents are correct. The model MUST be kept in the `ubos-verify` repository and checked in CI.
- **Rationale:** Merge is the most error-prone kernel algorithm.
- **Priority:** Must · **Phase:** PH-0 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the model with 3 branches and 4 commits per branch, when model-checked, then no property is violated.
- **Verification:** FORM
- **Origin:** STD-VER-050, STD-VER-054, NR-MAINT-002

## Revert

### DSN-DVM-250 — Revert of a commit
- **Statement:** Reverting a commit MUST create a new process that computes the inverse changes of the commit's Definition changes against the current head (three-way, with the reverted commit as base), and appends reversal entries for its ledger entries (STD-KIND-023). Conflicts MUST be reported like merge conflicts.
- **Rationale:** History is never rewritten (FR-VER-071, FR-VER-074).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a commit with 3 updates and 2 postings, when reverted, then the updates are undone, 2 reversal entries exist, and balances return to their prior values.
- **Verification:** CONF, PROP
- **Origin:** FR-VER-071, FR-VER-074, STD-VER-070, STD-KIND-023

### DSN-DVM-251 — Revert of a process or change set
- **Statement:** Reverting a process or a merged change set MUST revert all its commits in reverse order within one process (STD-VER-071).
- **Rationale:** FR-VER-072, FR-VER-073.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a merged change set of 5 commits, when reverted, then one revert commit restores the pre-merge state for all affected objects.
- **Verification:** CONF
- **Origin:** FR-VER-072, FR-VER-073, STD-VER-071
- **Phase note:** PH-1 step 7c (DEC-060): `revert` of a process takes `resolutions` like a merge (`source` keeps the reverted value, `target` the later change) and reverts the chunk commits of a bulk process with it (STD-TXN-070); dependents are reported, not reverted (STD-VER-071). Exercised by SCN-018 in `verify/reports/exit-1-13/`.

### DSN-DVM-252 — Dependent process report
- **Statement:** Before reverting, the DVM MUST list later processes that read or derived from the changes being reverted, using `derivation_edge` and read-set records (DSN-DVM-512).
- **Rationale:** FR-VER-075.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a NAV computed from a price that is later reverted, when the revert is previewed, then the NAV process is listed.
- **Verification:** CONF
- **Origin:** FR-VER-075, FR-TXN-043

## Inheritance between branches

### DSN-DVM-260 — Inheritance chain record
- **Statement:** Each branch MUST record its read-through parent and fork point. The DVM MUST reject cycles (`VER.INHERITANCE_CYCLE`) and chains deeper than the configured limit (default 8, FR-VER-084).
- **Rationale:** Bounded read amplification.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a chain at the limit, when another level is added, then the operation is rejected.
- **Verification:** CONF
- **Origin:** FR-VER-084, STD-VER-080

### DSN-DVM-261 — Read-through resolution
- **Statement:** A read on a branch MUST look up the local head index first, then each ancestor at its fork point (or tracked upstream point), and return the first hit. Hidden objects (tombstone overrides, FR-VER-083) MUST stop the search. Each result MUST carry its provenance: the branch and commit where the effective version was found (STD-VER-081).
- **Rationale:** FR-VER-081, FR-VER-082, FR-VER-085.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given a template VAE and a tenant override of 3 objects, when reading all objects, then 3 come from the tenant and the rest from the template, with correct provenance.
- **Verification:** CONF, PROP
- **Origin:** FR-VER-081, FR-VER-082, FR-VER-083, FR-VER-085, STD-VER-080, STD-VER-081

### DSN-DVM-262 — Queries over inherited state
- **Statement:** Queries on a branch with inheritance MUST merge index scans from each level with local overrides taking precedence, using a k-way merge on the index key. Pagination cursors MUST record a position per level.
- **Rationale:** Correct, paginated queries over layered state.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given a 3-level chain and a query sorted by name, when paged by 50, then the concatenated pages equal the unpaged result.
- **Verification:** PROP
- **Origin:** FR-VER-081, AR-019

### DSN-DVM-263 — Upstream tracking and change detection
- **Statement:** A tracking branch MUST store the upstream commit it currently reads. Advancing the upstream pointer MUST be an explicit process that produces a report of overridden objects whose upstream versions changed (STD-VER-082).
- **Rationale:** Tenants adopt template updates deliberately (FR-VER-086).
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given a tenant override and an upstream change to the same object, when the tenant advances, then the report lists it, and the override stays in effect.
- **Verification:** CONF
- **Origin:** FR-VER-086, STD-VER-082

### DSN-DVM-264 — Inheritance and proofs
- **Statement:** The state root of a branch with inheritance MUST be computed over the effective state (local overrides plus inherited objects), so proofs verify without knowledge of the chain. The tree MUST share unchanged subtrees with the parent's tree by hash.
- **Rationale:** Proofs are independent of the storage layout (NR-DET-003).
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a tenant branch that inherits a template, when an inclusion proof is produced for an inherited object, then it verifies against the tenant commit's root.
- **Verification:** CONF, PROP
- **Origin:** STD-PROOF-001, STD-PROOF-020, NR-DET-003

## Tags and releases

### DSN-DVM-270 — Immutable and signed tags
- **Statement:** Tags MUST point to a commit, MUST be immutable (`VER.TAG_IMMUTABLE`), and MAY be signed by an additional key (STD-VER-060).
- **Rationale:** FR-VER-061, FR-VER-064.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a tag, when an update is attempted, then `VER.TAG_IMMUTABLE` is returned.
- **Verification:** CONF
- **Origin:** FR-VER-061, FR-VER-064, STD-VER-060

### DSN-DVM-271 — Releases
- **Statement:** A release MUST be a System-kind object that names a tag, a model hash, a compatibility statement and release notes (STD-VER-061). Consumers MAY pin a VAE or branch to a release (FR-VER-063); pinned branches MUST resolve inherited content at the release commit.
- **Rationale:** Stable templates and Buk content.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a branch pinned to release 1.2, when upstream publishes 1.3, then reads still resolve to 1.2.
- **Verification:** CONF
- **Origin:** FR-VER-062, FR-VER-063, STD-VER-061

### DSN-DVM-272 — Snapshot selection
- **Statement:** Selectors (commit, tag, release, `time=`, `asof=`) MUST be resolved to `(branch, cseq, valid time)` by one resolver implementing STD-VER-090 and STD-ADDR-002. Conflicting selectors MUST fail with `ADDR.CONFLICTING_SELECTORS`.
- **Rationale:** One resolver avoids divergent semantics.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the selector vectors, when resolved, then the results equal the vectors.
- **Verification:** CONF
- **Origin:** FR-VER-101, FR-VER-104, STD-VER-090, STD-ADDR-002
