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
