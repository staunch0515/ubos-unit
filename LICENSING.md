# Licensing

Licences per folder follow DEC-019 and DEC-028. Each folder carries its own licence file.
A legal review is required before any public release (DEC-019); until then this repository
is private.

| Folder | Component | Licence | File |
|---|---|---|---|
| `standard/` | `bpa-standard` | text of the standard: CC BY 4.0; schemas, conformance vectors and the build tools and reference oracle in `standard/tools/`: Apache-2.0 | `standard/LICENSE`, `standard/LICENSE-APACHE` |
| `verify/` | `ubos-verify` | Apache-2.0 | `verify/LICENSE` |
| `crates/` | `ubos` (kernel, node, Forge) | AGPL-3.0-only as the open edition; a commercial licence is available for embedding and hosted use without AGPL obligations | `crates/LICENSE` |
| `packages/` (not yet created) | `ubos` (SDKs and clients) | SDKs Apache-2.0; other packages per DEC-019 | — |
| `buks/` (not yet created) | `ubos-buks` | commercial (source available to customers) | — |
| `docs/` | specification, finance requirements, tools, reports | internal | — |

A crate outside the kernel, node and Forge (for example an SDK crate) states its own licence
in its `Cargo.toml` and overrides the workspace default.
