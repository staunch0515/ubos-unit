---
id: UBS-SYS-FRG-09
title: Logic Forge — Buk Format
status: draft
phase: PH-2
depends_on: [UBS-SYS-FRG-02, UBS-SYS-FRG-06]
---

# Logic Forge — Buk Format (`ubos.buk.v1`)

The Buk is the package format of the UBOS product. It is not part of BPA 1.0 (DEC-127): a
second implementation of the standard need not read it. This page describes version 1 of
the format as PH-2 step 7c builds it (DEC-129). The designs it serves are DSN-FRG-301…305;
installation (DSN-FRG-306, DSN-DVM-134, DSN-NOD-422; §7, part 7d) and shipped migrations
(DSN-FRG-307, part 7e) use it. The shared code is `crates/ubos-dvm/src/buk.rs`, and the
Forge commands are in `crates/ubos-forge/src/package.rs`.

## 1. Archive

An archive is one JSON document in canonical form (RFC 8785), stored as
`<namespace>.<name>-<version>.buk`:

| Member | Content |
|---|---|
| `format` | `"ubos.buk.v1"` |
| `manifest` | the manifest (§2) |
| `tree` | path → chunk hash, for every packed file |
| `chunks` | chunk hash → content |
| `root` | the root hash (§3) |
| `signatures` | detached signatures over the root (§4); the trailer of the archive |

**Files.** `forge pack` packs the project folders `model/`, `sheets/`, `logic/`,
`components/`, `views/`, `seed/`, `tests/`, `docs/` and `migrations/`.
- A path is relative and `/`-separated, without `.` or `..` segments.
- A YAML or JSON file is packed as its parsed content, so formatting and comments do not
  change the hash.
- Any other file is packed as UTF-8 text. A binary file is refused.

**Chunks.** A chunk is the content of one file. Its hash is
`content_id("ubos.chunk.v1", content)`, so equal files share one chunk.

**Integrity.** A reader refuses an archive with `PKG.MANIFEST_INVALID` when:
- the format is unknown, or the manifest is malformed;
- a chunk does not match its hash, or a chunk belongs to no path;
- a path names a missing chunk;
- the stated root differs from the computed one.

## 2. Manifest

| Member | Content |
|---|---|
| `name` | lowercase letters, digits, `-` and `_` |
| `namespace` | dot-separated lowercase segments; the Buk owns this namespace and the namespaces below it |
| `version` | semantic version `major.minor.patch[-pre]` |
| `abi` | ABI `major.minor`; the major must be the one the platform runs (1 for BPA 1.0) |
| `dependencies` | Buk ID `<namespace>/<name>` → version range (§5) |
| `locked` | Buk ID → `{version, root, requires}`, the resolution packed with the Buk (§5) |
| `capabilities` | the capabilities requested: instruction families, for example `read` or `commit` |
| `contents` | counts of classes, value types, assets, seed changes and tests |
| `gates` | `script_coverage`, the coverage gate verification applies, as a decimal string |
| `dev` | `vae` and `branch` of the development fixture the tests run against |
| `deprecation` | optional: `message`, `end_of_support` (a date), `replacement` (a Buk ID) — this version is deprecated (FR-PKG-052, §9) |
| `description`, `licence` | optional |

The Buk ID is `<namespace>/<name>`. The manifest is built from `forge.toml` (UBS-SYS-FRG-06).

## 3. Root

The root is `content_id("ubos.chunk.v1", {"buk": {"manifest": manifest, "tree": tree}})`.
- It binds the manifest and every path to its content, so the same sources packed on two
  machines give the same root (DSN-FRG-301).
- Signatures are not covered: adding a signature does not change the root.
- The genesis Buks of PH-1 (DEC-077) use the same `buk` wrapper over their manifest; they
  move to this format with the genesis upgrades of part 7e.

## 4. Signatures

A signature is a DAT-Signature (STD-PROOF-011) with role `signer` over the UTF-8 bytes of
the root.
- The signer is the key-form DID `did:ubos:k:…` of the publisher's Ed25519 key, so
  `forge verify` checks it offline and shows the publisher's identity.
- `forge sign` adds a signature, or replaces one of the same signer and key.
- Trust in a publisher is a list of DIDs given to `forge verify` (`--trust`); without
  one, every valid signature is accepted and its signer reported.
- Keyless signing and publisher keys registered with the exchange come with PH-4
  (DEC-127).

## 5. Versions, ranges and `forge.lock`

**Ranges.** A version range is one of:
- `*`;
- `1.2.3` or `^1.2.3`: compatible, below the next major, or the next minor for `0.x`;
- `~1.2.3`: below the next minor;
- `=1.2.3`;
- comparators `>=`, `>`, `<=`, `<`, joined by spaces or commas;
- alternatives joined by `||`.

Partial versions (`1`, `1.2`) are allowed in the caret, tilde and bare forms. A
pre-release satisfies a range only when the range names a pre-release of the same version.

**Resolution.** `forge pack` resolves the dependencies against a local repository: a
directory of `.buk` files, `buks/` in the project or `--repo`.
- The solver is deterministic and backtracking. It takes the highest version that
  satisfies every range, and it tries the version of the previous lock first.
- A version with another ABI major is skipped.
- When no version fits, the error names every path to the Buk that cannot be satisfied,
  for example `app → base ^1.0.0` and `app → mid 0.1.0 → base ^0.1.0` (FR-PKG-023).

**Lock.** The result is written to `forge.lock` (TOML; one `[[buk]]` table per Buk with
`id`, `version`, `root` and `requires`) and packed as the manifest member `locked`.
Retrieval from the exchange comes with PH-4.

## 6. Verification record

`forge verify` produces a record with these members:
- `buk`, `version`, `root`, `signers`;
- `checks`: one entry per check below, each with its `result` and `findings`;
- `result` and `verifier`.

The checks are:
- `signatures`;
- `manifest`;
- `abi` (the Buk and its assets);
- `namespace`: classes and asset names inside the Buk's namespace;
- `dependencies`: every range locked to a matching version, and the locked archives
  present with their roots when a repository is given;
- `checks`: `forge check`;
- `tests`: `forge test` on the development fixture;
- `coverage`: the gate, per asset in permille;
- `capabilities`: the capabilities used by the assets, against those requested;
- `migrations`: against a previous version given with `--previous`. A lens-required class
  change needs a seeded `Lens` or a migration, and a breaking one needs a migration (§8).
  The previous version's fixture is then upgraded to this version as an installation
  would upgrade it, and this version's tests run on the migrated data (DSN-FRG-307).

The record carries no time, so the same inputs give the same record (FR-PKG-063). With
`--key` it is signed by the verifier: its content ID becomes `record_id`, and its
signature is stored in `signature`.

## 7. Installation

Installation is built in PH-2 step 7d (DEC-130). The kernel part is in
`crates/ubos-dvm/src/packages.rs`, the Server routes are under `/api/v1/admin/buks`, and the
client is `forge install`.

**Archive checks.** A Buk is refused before anything is written when:
- its integrity fails (§1);
- it has no valid signature, or no signature of a publisher in
  `buks.trusted_publishers` (`PKG.SIGNATURE_INVALID`).

The Server takes the archive in the request body, or as the name of a `.buk` file in its
`buks.dir`.

**Plan** (`buk_plan`, FR-PKG-033). The plan is computed against the target branch and
writes nothing. It lists:
- the classes and value types added or unchanged;
- the logic assets added or unchanged;
- the seed changes by class, with the subscriptions, schedules and connectors among them;
- the sheet, view and component files;
- the configuration the manifest requires (member `configuration`);
- the permissions requested (the capabilities);
- the dependencies, with the installed versions.

These problems block the installation:
- a class or asset outside the Buk's namespace;
- a class of the same name in another namespace;
- a namespace held by a Buk of another publisher (`PKG.NAMESPACE_VIOLATION`);
- a dependency that is not installed in a matching version (`PKG.CONFIGURATION_INCOMPLETE`);
- another version installed, or a class or asset that exists with another content
  (`PKG.MANIFEST_INVALID`; upgrades come with part 7e).

**Install** (`buk_install`, FR-PKG-031):
1. A loader branch `loader/<namespace>/<name>-<version>` is created from the target, with
   dots written as dashes and a numeric suffix when an earlier attempt left one.
2. The new classes and value types are declared. Declarations are shared by the branches
   of a VAE, so a retry finds them unchanged.
3. The assets and the seed are committed on the loader branch, and each asset is verified
   there by the node (`verify_asset`, STD-CTX-023), so that it runs on protected branches
   after the merge.
4. A change set from the loader branch into the target is opened and submitted; its
   preview validates the merge.

The registry records the Buk as `pending`. Running the Buk's scenarios in a sandbox copy
is the `forge verify` that `forge install` runs before it sends the Buk.

**Completion** (`buk_complete`). The administrator approves the change set, which is then
merged. The Buk is recorded on the target as a System-kind `Buk` object (name = Buk ID,
version, `content_hash` = root, publisher, signature). Its registry record goes in the same
commit: namespace, dependencies, capabilities, target, loader branch, change set, merged
commit and state. Installing the same root again reports `present`.

**Registry and audit.** `GET /api/v1/admin/buks` lists the registry. Installing and
completing are audited as `admin.buk_install` and `admin.buk_complete`. The registry
answers which Buks depend on a Buk; uninstalls and downgrades are refused on that answer
(§8, §9).

## 8. Upgrades, migrations and rollback

Upgrades are built in PH-2 step 7e-1 (DEC-131) in `crates/ubos-dvm/src/upgrades.rs`.

**Upgrade.** Installing a later version of an installed Buk is an upgrade. It uses the same
plan, loader branch and change set as an installation; a downgrade is refused (it is a
rollback), and so is a version that an installed dependent's range excludes. The plan
adds:
- `from`: the installed version and root;
- per changed class: the classification and the means, one of `none` (additive), `lens`
  (a `Lens` the new version seeds from the current schema version) or `migration`. A
  class goes up one schema version per upgrade;
- the assets changed, and those no longer shipped, which are kept;
- the seed objects to create, those to update (still at the previous version's seed body)
  and those kept because they were changed locally;
- per overlay of the target (a branch whose parent line is the target): the overridden
  objects the upgrade changes.

Staging applies the additive and lensed class changes (declarations are shared by the
branches of a VAE, and existing data reads through the lens), commits the lenses, the
changed assets (verified again) and the seed on the loader branch, and submits the change
set. Completion merges it, then applies the class changes by migration, rewrites their
instances, commits the seed of those classes, and records the new version with the
previous one kept. It reports each overlay's upstream changes (STD-VER-082), which the
overlay resolves by acknowledgement or a three-way merge. A completion that stops part way
is resumed by calling it again.

**Migrations.** A migration is a file under `migrations/` holding a one-way lens:

```yaml
class: Customer
from: 2
to: 3
one_way: true
one_way_reason: customers without a tier become standard customers
ops:
  - {op: default, field: tier, value: standard}
```

`change_class` with a `migration` accepts a breaking change when the migration takes every
stored body forward to a valid body (DSN-DVM-123). The migration is recorded as the step's
lens. `migrate_instances` then rewrites the instances on the target and on each overlay
(only the overlay's own objects) by a bulk process, in commits of 1,000 objects.

**Rollback.** `buk_rollback` (`POST /api/v1/admin/buks/rollback`, `forge install
--rollback <ID>`) reverts the merge of the Buk's last upgrade (STD-VER-071): objects the
upgrade created are ended, and changed objects and assets are restored. The previous
registry record and `Buk` object come back. A conflict with later changes is returned for
the administrator's resolutions; one on the change set's own state keeps its current
state. Class changes and rewritten instances stay: data written under the new schema reads
through the lenses (FR-PKG-044). One upgrade is kept for a rollback.

## 9. Removal and deprecation

Built in PH-2 step 7e-2 (DEC-132) in `crates/ubos-dvm/src/removal.rs`.

**Uninstall** (`POST /api/v1/admin/buks/uninstall`, `forge install --uninstall <ID>
[--plan] [--approve]`). An uninstall is refused, with the reason in the plan:
- while installed Buks depend on the Buk, naming them (FR-PKG-053);
- for the genesis Buks;
- for a Buk whose archive is not recorded.

The plan lists what goes and what stays:
- the logic assets to retire; an asset that another installed Buk's class implements a
  port by is kept;
- the configuration objects to end: objects the seed created of classes that are not the
  Buk's own, such as subscriptions, schedules, sheets and views;
- the Buk's classes with their object counts, which stay read-only (`retention:
  read_only`).

The retirements and ends are staged on a loader branch (`…-uninstall`) and submitted as a
change set. Completion (`buk_complete`) merges it, marks the classes read-only, and
records the Buk as `uninstalled`, in one commit. The data of a read-only class stays
readable and exportable. A commit that writes it is refused with
`MODEL.CONSTRAINT_VIOLATED` (reason `read_only`), except the platform's own commits, until
retention allows its disposal (FR-PKG-051).

**Reinstall.** Installing an uninstalled Buk again takes the upgrade path (§8):
- its retired assets are restored and verified;
- its classes are writable again;
- objects that were ended stay ended and are reported in the seed plan (`ended`).

**Deprecation** (FR-PKG-052).
- A version deprecates itself in its manifest (`[deprecation]` in `forge.toml`).
- A publisher deprecates installed versions later with a notice: `forge deprecate <ID>
  --versions <range> --message … [--end-of-support <date>] [--replacement <ID>] --key …`
  writes it, and `forge install --notice <file>` delivers it.
- A notice is `{buk, versions, message, end_of_support?, replacement?}` with a
  DAT-Signature over its content ID. It is accepted only from the Buk's publisher, and it
  applies when the installed version is in `versions`.
- Install plans warn about a deprecated version.
- The registry shows each Buk as `supported`, `deprecated` or `end_of_support` (the date
  has passed).

## 10. Genesis Buks

The genesis Buks `system` and `ontology` (DEC-077) are also `ubos.buk.v1` archives (Buk IDs
`ubos.platform/system` and `ubos.ontology/ontology`, the namespace of their classes): their classes are under
`model/classes.json`, and they are signed with the genesis publisher key.

A new VAE gets the node's genesis Buks at bootstrap. When a node process first opens a VAE
that holds an older version:
- it registers the installed version;
- it stages the newer one as an upgrade (§8) with a submitted change set
  (DSN-NOD-421, FR-PKG-073);
- an administrator completes it (`forge install --complete ubos.platform/system`);
- `main` does not change until then.

A VAE holding a newer version than the node's is refused. A proposal that cannot be made, for
example because the VAE declares a class of the same name in its own namespace, leaves the
VAE on its version; the node reports it as `upgrade_blocked` with the plan's problems. `system` 0.2.0 adds the registry
members to the `Buk` class (schema version 2), so a VAE that has completed the upgrade
records its Buks with them.

