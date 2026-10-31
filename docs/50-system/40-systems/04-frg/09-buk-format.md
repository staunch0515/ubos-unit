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
installation (DSN-FRG-306, DSN-DVM-134, DSN-NOD-422) and shipped migrations (DSN-FRG-307)
use it from parts 7d and 7e. The shared code is `crates/ubos-dvm/src/buk.rs`, and the
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
  change needs a seeded `Lens` or a migration, and a breaking one needs a migration. A
  migration is a file under `migrations/` that names its `class`; its execution comes
  with part 7e.

The record carries no time, so the same inputs give the same record (FR-PKG-063). With
`--key` it is signed by the verifier: its content ID becomes `record_id`, and its
signature is stored in `signature`.
