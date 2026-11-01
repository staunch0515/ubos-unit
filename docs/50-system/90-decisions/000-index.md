---
id: UBS-DEC-000
title: Decision Log
status: complete
phase: ALL
depends_on: [UBS-META-00, UBS-META-06]
---

# Decision Log

Decisions are never deleted. A replaced decision is marked
`Superseded by DEC-<NNN>`.

| ID | Title | Status |
|---|---|---|
| DEC-127 | PH-2 step 7 opened in parts 7a–7g with the Buk format outside the BPA standard; part 7a: schema evolution in the kernel — `Lens`, the computed classification, `change_class`, lenses on reads, writes, updates and merges, the totality check; STD-CLS-050 and -060…062 claimed | Accepted |
| DEC-128 | PH-2 step 7, part 7b: upstream change detection in the kernel — overrides recorded with their base version, `upstream_changes`, `resolve_upstream` by acknowledgement or three-way merge; STD-VER-082 claimed | Accepted |
| DEC-129 | PH-2 step 7, part 7c: the Buk format `ubos.buk.v1` (UBS-SYS-FRG-09) and Forge `new`, `pack`, `sign`, `verify` with dependency resolution into `forge.lock` | Accepted |
| DEC-130 | PH-2 step 7, part 7d: Buk installation — plan, loader branch, node-verified assets, change set, registry record with the `Buk` object; Server routes and `forge install` | Accepted |
| DEC-131 | PH-2 step 7, part 7e-1: Buk upgrades with class changes by lens or migration, overlays' upstream report, migrations as one-way lenses rewritten by bulk process, rollback by revert; step 7e split into 7e-1 and 7e-2 | Accepted |
| DEC-132 | PH-2 step 7, part 7e-2: Buk uninstall with dependency protection and read-only data, reinstall, deprecation notices, genesis upgrades as proposed change sets (`system` 0.2.0) | Accepted |
| DEC-133 | PH-2 step 7, part 7f, first half: model export as OpenAPI 3.1 and the `ubos.model-graph.v1` graph; the ontology Buk waits for a decision on its class names | Accepted |
| DEC-134 | PH-2 step 7, part 7f, second half: the genesis `ontology` Buk 0.2.0 with the content of FR-MODEL-091; fixture classes that collided with it renamed (option A) | Accepted |
| DEC-135 | PH-2 step 7 closed: models and packages (parts 7a–7f, DEC-127…134) — schema evolution and lenses, upstream changes, the Buk format and Forge commands, installation, upgrade, migration, rollback, removal, deprecation, genesis upgrades, model export and the ontology Buk; 61 step-7 vectors pass in memory and on PostgreSQL; deferred items listed with their target steps | Accepted |
| DEC-136 | PH-2 step 8 opened in parts 8a–8e; part 8a: authorization relations and relation-based access rules in the kernel; STD-LGS-050 and -051 claimed | Accepted |
| DEC-137 | PH-2 step 8, part 8b: field masking for viewers (reads, query rows, outbox payloads), clearances, the tenant lookup key, and masked fields refused in query filters and sorts; STD-LGS-052 claimed with part 8c | Accepted |
| DEC-138 | PH-2 step 8, part 8c: encrypted fields under subject keys, disclosure, key rotation, crypto-shredding and key unavailability; STD-OBJ-010, -011, -012 and STD-LGS-052 claimed | Accepted |
| DEC-139 | PH-2 step 8, part 8d: personal-data read logging in the security log, the subject access export, and security-log records of key rotations and erasures | Accepted |
| DEC-140 | PH-2 step 8 closed: protection (parts 8a–8d, DEC-136…139) — relation-based authorization, field masking, encrypted fields and crypto-shredding, read logging and the subject export; 48 vectors in memory and on PostgreSQL; open items assigned to steps | Accepted |
| DEC-141 | PH-2 step 9 opened in parts 9a–9e; part 9a: the `finance` Buk in `buks/finance` with the chart of accounts, fiscal periods, the journal ledger and the balance projection; tie-out tests generated from the frozen sample company; FIN-EXP-01 ties out | Accepted |
| DEC-142 | PH-2 step 9, part 9b: the January documents posted as balanced journal entries at their recording dates; the `gl-document` projection; FIN-EXP-02 and FIN-EXP-03 tie out | Accepted |
| DEC-143 | PH-2 step 9, part 9c: January closed with its close artefact, later January postings refused, the February items with the back-dated correction; `read_projection` takes every snapshot selector; FIN-EXP-15…17 tie out | Accepted |
| DEC-144 | PH-2 step 9, part 9d: financial statements as layout data rendered by an L2 port from the balance projections; the four US layouts and the `gl-kind` projection; FIN-EXP-04…07 tie out line by line | Accepted |
| DEC-145 | PH-2 step 9 closed: the `finance` Buk, part 1 (parts 9a–9d, DEC-141…144) — 12 tests tie out 466 figures in memory and on PostgreSQL; EXIT-2-04 passed by inspection (VER-INSP-0002, 0 findings); requirement coverage recorded and the rest assigned to steps | Accepted |

---

The table rows and records of DEC-001 … DEC-029 are in `01-records-dec-001-029.md`
(UBS-DEC-01); those of DEC-030 … DEC-059 in `02-records-dec-030-059.md` (UBS-DEC-02);
those of DEC-060 … DEC-089 in `03-records-dec-060-089.md` (UBS-DEC-03); those of
DEC-090 … DEC-109 in `04-records-dec-090-109.md` (UBS-DEC-04); those of DEC-110 …
DEC-126 in `05-records-dec-110-126.md` (UBS-DEC-05); the records from DEC-127 on follow.

### DEC-127 — Models and packages, part 7a: schema evolution in the kernel (PH-2 step 7)
- **Date:** 2026-10-31 · **Status:** Accepted
- **Context:** PH-2 step 7 (UBS-PH-2 §11) covers three blocks:
  - schema evolution with lenses, the base ontology and model export (CAP-MODEL-08, -09, -10);
  - the Buk format with its dependencies, installation, upgrade, removal, verification and signing (CAP-PKG-01…07);
  - `forge new` and `forge pack`, moved here by DEC-087.

  **Baseline.** On the kernel, none of the 51 vectors of STD-CLS-050 and -060…062 or the 10 of STD-VER-082 (BPA 1.0-rc8) passed: lenses, class changes and upstream changes were missing. The genesis Buks have their PH-1 form (DEC-077), and Forge lacks `new`, `pack`, `sign`, `verify` and `install`. The owner confirmed the plan of step 7 with the default suggestions.
- **Decision:**
  1. **Parts.**
     - 7a: schema evolution in the kernel (STD-CLS-050, -060…062).
     - 7b: upstream change detection (STD-VER-082).
     - 7c: the Buk format and its description; `forge new`, `pack`, `sign` and `verify`; dependency resolution with `forge.lock`.
     - 7d: installation — the install plan, the loader branch and change set, the installation registry with dependency protection (DSN-DVM-134), retrieval from local files with verification (DSN-NOD-422), and `forge install`.
     - 7e: upgrade and removal — upgrades merged into overlays with the upstream report, migrations shipped with Buks, rollback, uninstall with data retention, deprecation notices, and genesis upgrades proposed as change sets (DSN-NOD-421).
     - 7f: the full ontology Buk (DSN-NOD-424), and OpenAPI and model-graph export (DSN-DVM-132).
     - 7g closes the step.

     No new tests are written; the package parts have no conformance vectors, and their scenarios run in steps 18 and 20. The exchange (download, `forge publish`, keyless signing) stays in PH-4, air-gapped import in PH-3, and staged rollout in step 11.
  2. **The Buk format is not part of BPA 1.0.** It is a format of the UBOS product, specified by DSN-FRG-301…307, DSN-DVM-134 and DSN-NOD-407, -421 and -422, with a format description written in part 7c. The installation registry uses the platform class `Buk` of the `system` Buk, outside BPA 1.0 (DEC-077). No release candidate is opened in step 7.
  3. **Lenses (7a).** `crates/ubos-dvm/src/lenses.rs` follows the reference model.
     - **Class.** `Lens` joins the genesis registry as a Definition below `ConfigAsset`, and a commit that writes one checks it.
     - **Pure operations.** `classify_change`, `apply_lens` and `check_lens`.
     - **Class changes.** `change_class` classifies a class change over the effective fields, the ports and the other members, against the stored current versions. A lens-required change needs a total lens, and a breaking one is refused. The step is recorded in `kernel_meta` with its lens and the effective fields before and after.
     - **Application.** Reads return the current or the asked schema version. A change in an older version is lensed forward, and an update applies to its base in the current version.
     - **Merges.** A merge aligns base, source and target through an aligner that the DVM installs in the merge engine of `ubos-version`, which does not depend on the expression language.
  4. **Claim.** The conformance test claims STD-CLS-050 and -060…062.
  5. **Design notes.** DSN-DVM-121…124 get phase notes. A breaking change committed with a migration process (DSN-DVM-123) comes with the Buk migrations of part 7e.
- **Consequences:**
  - The 51 vectors pass in memory and on PostgreSQL 16, and the conformance test passes.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
- **Alternatives considered:**
  - Writing the Buk format into BPA 1.0 was rejected: the standard fixes the semantics of business processing, while a package format is a product concern that a second implementation need not share.
  - Compiling lens chains into one composed operation (DSN-DVM-121) was deferred: step-by-step application gives the same results, and the benchmarks of step 18 show whether composition is needed.
- **Affects:** STD-CLS-050, STD-CLS-060, STD-CLS-061, STD-CLS-062, DSN-DVM-121, DSN-DVM-122, DSN-DVM-123, DSN-DVM-124, DSN-FRG-301, DSN-DVM-134, DEC-077, DEC-087, DEC-126

### DEC-128 — Models and packages, part 7b: upstream change detection (PH-2 step 7)
- **Date:** 2026-10-31 · **Status:** Accepted
- **Context:** Part 7b of step 7 (DEC-127). On the kernel, none of the 10 vectors of STD-VER-082 (BPA 1.0-rc8) passed: a branch with a parent line could override inherited objects (STD-VER-080), but did not record what an override was based on, so a later change of the parent line went unnoticed. The upgrades of part 7e merge Buk updates into overlays and report them through this mechanism. The owner confirmed part 7b with the default suggestions.
- **Decision:**
  1. **Overrides.** When a commit on a branch with a parent line writes its own version of an inherited object, the parent-line version it was based on is recorded. The record is a `kernel_meta` map per branch (`overrides:<branch>`), written in the same flush as the commit. Commits on branches without a parent line do not touch it.
  2. **Detection.** `upstream_changes` resolves the parent line at its head, as the branch reads it (live or pinned), and lists the overrides whose parent-line head differs from the recorded version, with `before` and `after`; `after` is null when the parent line no longer has the object.
  3. **Resolution.** `resolve_upstream` with `acknowledge` records the parent head as the new base. With `merge` it runs the three-way merge of `ubos-version` on that one object (base = the recorded version, source = the parent head, target = the branch's version) with recorded resolutions, fails with `VER.MERGE_CONFLICT` on a conflict left, and commits the merged body when it differs from the branch's version; the new base is recorded in that commit. A parent version that ends the object can only be acknowledged (`MODEL.CONSTRAINT_VIOLATED`).
  4. **Claim.** The conformance test claims STD-VER-082.
  5. **Design note.** DSN-DVM-263 gets a phase note: the report is available at any time, not only when the upstream pointer advances.
- **Consequences:**
  - The 10 vectors pass in memory and on PostgreSQL 16, and the conformance test passes.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
- **Alternatives considered:**
  - One metadata key per override was rejected for now: the metadata store has no prefix listing, and a per-branch map is read once per report. If overlays with many overrides make the map large, a table with an index replaces it (step 18 benchmarks).
  - Detecting changes when the parent line commits (pushing marks to every child branch) was rejected: it makes a parent commit depend on its children, while comparing on request costs one parent-line view.
- **Affects:** STD-VER-082, STD-VER-080, DSN-DVM-263, DEC-127

### DEC-129 — Models and packages, part 7c: the Buk format and its Forge commands (PH-2 step 7)
- **Date:** 2026-10-31 · **Status:** Accepted
- **Context:** Part 7c of step 7 (DEC-127). Forge had `init`, `check`, `types`, `test`, `dev`, `simulate` and `conform`, but no Buk: the genesis Buks have their PH-1 form (DEC-077), and `new`, `pack`, `sign` and `verify` were missing (FR-DEV-011, DSN-FRG-301…305). The node's installation in part 7d reads the same archives, and `ubos-node` may not depend on `ubos-forge` (UBS-ARC-07 §2). The owner confirmed part 7c with the default suggestions.
- **Decision:**
  1. **Format.** `ubos.buk.v1` is described in UBS-SYS-FRG-09. In short:
     - an archive is one canonical JSON document holding the manifest, the path tree, the chunks (one per file), the root and the detached signatures;
     - the root is `content_id("ubos.chunk.v1", {"buk": {manifest, tree}})`;
     - a signature is a DAT-Signature by the publisher's key-form DID over the root.
  2. **Shared code.** The archive, its integrity check, the signatures, and semantic versions with ranges are in `ubos-dvm::buk`. Forge and the node both use it, so no new crate is needed.
  3. **Commands** (`crates/ubos-forge/src/package.rs`):
     - `forge new` scaffolds a Buk project with a namespace;
     - `forge pack` runs `forge check` (seed data included), resolves dependencies and writes the archive, optionally signed;
     - `forge sign` adds a signature;
     - `forge verify` runs the checks of UBS-SYS-FRG-09 §6 and writes a reproducible record, signed with `--key`.

     The manifest keys stay in the `[project]` table of `forge.toml`; UBS-SYS-FRG-06 lists them.
  4. **Dependencies.** Resolution runs against a local repository of `.buk` files. A deterministic backtracking solver prefers the previous lock and explains a conflict by every path. The result goes to `forge.lock` and the manifest member `locked`.
  5. **Coverage.** A port test counts toward the coverage of the function that implements the port. Before, only unit and property tests counted, so a function reached only through its port could not meet the coverage gate (DSN-FRG-104).
  6. **Errors.** The registered codes are used: `PKG.MANIFEST_INVALID` for a malformed or inconsistent archive, `PKG.SIGNATURE_INVALID`, `PKG.NAMESPACE_VIOLATION`, and `MODEL.LENS_REQUIRED` and `MODEL.MIGRATION_REQUIRED` for changes without a lens or a migration.
  7. **Design notes.** DSN-FRG-301…305 and -307 get phase notes.
- **Consequences:**
  - Each command was run on a scaffolded project:
    - packing twice gives the same root, and a changed manifest is refused;
    - an unsigned Buk or one without a trusted signer fails verification;
    - a missing capability request is listed;
    - a breaking class change without a lens or migration is listed;
    - a version conflict names both dependency paths;
    - the record is byte-identical on a second run.
  - No new tests were written (DEC-127); the package scenarios run in steps 18 and 20.
- **Alternatives considered:**
  - A tar or zip container with content-defined chunks was rejected for version 1. Sources are small structured files, a canonical JSON document needs no new dependency, and large binary content belongs in the blob store.
  - A full PubGrub solver was deferred. The backtracking solver gives the same answers on small graphs and explains conflicts by path; the exchange in PH-4 shows whether larger graphs need learned incompatibilities.
  - A new error code for archive integrity was rejected: `PKG.MANIFEST_INVALID` is registered and covers an archive whose manifest and content disagree.
- **Affects:** DSN-FRG-301, DSN-FRG-302, DSN-FRG-303, DSN-FRG-304, DSN-FRG-305, DSN-FRG-307, DSN-FRG-104, FR-DEV-011, UBS-SYS-FRG-06, UBS-SYS-FRG-09, DEC-077, DEC-127

### DEC-130 — Models and packages, part 7d: Buk installation (PH-2 step 7)
- **Date:** 2026-10-31 · **Status:** Accepted
- **Context:** Part 7d of step 7 (DEC-127). Buks could be packed, signed and verified (DEC-129), but not installed:
  - the node installed only its embedded genesis Buks (DEC-077);
  - the installation registry, the install plan and the loader branch were missing (FR-PKG-031…033, DSN-FRG-306, DSN-DVM-134, DSN-NOD-422).

  The `Buk` class belongs to the signed `system` genesis Buk. Changing it would stop existing VAEs until the genesis upgrades of part 7e. The owner confirmed part 7d with the default suggestions.
- **Decision:**
  1. **Kernel operations** (`crates/ubos-dvm/src/packages.rs`):
     - `buk_plan` computes the plan and writes nothing;
     - `buk_install` creates the loader branch, declares the new classes, commits the assets and the seed on the loader branch, verifies each asset there with `verify_asset`, and opens and submits the change set;
     - `buk_complete` approves and merges the change set, and records the `Buk` object with its registry record in one commit;
     - `buks` lists the registry.

     UBS-SYS-FRG-09 §7 describes them.
  2. **Checks before any write.** Integrity, signatures and trusted publishers are checked first. Then every problem of the plan blocks the installation:
     - namespace, class and asset conflicts;
     - missing dependencies;
     - another installed version or changed content, since upgrades are part 7e.

     Two Buks may share a namespace only when they have the same publisher.
  3. **Registry.** The `Buk` class is unchanged. Its object carries the Buk ID as name and the archive root as `content_hash`. The rest of the registry record is kept in the kernel metadata (`buks`), written in the flush of the commit that records the `Buk` object, until part 7e adds the members to the class by a genesis upgrade.
  4. **Server.** The administration interface gains the routes under `/api/v1/admin/buks`: `GET buks`, `POST buks/plan`, `POST buks/install` and `POST buks/complete`.
     - The administrator is the approver.
     - Installing and completing are audited.
     - An archive comes in the body, or as a file of the new key `buks.dir`.
     - Trusted publishers are the new key `buks.trusted_publishers`.
  5. **Forge.** `forge install` runs `forge verify` locally, which is the sandbox run of the Buk's scenarios. It then shows the plan and stages the Buk. `--plan` stops after the plan; `--approve`, or `forge install --complete <ID>`, completes the installation. `forge test` and installation share the asset object IDs.
  6. **Design notes.** DSN-FRG-306, DSN-DVM-134 and DSN-NOD-422 get phase notes.
- **Consequences:**
  - The flow was run on a Server over PostgreSQL:
    - the plan of a Buk with a missing dependency is blocked;
    - the dependency was staged, completed and recorded;
    - the dependent Buk was installed with its asset verified;
    - the ports of both Buks run on `main`;
    - installing again reports `present`, and another version is refused;
    - a changed chunk, an unsigned Buk, an untrusted publisher and a path outside `buks.dir` are refused before any commit.
  - No new tests were written (DEC-127).
- **Alternatives considered:**
  - Adding the registry members to the `Buk` class now was rejected. It changes the signed `system` genesis Buk, so it belongs with the genesis upgrades of part 7e.
  - Running the Buk's scenarios on the node was deferred. Forge already runs them on the development fixture before sending the Buk, and the node verifies every asset with its own tests on the loader branch.
- **Affects:** DSN-FRG-306, DSN-DVM-134, DSN-NOD-422, FR-PKG-031, FR-PKG-032, FR-PKG-033, STD-CTX-023, UBS-SYS-FRG-09, UBS-SYS-NOD-06, DEC-077, DEC-127, DEC-129

### DEC-131 — Models and packages, part 7e-1: upgrades, migrations and rollback (PH-2 step 7)
- **Date:** 2026-10-31 · **Status:** Accepted
- **Context:** Part 7e of step 7 (DEC-127) covers upgrade and removal. The owner confirmed splitting it in two:
  - 7e-1: upgrades, migrations and rollback;
  - 7e-2: uninstall, deprecation and genesis upgrades.

  After part 7d an installed Buk could not be upgraded: another version was refused (DEC-130). A breaking class change was refused without the migration processes of DSN-DVM-123 (DEC-127). Overlays had the upstream report (DEC-128), but nothing connected it to a Buk release.
- **Decision:**
  1. **Upgrade.** A later version of an installed Buk goes through the plan, loader branch and change set of an installation (UBS-SYS-FRG-09 §8).
     - **Refused:** a downgrade, and a version that an installed dependent's range excludes.
     - **Plan:** the class changes with their classification and means (none, lens or migration, one schema step), the assets changed or no longer shipped (kept), the seed objects to create, update or keep (changed locally), and the overridden objects per overlay.
     - **Staging:** applies the additive and lensed class changes, because declarations are shared by the branches of a VAE and data reads through the lens.
     - **Completion:** merges the change set, then applies the class changes by migration and rewrites their instances, commits the seed of those classes, records the new version with the previous one kept, and reports each overlay's upstream changes.
     - **Resumption:** a completion that stops part way is resumed by calling it again.
  2. **Migrations.**
     - A migration is a one-way lens under `migrations/`.
     - `change_class` accepts a breaking change with a `migration` that takes every stored body forward to a valid body, and records it as the step's lens.
     - `migrate_instances` rewrites a branch's own instances by a bulk process (STD-TXN-070) in commits of 1,000 objects. It runs on the target and on its overlays.
  3. **Rollback.**
     - `buk_rollback` reverts the merge of the Buk's last upgrade (STD-VER-071) and restores the previous record and `Buk` object.
     - A conflict on the change set's own state keeps that state; any other conflict is returned for the administrator's resolutions.
     - Class changes and rewritten instances stay (FR-PKG-044).
     - One upgrade is kept for a rollback.
  4. **Forge.**
     - `forge verify --previous` builds the previous version's fixture, upgrades it as an installation would, and runs the new version's tests on the migrated data (DSN-FRG-307). The test runner gains `run_with` for this.
     - `forge install --rollback` and the Server route `POST /api/v1/admin/buks/rollback` (audited) are added.
  5. **Design notes.** DSN-DVM-123, DSN-NOD-407 and DSN-FRG-307 get phase notes.
- **Consequences:**
  - The flow was run on a Server over PostgreSQL with `contoso/crm` 1.0.0 → 1.1.0 → 2.0.0:
    - 1.1.0 is an additive class change, a changed asset, one seed object updated and one created;
    - 2.0.0 makes `tier` required, with a migration that defaults it;
    - a completion interrupted after its merge was resumed;
    - two rollbacks restored the previous versions: 2.0.0 → 1.1.0 kept the migrated data; 1.1.0 → 1.0.0 restored the changed object, ended the created one, and the restored asset still runs.
  - The overlay flow was run in process, because UBTP's `create_branch` takes no parent line:
    - the plan listed the overlay's override;
    - completion reported its upstream change, and the three-way merge kept the tenant's change and took the vendor's;
    - the migration rewrote two objects on the target and only the overlay's own object on the overlay.
  - `forge verify --previous` refuses 2.0.0 without its migration.
  - No new tests were written (DEC-127).
- **Alternatives considered:**
  - Applying breaking class changes at staging was rejected: the rewrite would change the target before approval.
  - Several schema steps per upgrade were deferred: one step per class per upgrade keeps the lens and migration checks exact; a Buk that skips versions is upgraded through each one.
  - Passing a parent line through UBTP `create_branch` was left out: it is a protocol change (STD-PROTO), not a package change.
- **Affects:** DSN-DVM-123, DSN-NOD-407, DSN-FRG-307, FR-PKG-041, FR-PKG-042, FR-PKG-044, FR-TEN-032, STD-CLS-050, STD-TXN-070, STD-VER-071, STD-VER-082, UBS-SYS-FRG-09, DEC-127, DEC-128, DEC-130

### DEC-132 — Models and packages, part 7e-2: removal, deprecation and genesis upgrades (PH-2 step 7)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 7e-2 of step 7 (DEC-127, DEC-131) completes upgrade and removal. Before it:
  - an installed Buk could not be removed (FR-PKG-051, -053);
  - deprecations could not be expressed (FR-PKG-052);
  - a node carrying a newer genesis Buk refused to start on a VAE holding the older one (DEC-077), although genesis Buks must upgrade through the normal process (FR-PKG-073, DSN-NOD-421);
  - the registry members of DSN-DVM-134 were kept in kernel metadata only, because the `Buk` class could not change without a genesis upgrade (DEC-130).

  The owner confirmed part 7e-2 with the default suggestions.
- **Decision:**
  1. **Uninstall** (`buk_uninstall`, UBS-SYS-FRG-09 §9).
     - **Refused** while installed Buks depend on the Buk (they are named), for the genesis Buks, and for a Buk without a recorded archive.
     - **Behaviour goes:** the Buk's logic assets are retired, except those another installed Buk's class implements a port by; the configuration objects its seed created of classes not its own are ended.
     - **Data stays:** the Buk's classes and data stay readable and exportable but read-only (`retention: read_only`, FR-PKG-051). The platform's commits may still write them, for the disposal that retention will allow. The read-only classes are kept in the kernel metadata and held by the engine, so a commit pays no extra read.
     - **Flow:** the changes go through a loader branch and a change set; completion marks the classes read-only and records the Buk as `uninstalled` in one commit.
  2. **Reinstall.** Installing an uninstalled Buk takes the upgrade path. Retired assets are restored and verified, and the classes become writable again. Ended objects stay ended and are reported.
  3. **Deprecation** (FR-PKG-052).
     - A version's manifest may carry `deprecation` (`message`, `end_of_support`, `replacement`), from `forge.toml`.
     - A publisher may deprecate installed versions later with a notice signed with its key (`forge deprecate`, `POST /api/v1/admin/buks/deprecation`); the notice is accepted only from the Buk's publisher.
     - Plans warn, and the registry shows `supported`, `deprecated` or `end_of_support`.
     - Distribution through the exchange stays in PH-4 (DSN-EXC-008).
  4. **Genesis upgrades** (FR-PKG-073, DSN-NOD-421).
     - A genesis Buk is also a `ubos.buk.v1` archive (ID `ubos.platform/<name>`), signed with the genesis publisher key.
     - When a node process first opens a VAE holding an older version, it registers that version (`buk_register`) and stages the newer one as a normal upgrade with a submitted change set, which an administrator completes. `main` does not change until then.
     - A VAE holding a newer version than the node's is refused.
     - The check only proposes upgrades of genesis Buks the VAE records. It never installs one, because installing the genesis Buks is provisioning's work; the node suites showed that a fuller check broke VAEs that hold their own classes.
     - The proposal is made on first use and not by a sweep at start, because a sweep opened every VAE with its own connections and exhausted PostgreSQL's limit.
  5. **`system` 0.2.0.** The `Buk` class goes to schema version 2 with the registry members as optional fields (namespace, state, dependencies, capabilities, target, change set, deprecation). They are written once a VAE has completed its genesis upgrade, and the kernel metadata stays the registry. The manifest is re-signed; the `ontology` signature is unchanged.
  6. **Server and Forge.**
     - New routes `POST /api/v1/admin/buks/uninstall` and `…/deprecation`, both audited.
     - `forge install --uninstall` and `--notice`, and `forge deprecate`.
  7. **Design notes.** DSN-DVM-134, DSN-NOD-421 and DSN-EXC-008 get phase notes.
- **Consequences:**
  - On a Server over PostgreSQL:
    - a tenant created with `system` 0.1.0 got its upgrade proposed on first use; it was completed, the `Buk` object carries the new members, and a restart proposes nothing;
    - with `contoso/app` depending on `contoso/crm`, uninstalling `crm` was refused naming `app`;
    - `app` and then `crm` were uninstalled. Their data stayed readable, a write was refused as read-only, and the retired port no longer ran;
    - reinstalling `crm` restored and verified its asset, the port ran, and `Customer` was writable again while `app`'s `Account` stayed read-only;
    - a signed notice set `crm` to `end_of_support`, and a notice signed by another key was refused;
    - a version with a manifest deprecation is flagged in its plan.
  - No new tests were written (DEC-127).
- **Alternatives considered:**
  - Deleting a Buk's classes and data on uninstall was rejected: records are never lost by removing software (FR-PKG-051).
  - A sweep of every VAE at node start was replaced by the check on first use (above).
  - Keeping genesis Buks outside the Buk registry was rejected: FR-PKG-073 asks for no special paths.
- **Affects:** DSN-DVM-134, DSN-NOD-421, DSN-EXC-008, FR-PKG-051, FR-PKG-052, FR-PKG-053, FR-PKG-073, UBS-SYS-FRG-06, UBS-SYS-FRG-09, DEC-077, DEC-127, DEC-130, DEC-131

### DEC-133 — Models and packages, part 7f, first half: model export (PH-2 step 7)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 7f of step 7 (DEC-127) covers the full ontology Buk (DSN-NOD-424, FR-MODEL-091) and the model export (DSN-DVM-132). The owner confirmed it with the default suggestions.

  While preparing the ontology, a conflict appeared. FR-MODEL-091 gives the ontology the class names `Account`, `Document`, `Agreement`, `Location`, `Calendar`, `Address` and others. Class names are global within a VAE, and the Server refuses a project class whose name a genesis Buk holds in another namespace (DEC-083). FR-MODEL-093 also says tenants extend ontology classes and never redefine them. Fixtures in CI declare some of these names:
  - the SDK fixture declares `Account` and `Document`;
  - `verify/scenarios` declares `Agreement`;
  - the node, events and edge suites declare `Account`.

  Shipping the ontology as specified would make those fixtures fail on the Server. This is a consequence the confirmed plan did not name.
- **Decision:**
  1. **Split.** Part 7f is done in two halves. The model export is done now. The ontology Buk follows the owner's decision on the class names.
  2. **Export** (UBS-SYS-DVM-03, Model export formats).
     - `export_openapi` gives an OpenAPI 3.1 description of a model scope's REST binding: queries, reads and the ports of concrete classes, with class bodies as JSON Schema and problem details for errors.
     - `export_model_graph` gives the documented `ubos.model-graph.v1` graph: classes and traits with `extends`, `has_trait`, `references` and `implemented_by` edges.
     - Both are served by the Server at `POST /api/v1/admin/model/export` and run by `forge export`, offline on a project's model or against a Server.
  3. **Design note.** DSN-DVM-132 gets a phase note.
- **Consequences:**
  - The OpenAPI documents of the SDK fixture (13 paths), the scenario model (42 paths), a Buk's model and a Server VAE pass `openapi-spec-validator` with no errors.
  - The linter is not added to CI.
  - No new tests were written (DEC-127).
- **Alternatives considered:**
  - Describing the UBTP operations in OpenAPI was rejected: they are not HTTP operations. The REST binding is what OpenAPI tools can call.
- **Affects:** DSN-DVM-132, DSN-NOD-424, FR-MODEL-091, FR-MODEL-093, FR-MODEL-103, FR-MODEL-104, UBS-SYS-DVM-03, DEC-083, DEC-127

### DEC-134 — Models and packages, part 7f, second half: the ontology Buk (PH-2 step 7)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** DEC-133 left the ontology Buk (DSN-NOD-424, FR-MODEL-091) open. Its class names collide with classes that CI fixtures declare (`Account`, `Document`, `Agreement`). The owner chose option A: build the ontology as specified and rename the fixtures, instead of leaving the colliding names out.
- **Decision:**
  1. **Content.** The genesis `ontology` Buk 0.2.0 keeps the party–role core of PH-1 unchanged and adds:
     - the classes `Address`, `ContactPoint`, `Identifier` (schemes LEI, tax ID, registration number), `Account`, `Calendar`, `Document`, `Agreement` (abstract) and `Location`;
     - the traits `Addressable`, `Approvable`, `Classifiable` and `Attachable`, which apply to the concrete kinds.

     Every class and trait has a description. `Money` is the standard value type `money` (STD-FND-020); a second declaration would be a synonym. The manifest is re-signed.
  2. **Namespaces of genesis Buks.** Each genesis Buk is an archive in its classes' namespace: `ubos.platform/system` and `ubos.ontology/ontology`. With one namespace for both, the ontology's own classes were refused as foreign.
  3. **Delivery.** Existing VAEs receive the new version as a genesis upgrade (DEC-132). A VAE that declares one of the ontology's class names in its own namespace keeps the previous version, and the node reports `upgrade_blocked` with the plan's problems (FR-MODEL-093). A proposal that cannot be made no longer stops the node.
  4. **Fixtures renamed.** Class names only; the asset names and the event `AccountClosed` are unchanged.
     - The SDK fixture's `Account` and `Document` become `Wallet` and `Dossier`, with their users: the Rust and TypeScript SDK suites, the tours, the SDK README and the UI walk-through.
     - The scenario model's `Agreement` becomes `Contract` (`ServiceAgreement` extends it), through `gen_scenarios.py`.
     - The node, events and edge suites' `Account` becomes `Wallet`.
     - The generated model and types are regenerated with `forge types`.
  5. **Design note.** DSN-NOD-424 gets a phase note.
- **Consequences:**
  - A fresh VAE lists all 17 classes and traits with descriptions (FR-MODEL-091 acceptance), and their OpenAPI export passes `openapi-spec-validator`.
  - An older tenant received the proposal and completed it.
  - Tenants with their own `Account` were reported as blocked while the node kept running.
  - The CI steps the renames touch pass locally:
    - TypeScript typecheck, lint and build;
    - the SDK suite with both SDKs over both bindings;
    - the UI walk-through over both bindings (12 of 12);
    - the scenarios over UBTP (40 of 40, both bindings);
    - the operator scenarios (19 of 19);
    - the `forge dev` loop.
  - PH-1 reports keep their old class names as records of their runs.
  - No new tests were written (DEC-127).
- **Alternatives considered:**
  - Leaving `Account`, `Document` and `Agreement` out of the ontology (option B) was declined by the owner: domain Buks would define them again, against FR-MODEL-093.
- **Affects:** DSN-NOD-424, DSN-NOD-421, FR-MODEL-091, FR-MODEL-092, FR-MODEL-093, FR-MODEL-094, DEC-077, DEC-132, DEC-133

### DEC-135 — PH-2 step 7 closed: models and packages (PH-2 step 7, part 7g)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** PH-2 step 7 (DEC-127) was done in parts 7a–7f. Part 7e was split into 7e-1 and 7e-2 (DEC-131), and part 7f into two halves (DEC-133, DEC-134). Parts 7a–7f are recorded in DEC-127…134. Part 7g closes the step: it records what the step delivered and lists, in one place, what the parts deferred. The owner confirmed the part with the default suggestions.
- **Decision:**
  1. **Delivered.**
     - **7a (DEC-127).** Schema evolution in the kernel: `Lens`, the computed change classification, `change_class`, and lenses on reads, writes, updates and merges with the totality check. The Buk format stays outside BPA 1.0.
     - **7b (DEC-128).** Upstream change detection: overrides recorded with their base, `upstream_changes` and `resolve_upstream`.
     - **7c (DEC-129).** The Buk format `ubos.buk.v1` (UBS-SYS-FRG-09), plus:
       - `forge new`, `pack`, `sign` and `verify`;
       - dependency resolution into `forge.lock`;
       - port tests counted toward coverage.
     - **7d (DEC-130).** Installation: the plan, the loader branch with assets verified by the node, the change set, the registry, the Server routes and `forge install`.
     - **7e-1 (DEC-131).** Upgrades:
       - class changes by lens or migration;
       - migrations as one-way lenses rewritten by bulk process;
       - the overlays' upstream report;
       - rollback by revert;
       - `forge verify --previous` on migrated data.
     - **7e-2 (DEC-132).** Removal and lifecycle:
       - uninstall with dependency protection and read-only data, and reinstall;
       - deprecation in manifests and by signed notices;
       - genesis upgrades proposed per VAE (`system` 0.2.0).
     - **7f (DEC-133, DEC-134).** Model export as OpenAPI 3.1 and as the `ubos.model-graph.v1` graph. The `ontology` Buk 0.2.0 with the content of FR-MODEL-091; fixtures that collided with it were renamed.
  2. **Verification.**
     - The 61 current vectors of the step-7 clauses pass in one run in memory and on PostgreSQL 16: STD-CLS-050 and -060…062 (51) and STD-VER-082 (10).
     - The kernel conformance test claims these clauses.
     - The Buk flows, which have no vectors (DEC-127), were run on a Server over PostgreSQL in each part. The OpenAPI exports pass `openapi-spec-validator`.
  3. **Deferred.** Each item keeps its ID and moves to the target named:

     | Item | Source | Target |
     |---|---|---|
     | Retrieval from the exchange (CTR-054), `forge publish` (DSN-FRG-308), keyless signing (DSN-FRG-304), notices, yanking and advisories through the exchange (DSN-EXC-008) | DEC-127, DEC-129, DEC-130, DEC-132 | PH-4 |
     | Air-gapped import of Buks | DEC-127 | PH-3 |
     | Staged rollout of upgrades across VAEs with gates (FR-PKG-043) | DEC-127 | step 11 |
     | Disposal of an uninstalled Buk's read-only data by retention schedules (FR-PKG-051, FR-AUD-071) | DEC-132 | PH-3 |
     | Overlay branches created through UBTP (`create_branch` with a parent line; a protocol change) | DEC-131 | step 16, with Studio's overlay editing |
     | A PubGrub solver with learned incompatibilities (DSN-FRG-303) | DEC-129 | PH-4, if the exchange's dependency graphs need it |
     | Compiled lens chains (DSN-DVM-121) and an indexed override table (DEC-128) | DEC-127, DEC-128 | step 18, if the benchmarks need them |
     | The migration of 100,000 instances across a node restart (DSN-DVM-123 acceptance) and the scenario SCN-013 | DEC-131 | steps 18 and 20 |
     | Several schema steps of one class in one upgrade; reviving configuration objects ended by an uninstall when the Buk is reinstalled | DEC-131, DEC-132 | when a Buk needs them |
     | `openapi-spec-validator` in CI | DEC-133 | step 20, if the owner asks for it (CLAUDE.md rule 4) |

  4. **Owner actions.** Merge the branch, then sign the tags `bpa-v1.0.0-rc8` to `bpa-v1.0.0-rc12`. Step 7 opened no release candidate.
  5. **Next.** Step 8 (protection): relation-based authorization, masking and filters, encrypted fields and crypto-shredding, access logs. Its plan goes to the owner first.
- **Consequences:**
  - The status table in `docs/reports/ph2-plan-zh.md` marks step 7 as done.
  - The deferred items are picked up by the steps named in the table above, whose plans cite this decision.
- **Alternatives considered:**
  - Leaving the deferred items spread over DEC-127…134 was rejected, as in DEC-104, DEC-111, DEC-118 and DEC-126.
- **Affects:** DEC-127, DEC-128, DEC-129, DEC-130, DEC-131, DEC-132, DEC-133, DEC-134, CTR-054, DSN-FRG-303, DSN-FRG-304, DSN-FRG-308, DSN-EXC-008, DSN-DVM-121, DSN-DVM-123, FR-PKG-043, FR-PKG-051, FR-AUD-071

### DEC-136 — Protection, part 8a: relation-based authorization (PH-2 step 8)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** PH-2 step 8 (UBS-PH-2 §11) covers protection:
  - relationship authorization (CAP-IAM-05);
  - masking and filtering (CAP-IAM-06);
  - encrypted fields and crypto-shredding (CAP-PROOF-07);
  - access logging (CAP-AUD-05).

  Its BPA clauses were released in 1.0-rc8 (DEC-093): STD-LGS-050…052 and STD-OBJ-010, -011. On the kernel, 2 of their 43 vectors passed, 17 failed, and 24 were not applicable for lack of the operations. The owner confirmed the plan with the default suggestions.
- **Decision:**
  1. **Parts.**
     - 8a: authorization relations and access rules (STD-LGS-050, -051; DSN-DVM-810…813).
     - 8b: field masking and masked fields in queries and expressions (STD-LGS-052; DSN-DVM-821, -822).
     - 8c: encrypted fields under subject keys, equality lookups, crypto-shredding and key unavailability (STD-OBJ-010, -011; DSN-DVM-830, -833, -834).
     - 8d: personal-data read logging on the node and the subject access export (DSN-DVM-823, -837).
     - 8e closes the step.

     No release candidate is opened. Who-can and policy impact (DSN-DVM-840, -841) belong to access review (CAP-IAM-08, PH-3), and break-glass (DSN-DVM-808) to identity; neither is part of this step.
  2. **Declarations.** `ubos-model` checks `authz_relation`: a member name, on a reference field or a list of references. It also checks `principal_field`, which must name a text field.
  3. **Rules.** `ubos-rules` registers the kernel property `access` (enforcement `action`, combined by union) and checks its paths when a sheet is committed (`RULE.PARSE_ERROR`).
  4. **Evaluation** (`crates/ubos-dvm/src/access.rs`).
     - Relations come from the objects' versions at the snapshot of the request. Paths are followed by reading the referenced objects at that snapshot. `relations` and `check_access` are adapter operations.
     - A read made `as` a viewer is refused with `IAM.NOT_FOUND`.
     - A query made `as` a viewer leaves out, inside its execution, the objects the viewer may not read, so pages and counts agree.
     - A port call is refused with `IAM.DENIED`. An object created in the same process is not checked, because it is not yet at a snapshot.
     - Without a live sheet declaring `access`, nothing is evaluated.
  5. **Claim.** The conformance test claims STD-LGS-050 and -051.
  6. **Design notes.** DSN-DVM-810…813 get phase notes. The group-expansion cache (DSN-DVM-813) is not built, since rc8 paths have no group closure, and moves to step 18 if needed.
- **Consequences:**
  - The 17 vectors pass in memory and on PostgreSQL 16, and the conformance test passes.
  - A viewer query with rules present evaluates every candidate of its class once; the benchmarks of step 18 decide whether that needs an index.
- **Alternatives considered:**
  - Keeping relation tuples in an index maintained by the flush (DSN-DVM-810's `relationship_index`) was deferred. Deriving them from the versions at the snapshot is what STD-LGS-050 requires and needs no second store; an index is an optimisation for step 18.
- **Affects:** STD-LGS-050, STD-LGS-051, DSN-DVM-810, DSN-DVM-811, DSN-DVM-812, DSN-DVM-813, DSN-DVM-840, DSN-DVM-841, DSN-DVM-808, DEC-093

### DEC-137 — Protection, part 8b: field masking (PH-2 step 8)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 8b of step 8 (DEC-136) covers field masking and masked fields in queries (STD-LGS-052; DSN-DVM-821, -822). STD-LGS-052 has 11 vectors. One of them (VER-CONF-5364) masks an erased value and needs `erase_subject`, which part 8c builds.
- **Decision:**
  1. **Viewer outputs** (`crates/ubos-dvm/src/masking.rs`). Outputs made `as` a viewer `{principal, clearances, ctx}` are masked after authorization:
     - read bodies;
     - the selected values of query rows;
     - outbox payloads, by the masks of each effect's object.
  2. **Modes.** The sheet property `mask` (and `mask.<field>`) is resolved by the cascade with the viewer's `ctx`.
     - `hide` removes the field.
     - `partial` keeps the last four characters of a text and stars the rest.
     - `hash` gives `hmac-sha256:` and the keyed hash of the canonical value under the tenant lookup key.
     - `partial` on a value that is not a text, and `hash` without a lookup key or on an erased marker, hide the field.
  3. **Clearances.** A field whose classification the viewer's clearances do not cover is hidden unless a mask declares another mode. Clearances default to `internal`; an unknown level is `MODEL.INVALID_VALUE`.
  4. **Lookup key.** The adapter operation `lookup_key {key}` sets the tenant lookup key (32 bytes in lowercase hexadecimal, kernel metadata `lookup_key`). Part 8c uses the same key for equality lookups on encrypted fields (STD-OBJ-012).
  5. **Queries.** A query made as a viewer that filters or sorts on a field masked for the viewer on an object it may read fails with `QRY.INVALID_QUERY`, with the masked names in its details. The check runs in the same pass as the access check of DEC-136.
  6. **Claim.** STD-LGS-052 is claimed by the conformance test with part 8c, when VER-CONF-5364 can run.
  7. **Design notes.** DSN-DVM-821 and -822 get phase notes. Diffs and explanations have no viewer form in the adapter yet. They are masked when they gain one; the access export is masked with part 8d.
- **Consequences:**
  - 10 of the 11 vectors pass in memory and on PostgreSQL 16; VER-CONF-5364 is not applicable until part 8c.
  - Without a live sheet declaring `mask`, only classifications are checked.
- **Alternatives considered:**
  - Masking query rows inside the query execution was rejected: the selected values are masked after the page is built, as for reads, and only filters and sorts need the check inside, to stop inference by probing.
- **Affects:** STD-LGS-052, STD-OBJ-012, DSN-DVM-821, DSN-DVM-822, DSN-DVM-823, DEC-136

### DEC-138 — Protection, part 8c: encrypted fields and crypto-shredding (PH-2 step 8)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 8c of step 8 (DEC-136) covers encrypted fields under subject keys, equality lookups, crypto-shredding and key unavailability (STD-OBJ-010, -011, -012; DSN-DVM-830, -831, -833, -834). Their 20 vectors and VER-CONF-5364 of STD-LGS-052 (DEC-137) needed it; the 5 vectors of STD-OBJ-012 already passed.
- **Decision:**
  1. **Field codec** (`ubos-version`). `set_field_codec` installs a sealer and an opener, as `set_aligner` installs the aligner of merges.
     - `Repo::new_version` seals the body before the version is hashed.
     - `Repo::version`, `versions` and `version_in` open what they read.
     - `Repo::stored_version` reads a version as stored; archives read the store directly.
  2. **Stored form** (`crates/ubos-dvm/src/protection.rs`).
     - Protected fields are those of effective classification `personal` or `sensitive_personal` (the field's, else the class's `classification_default`), in a class whose linearization declares `subject_key`.
     - Each is stored as AES-256-GCM under the subject's current key: a random 12-byte nonce, the object and field as associated data, and the ciphertext of `canonical({salt, value})` with a random 16-byte salt. The commitment is `H(ubos.field-commit.v1 || salt || canonical(value))`.
     - The subject is the object (`self`), the text of the subject key field, or the object its reference names, resolved in the transaction. A missing subject value is `MODEL.CONSTRAINT_VIOLATED`.
     - `ubos-model` refuses a `subject_key` that is not `self` or an unprotected text or reference field of the class (`MODEL.INVALID_VALUE`).
  3. **Key service.**
     - Subject keys are kernel metadata `subject_key/<subject_id>` with their versions, outside the business data and archives.
     - A key is created on the first protected write; `subject_key {subject_id, rotate}` creates or rotates one, and earlier versions stay readable under their key version.
     - `key_availability` marks a key unavailable. Reads of its fields, and reads without a key on the node, fail with `IAM.KEY_UNAVAILABLE`.
  4. **Erasure.**
     - `erase_subject {subject_id, reason}` destroys the key versions and records the erasure. It reports the versions and fields encrypted under the key, found from every branch's objects through their timelines and predecessors.
     - Fields under a destroyed key read as `{erased, commitment}`, and the versions, hashes and proofs are unchanged.
     - A new value for the subject fails with `PROOF.VALUE_ERASED`. An update of other fields keeps the erased fields as their predecessor stored them, and its body check leaves the markers out.
  5. **Disclosure.**
     - `disclose {branch, object_id, field}` returns `{field, value, salt, commitment}`. A field that is not encrypted is `MODEL.INVALID_VALUE`; an erased one is `PROOF.VALUE_ERASED`.
     - The pure operation `verify_disclosure` checks a disclosure against a commitment.
     - `stored_form` returns the body as stored.
  6. **Claim.** The conformance test claims STD-OBJ-010, -011, -012 and STD-LGS-052.
  7. **Open, assigned when step 8 closes (8e):**
     - wrapping subject keys with the tenant key through the node's key service, and keeping them out of node backups (DSN-DVM-830, -832);
     - the check of legal holds and retention before an erasure (DSN-DVM-831);
     - stored lookup tokens, dropped on erasure (DSN-DVM-833).
- **Consequences:**
  - The 31 vectors of STD-OBJ-010…012 and STD-LGS-052 pass in memory and on PostgreSQL 16.
  - Opening costs one metadata read per encrypted field read; classes without `subject_key` are unaffected.
  - A unique key on a protected field compares stored forms, which differ for equal values; such keys wait for stored lookup tokens.
- **Alternatives considered:**
  - Decrypting in each reader (reads, queries, merges, rules, logic) was rejected: the codec at the version layer gives every reader the opened form and every writer the stored form, with archives and proofs on the stored form by construction.
- **Affects:** STD-OBJ-010, STD-OBJ-011, STD-OBJ-012, STD-LGS-052, DSN-DVM-830, DSN-DVM-831, DSN-DVM-832, DSN-DVM-833, DSN-DVM-834, DEC-137

### DEC-139 — Protection, part 8d: read logging and subject access export (PH-2 step 8)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 8d of step 8 (DEC-136) covers personal-data read logging (DSN-DVM-823; FR-AUD-051, NR-PRIV-005) and the subject access export (DSN-DVM-837; CR-GDPR-002, CR-CCPA-001). BPA 1.0-rc8 has no clause or vector for either, so the kernel builds them on the security log of PH-1 (DEC-073) and the subjects of part 8c (DEC-138).
- **Decision:**
  1. **Read log** (`crates/ubos-dvm/src/privacy.rs`).
     - A read or query made `as` a viewer whose output, after masking, shows personal or sensitive personal fields appends one `pii.read` record to the security log per operation. The record holds the principal, the operation, the branch and its evaluated commit, and the objects with the field names shown. It never holds values.
     - Objects whose subject is the viewer are not logged. The viewer is the subject when its principal is the subject ID, or is the value of the principal field (DEC-136) of the subject object.
     - Null and erased values are not personal data shown.
  2. **Subject export.** The adapter operation `subject_export {subject_id, branch?}` returns `{manifest, objects, entries}`.
     - `objects`: every object of the branch whose class links it to the subject through `subject_key` (DEC-138), with all its versions (timeline and predecessors), ordered by valid time.
     - `entries`: the entries of ledger classes linked the same way.
     - Fields encrypted under another subject's key are left out; erased fields appear as markers.
     - `manifest`: format `ubos.subject-export.v1`, the subject, branch, commit, time, the counts, and the content ID of the exported content.
  3. **Security log.** Exports (`subject.export`), key rotations (`key.rotate`) and erasures (`subject.erase`, with the reason) are recorded, as FR-AUD-012 asks for key operations and exports.
  4. **Open, assigned when step 8 closes (8e):**
     - read logging for reads without a viewer and for other outputs (outbox, diffs, explanations);
     - the approval of export requests and the NR-PRIV-002 deadline on the node;
     - exports on branches with a parent line.
- **Consequences:**
  - The ad hoc run on the data of VER-CONF-0380 behaved as specified. Another principal's read logged `email` and `tax_id`. The subject's own read and a masked read logged nothing. A query logged one record for its rows. The export gave the subject's person and agreement.
  - The security log grows with every logged read; batching per operation keeps a list query to one record.
- **Alternatives considered:**
  - A separate read-log store was rejected: the hash-chained security log already gives append-only, verifiable records (FR-AUD-014), and `security_log` reads them.
- **Affects:** DSN-DVM-823, DSN-DVM-837, FR-AUD-051, FR-AUD-012, NR-PRIV-005, NR-PRIV-002, DEC-073, DEC-136, DEC-138

### DEC-140 — PH-2 step 8 closed: protection (PH-2 step 8, part 8e)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** PH-2 step 8 (DEC-136) was done in parts 8a–8d, recorded in DEC-136…139. Part 8e closes the step: it records what the step delivered and lists, in one place, what the parts left open. The owner confirmed the part with the default suggestions.
- **Decision:**
  1. **Delivered.**
     - **8a (DEC-136).** Relation-based authorization: `authz_relation` and `principal_field` in the model, the sheet property `access`, `relations` and `check_access`, reads and queries `as` a viewer, port calls refused with `IAM.DENIED`.
     - **8b (DEC-137).** Field masking for viewers: reads, query rows and outbox payloads masked by `mask` (`hide`, `partial`, `hash` under the tenant lookup key) and by clearances; filters and sorts on masked fields refused.
     - **8c (DEC-138).** Encrypted fields under subject keys: a field codec in `ubos-version` that seals before hashing and opens on reads; key rotation, erasure with markers, key unavailability; `disclose`, `verify_disclosure` and `stored_form`.
     - **8d (DEC-139).** Personal-data read logging in the security log, the subject access export with a manifest, and security-log records of key rotations, erasures and exports.
  2. **Verification.**
     - The 48 current vectors of the step-8 clauses pass in one run in memory and on PostgreSQL 16: STD-LGS-050 and -051 (17), STD-LGS-052 (11), STD-OBJ-010 and -011 (15), and STD-OBJ-012 (5).
     - The kernel conformance test claims these clauses.
     - Read logging and the export, which have no vectors, were run on the data of VER-CONF-0380 (DEC-139).
  3. **Open.** Each item keeps its ID and moves to the target named:

     | Item | Source | Target |
     |---|---|---|
     | The group-expansion cache (DSN-DVM-813), a relation index maintained by the flush (DSN-DVM-810), and relation paths evaluated by index lookups (DSN-DVM-811) | DEC-136 | step 18, if the benchmarks need them |
     | Masking of diffs and explanations, and read logging of reads without a viewer and of other outputs (DSN-DVM-821, -823) | DEC-137, DEC-139 | step 14, where the Workspace shows history, change review and explanations to users |
     | Break-glass (DSN-DVM-808) | DEC-136 | step 12, with the identity boundary |
     | Subject keys wrapped by the tenant key through the node's key service and kept out of node backups (DSN-DVM-830, -832) | DEC-138 | step 12, with the KMS adapter |
     | Stored lookup tokens, dropped on erasure (DSN-DVM-833); personal fields of ledger entries, which are stored as they are | DEC-138, DEC-139 | step 10, if the finance Buk's vendor tax IDs or ledgers need them; otherwise step 19 |
     | The check of legal holds and retention before an erasure (DSN-DVM-831); the approval of export requests, the NR-PRIV-002 deadline and exports on branches with a parent line (DSN-DVM-837) | DEC-138, DEC-139 | step 19, with the privacy scenario SCN-016 |
     | Who-can and policy impact (DSN-DVM-840, -841) | DEC-136 | PH-3, with access review (CAP-IAM-08) |
  4. **Owner actions.** Merge the branch, then sign the tags `bpa-v1.0.0-rc8` to `bpa-v1.0.0-rc12`. Step 8 opened no release candidate.
  5. **Next.** Step 9 (the `finance` Buk, part 1: chart of accounts, journals, periods and close, general ledger and financial statements of the sample company, tied out in process; EXIT-2-04 by inspection). Its plan goes to the owner first.
- **Consequences:**
  - The status table in `docs/reports/ph2-plan-zh.md` marks step 8 as done.
  - The open items are picked up by the steps named in the table above, whose plans cite this decision.
- **Alternatives considered:**
  - Leaving the open items spread over DEC-136…139 was rejected, as in DEC-104, DEC-111, DEC-118, DEC-126 and DEC-135.
- **Affects:** DEC-136, DEC-137, DEC-138, DEC-139, DSN-DVM-808, DSN-DVM-810, DSN-DVM-811, DSN-DVM-813, DSN-DVM-821, DSN-DVM-823, DSN-DVM-830, DSN-DVM-831, DSN-DVM-832, DSN-DVM-833, DSN-DVM-837, DSN-DVM-840, DSN-DVM-841

### DEC-141 — The finance Buk, part 9a: chart of accounts, journal and opening balances (PH-2 step 9)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** PH-2 step 9 (UBS-PH-2 §11) builds part 1 of the `finance` Buk (DEC-022). Its scope is the chart of accounts, journals, periods and close, the general ledger and the financial statements of the sample company, tied out in process to `docs/finance-requirements/21-expected-results.md` (fin-req-v1.0, DEC-029). Its exit criterion is EXIT-2-04: no finance-specific kernel code (MET-BIZ-001), shown by inspection. The kernel features it needs were released in steps 1 and 3: the chart of accounts, double entry, periods and close artefacts, and projections (STD-KIND-020…029, STD-TIME-021). The owner confirmed the plan with the default suggestions.
- **Decision:**
  1. **Parts.**
     - 9a: the Buk and its model, and the opening trial balance (FIN-EXP-01).
     - 9b: journals and posting of the January documents (FIN-EXP-02, -03).
     - 9c: periods, close and the February items (the general-ledger part of FIN-EXP-15…17).
     - 9d: the financial statements (FIN-EXP-04…07).
     - 9e: the tie-out report, the EXIT-2-04 inspection record, and closing the step.

     Receivables, payables, bank, fixed assets, multi-currency, sales tax and 1099 (FIN-EXP-08…14) stay in step 10.
  2. **Place and licence.** First-party Buks live in `buks/` of the monorepo, licensed as `LICENSING.md` states (commercial, source available to customers). The finance Buk is `buks/finance`, namespace `ubos.finance`, version 0.1.0.
  3. **Model.**
     - `GlAccount` subclasses the genesis `LedgerAccount` (STD-KIND-029). It adds the chart's `category`, `normal_balance` and `statement_line`. The categories map to account types: Revenue to `income`, Tax to `expense`, and Other to `income` or `expense` by normal balance.
     - `FiscalPeriod` is the period class (STD-KIND-025).
     - `JournalLine` is one double-entry ledger with key `account`, amount `amount` (debits positive, credits negative), `ledger.account` and `period_scope`. The lines of a document share its posting group and carry `document`, `kind`, `party` and `description`.
     - The seed holds the `gl-balance` projection, the sum and count by account. A trial balance at a date is a read of it with `asof`.
  4. **Tie-out.**
     - The sample company is test data, not seed. `buks/finance/tools/gen_tests.py` generates `tests/sample-*.yaml` from the frozen CSVs and the tables of the expected results; the generated files are never edited by hand.
     - The tests load the company through adapter operations and compare results exactly. Lists match in length and order.
     - CI runs `gen_tests.py --check`, then `forge check` and `forge test` in memory and on PostgreSQL.
  5. **Forge.** A seed file whose classes need a change set on `main` (`VER.CHANGE_SET_REQUIRED`) is loaded on a loader branch and merged through an approved change set, as installation does (FR-PKG-031).
- **Consequences:**
  - FIN-EXP-01 (the 16 opening balances, 638,500.00 each side) ties out in memory and on PostgreSQL 16.
  - The commit clock of `forge test` starts at 2026-01-01, after the seed. Posting dates, which are commit dates (STD-KIND-025), therefore fall in 2026 or later; dates of business effect (`valid_at`) may be earlier.
- **Alternatives considered:**
  - Loading the sample company as seed was rejected: the seed is installed in every VAE that installs the Buk.
  - A separate ledger per document kind was rejected for step 9. One journal keeps the trial balance a single projection; step 10 adds subledgers where FIN-AR and FIN-AP need them.
- **Affects:** DEC-022, DEC-029, EXIT-2-04, MET-BIZ-001, STD-KIND-025, STD-KIND-029, STD-TIME-021, FR-PKG-031

### DEC-142 — The finance Buk, part 9b: posting the January documents (PH-2 step 9)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 9b of step 9 (DEC-141) posts the January 2026 documents of the sample company and ties out FIN-EXP-02 (the postings, line by line) and FIN-EXP-03 (the trial balance at 2026-01-31 before closing entries).
- **Decision:**
  1. **Posting.**
     - Each document of `transactions.csv` with a January posting date is one commit of `JournalLine` appends, sharing the document number as posting group, so the kernel checks the balance (STD-KIND-024).
     - Each line's `valid_at` is the posting date. The commit is made at noon of the recording date, so the posting date of STD-KIND-025 falls in the right open period.
     - Lines carry the document, its kind, the party and the description.
  2. **Projection.** The seed adds `gl-document`, the sum by document and account. It is the journal report and the drill-down from a balance to its documents (FIN-RP-006, -008), and the test reads it to compare FIN-EXP-02 line by line.
  3. **Tie-out.** `tools/gen_tests.py` takes the documents from the CSV and the expected figures from the tables of FIN-EXP-02 and -03, never from the CSV. A test therefore fails when the posting differs from the published results.
  4. **Not in 9b.** The approval of manual entries over 10,000.00 (FIN-GL-015), recurring entries (FIN-GL-017) and automatic reversal (FIN-GL-018) do not change the figures. Part 9e records how far step 9 covers each FIN-GL and FIN-PC requirement and where the rest goes.
- **Consequences:**
  - FIN-EXP-01, -02 and -03 tie out in memory and on PostgreSQL 16: the 28 January documents, and the trial balance of 826,012.90 each side.
- **Alternatives considered:**
  - Comparing per-account ledgers with `read_ledger` was rejected: one projection read compares every line of every document in one step.
- **Affects:** DEC-141, STD-KIND-024, STD-KIND-025, STD-TIME-021, FIN-GL-015, FIN-GL-017, FIN-GL-018, FIN-RP-006, FIN-RP-008

### DEC-143 — The finance Buk, part 9c: periods, close and the February items (PH-2 step 9)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 9c of step 9 (DEC-141) covers periods and close (FIN-PC-003, -005, -007) and the February items of the sample company. It ties out FIN-EXP-15 (the February postings), FIN-EXP-16 (the back-dated correction "as known on" two dates) and the general-ledger figures of FIN-EXP-17. The open balance of customer C400 belongs to receivables, in step 10.
- **Decision:**
  1. **Close.** January is closed on 2026-02-02 with the kernel's period close (STD-KIND-025, -026): an update to `closing`, then `close_period`. The close artefact's balances equal the trial balance FIN-EXP-03, and the commit gets the tag `close-2026-01-1`.
  2. **February.**
     - The February documents are committed in recording order at their recording dates.
     - BILL-OS-0120, a January item, posts on 2026-02-10 in the open February (FIN-PC-007).
     - JE-0005 is effective 2026-02-03 (`valid_at`) and recorded on 2026-02-20.
  3. **Refused postings.** A further test closes January on 2026-01-31 and then appends a January entry. The append fails with `LEDG.PERIOD_CLOSED`.
  4. **As known on.**
     - FIN-EXP-16 reads `gl-balance` at valid date 2026-02-05, as known at the end of 2026-02-05 and at the end of 2026-02-28.
     - The second reading uses the snapshot selector `tx_time` (convention 9 of STD-CONF-005). `read_projection` honoured only `time`. It now selects the snapshot with every selector, as the other reads do. This is a conformance gap closed in the kernel, not finance-specific code.
  5. **Single balances.** FIN-EXP-16 and -17 list only some accounts, so the tests read the projection's `group` of each account.
- **Consequences:**
  - FIN-EXP-01, -02, -03, -15, -16 and -17 (general-ledger figures) tie out in memory and on PostgreSQL 16: seven tests.
  - The 54 vectors of STD-TIME-021, STD-ISA-037, STD-KIND-025 and -026 still pass in memory and on PostgreSQL.
- **Alternatives considered:**
  - Reading "as known on" by commit sequence (`time`) was rejected: the expected results state dates, and `tx_time` is the selector made for them.
- **Affects:** DEC-141, STD-KIND-025, STD-KIND-026, STD-TIME-021, STD-CONF-005, FIN-PC-003, FIN-PC-005, FIN-PC-007, FIN-RP-020

### DEC-144 — The finance Buk, part 9d: financial statements (PH-2 step 9)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** Part 9d of step 9 (DEC-141) produces the four statements of January 2026: income statement, balance sheet, statement of cash flows by the indirect method, and statement of stockholders' equity. They must equal FIN-EXP-04…07 line by line and come from the Buk alone (EXIT-2-04).
- **Decision:**
  1. **Layouts are data.**
     - The class `FinancialStatement` holds a layout: `basis` (`lines` or `columns`), the line specifications and, for the equity statement, the columns. This is the report-layout requirement FIN-RP-011.
     - A line takes the balance at the period's end or start, or the change between them, of account codes or ranges, with a sign. Or it sums earlier lines.
     - A line can leave out document kinds, keep only documents that post to given accounts or leave them out, and take debits only. With these the cash-flow statement leaves out the revaluation from receivables and the equipment bought on account from payables, and reports interest and taxes paid.
     - A label may insert the amount of another line and the period's dates.
  2. **Seed.** The four US layouts of the standard chart (FIN-GL-002), with the labels of the expected results.
     - The retained earnings of the balance sheet and the equity statement include the year's result (accounts 4000–8999), since monthly closes post no closing entries.
     - The seed also adds the projection `gl-kind`, the sum by account and document kind.
  3. **Rendering.**
     - The query port `render(opening, closing)` is implemented by the L2 asset `ubos.finance.statements`. It reads `gl-balance`, `gl-kind` and `gl-document` at both dates and computes the period (opening, closing].
     - The script reads only projections and computes in exact decimals; it writes nothing.
     - Two L2 rules shape the script: the instruction gateway is visible only in the exported function, so it makes all reads; and the hardened profile has no `parse_int`, so account codes of one length compare as text.
  4. **Branch.** `forge test` commits logic assets on its working branch `feature/forge` (DEC-060). The tie-out tests therefore run on that branch, which inherits the seed.
  5. **Tests.** Four scenario tests compare every label and amount of FIN-EXP-04…07; amounts are signed, with two decimals. A port test renders the income statement on an empty ledger, every line at zero, which also counts the port toward script coverage (1.0).
- **Consequences:**
  - The 12 tie-out tests (FIN-EXP-01…07 and -15…17) pass in memory and on PostgreSQL 16.
  - No kernel code was added in this part.
- **Alternatives considered:**
  - Hard-coding the statements in the script was rejected: FIN-RP-011 asks for layouts, and a layout is reviewable and changeable through change sets.
  - Closing entries posted by each monthly close were rejected: the expected results keep income and expense balances through February.
- **Affects:** DEC-141, DEC-060, EXIT-2-04, FIN-RP-002, FIN-RP-003, FIN-RP-004, FIN-RP-005, FIN-RP-011, FIN-GL-002, STD-CTX-020, STD-ISA-037

### DEC-145 — PH-2 step 9 closed: the finance Buk, part 1 (PH-2 step 9, part 9e)
- **Date:** 2026-11-01 · **Status:** Accepted
- **Context:** PH-2 step 9 (DEC-141) was done in parts 9a–9d, recorded in DEC-141…144. Part 9e closes the step: it records the tie-out, the inspection for its exit criterion EXIT-2-04, and how far the step covers the requirements of the general ledger, periods and reporting. The owner confirmed the part with the default suggestions.
- **Decision:**
  1. **Delivered.** The `finance` Buk 0.1.0 in `buks/finance` holds:
     - the chart of accounts (`GlAccount`) and fiscal periods;
     - the double-entry journal `JournalLine`;
     - the projections `gl-balance`, `gl-document` and `gl-kind`;
     - the statement layouts `FinancialStatement` with the L2 port `render`;
     - the four US layouts in its seed;
     - the tie-out tests, generated from the frozen sample company.
  2. **Tie-out.** `buks/finance/docs/tie-out.md` is generated by `tools/tieout_report.py`. 12 tests compare 466 figures of FIN-EXP-01…07 and FIN-EXP-15…17 (general ledger), and all pass in memory and on PostgreSQL 16.
  3. **EXIT-2-04 passed by inspection** (VER-INSP-0002).
     - Procedure 1: `docs/tools/check_kernel_purity.py` scans the kernel crates and classifies every finance identifier by a rule naming its generic meaning. It found 172 hits, all classified, 0 findings; CI runs it.
     - Procedure 2: the kernel changes of step 9 are the `read_projection` selectors (a conformance gap, DEC-143) and the Forge seed path (a tool, DEC-141). Both are generic and standardised.
     - The record is `buks/finance/docs/step-9-record.md`.
  4. **Requirement coverage.** The record lists FIN-GL-001…022, FIN-PC-001…009 and FIN-RP-001…021: 5 covered, 17 partial and 20 later. Each partial or later item names its target:
     - step 10: entry lifecycle and approval, recurring and reversing entries, subledger posting and control accounts, dimensions, numbering per year and source, fiscal years and year-end close, governed reopening, document dates, the standard chart template, "as known on" statements;
     - step 13: attachments;
     - step 14: account inquiry;
     - step 15: the close checklist and overview, report runs and exports, comparatives, drill-down, notes and the dashboard;
     - step 16: the layout editor;
     - step 17: journal import.

     The finance scenarios SCN-501…510 verify the whole set in step 18.
  5. **Owner actions.** Merge the branch, then sign the tags `bpa-v1.0.0-rc8` to `bpa-v1.0.0-rc12`. Step 9 opened no release candidate.
  6. **Next.** Step 10 (the `finance` Buk, part 2: receivables, payables with 1099, bank reconciliation, fixed assets, multi-currency, sales tax; SCN-501…510 in process; the accounting reviewer's review; EXIT-2-16 in process). Its plan goes to the owner first.
- **Consequences:**
  - The status table in `docs/reports/ph2-plan-zh.md` marks step 9 as done.
  - Step 10 starts from the coverage record and its targets.
- **Alternatives considered:**
  - Recording the requirement coverage in `docs/finance-requirements` was rejected: that folder is frozen and never refers to an implementation (DEC-029). The record lives with the Buk.
- **Affects:** DEC-141, DEC-142, DEC-143, DEC-144, EXIT-2-04, VER-INSP-0002, MET-BIZ-001, CST-005
