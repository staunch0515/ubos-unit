# ubos-unit

Design specification of the **UBOS Business Processing Architecture (BPA)** and the
implementation-neutral **US Basic Finance requirements**. This repository is also the single
implementation monorepo (DEC-028).

| Folder | Content |
|---|---|
| `docs/` | specification, finance requirements, tools, reports |
| `standard/` | BPA standard releases (`bpa-standard`) |
| `verify/` | formal models, environment definitions, verifier (`ubos-verify`) |
| `crates/` | Rust crates of kernel, node and Forge (`ubos`); workspace in `Cargo.toml` |
| `packages/`, `buks/` | TypeScript packages and first-party Buks (created when needed) |

Licences per folder: [`LICENSING.md`](LICENSING.md). CI: `.github/workflows/ci.yml`.

| Start here | Path |
|---|---|
| working agreement for AI sessions | [`CLAUDE.md`](CLAUDE.md) |
| current status, decisions and next steps | [`docs/HANDOFF.md`](docs/HANDOFF.md) |
| system specification (authoritative) | [`docs/50-system/README.md`](docs/50-system/README.md) |
| US Basic Finance requirements (version 1.0, frozen, tag `fin-req-v1.0`) | [`docs/finance-requirements/README.md`](docs/finance-requirements/README.md) |
| reports to the owner (Chinese) | [`docs/reports/`](docs/reports/) |

Validate the documents with `bash docs/tools/regen_all.sh` (must report 0 errors) and the
code with `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D
warnings` and `python3 docs/tools/check_deps.py`.

This repository was created from `staunch0515/ubos-platform` with the full history of the
`docs/` folder (DEC-027). `ubos-platform` is kept as an archive.
