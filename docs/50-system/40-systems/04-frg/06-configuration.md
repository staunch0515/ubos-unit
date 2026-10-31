---
id: UBS-SYS-FRG-06
title: Logic Forge — Configuration
status: draft
phase: PH-1
depends_on: [UBS-SYS-FRG-02]
---

# Logic Forge — Configuration (`forge.toml`)

| Key | Default | Design |
|---|---|---|
| `project.name`, `project.version` | — (version `0.1.0`) | DSN-FRG-301 |
| `project.namespace` | `project.vae` | DSN-FRG-301 |
| `project.abi` | `1.0` | DSN-FRG-301 |
| `project.description`, `project.licence` | empty | DSN-FRG-301 |
| `project.capabilities` | the union of the assets' capabilities | DSN-FRG-301, DSN-FRG-305 |
| `dependencies."<namespace>/<name>"` | — (a version range) | DSN-FRG-303 |
| `gates.script_coverage` | 0.80 | DSN-FRG-104 |
| `gates.rule_row_coverage` | 1.00 | DSN-FRG-104 |
| `gates.fail_on_warning` | false | DSN-FRG-101 |
| `types.languages` | ["ts", "rust"] | DSN-FRG-102 |
| `dev.vae_seed` | `seed/` | DSN-FRG-501 |
| `simulate.budget.processes` | 100000 | DSN-FRG-205 |
| `sign.key` | key reference or `sigstore` | DSN-FRG-304 |
| `publish.exchange` | public EXC URL | DSN-FRG-308 |

The manifest table is `[project]`, as `forge init` writes it since PH-1; the archive manifest built from it is described in UBS-SYS-FRG-09 (DEC-129). The lock of the resolved dependencies is `forge.lock`, written by `forge pack`.
