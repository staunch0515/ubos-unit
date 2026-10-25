# CLAUDE.md — working agreement for this repository

Read this file and `docs/HANDOFF.md` before doing anything else.

## What this repository is

The design specification of **UBOS — Business Processing Architecture (BPA)** and the
implementation-neutral **US Basic Finance requirements** that will be implemented on UBOS
and, for comparison, on the owner's separate Java system.

| Path | Content |
|---|---|
| `docs/50-system/` | the authoritative system specification (requirements, architecture, BPA standard, 13 systems, phases PH-0…PH-5, verification, decisions DEC-001…110) — start at `docs/50-system/README.md` and `docs/50-system/00-meta/05-ai-reading-guide.md` |
| `docs/finance-requirements/` | implementation-neutral US Basic Finance requirements (FIN-*), sample company dataset, expected results, acceptance scenarios, comparison protocol; self-contained with its own `tools/`, frozen at version 1.0 (`fin-req-v1.0`, DEC-029) |
| `docs/tools/` | generators and checkers of the system volume (`regen_all.sh` runs all, including the finance folder's `tools/regen.sh`) |
| `docs/reports/` | reports to the owner, in Chinese |
| `docs/10-inventory` … `docs/40-spec`, `docs/00-meta` | earlier research volume, kept as history and input (DEC-008); do not extend |
| `docs/HANDOFF.md` | current status, decisions, open items and next steps |
| `standard/`, `verify/`, `crates/` | implementation monorepo (DEC-028): BPA standard releases, formal models and verifier, Rust crates; licences in `LICENSING.md` |

## Rules the owner has set (follow them without being reminded)

1. **Language.** All specification documents are written in **English**. Every report,
   summary, plan or question addressed to the owner is written in **Chinese**.
2. **Plan first.** For any non-trivial change, first state your understanding and a plan in
   Chinese and wait for the owner's confirmation. When the owner says "按默认建议" or
   confirms, proceed without asking again.
3. **Advisory questions.** When the owner asks for an opinion or assessment ("不开发,不设计,
   不测试"), answer in text only: no development, no design documents, no tests.
4. **No testing and no security review** unless explicitly asked: do not write tests, do not
   hunt for bugs, vulnerabilities or security issues in code ("不要测试,不要找代码的漏洞和安全问题").
   Exception: the verification work that a phase's exit criteria require is requested when
   the owner confirms that phase's plan (PH-0: DEC-030).
5. **IDs are permanent.** Never renumber or reuse an ID. Deferred items keep their ID and are
   marked `**Status:** Deferred (DEC-xxx)`; withdrawn items are marked `**Withdrawn**` with a
   reason. A change of phase or scope needs a `DEC-*` record and a `**Phase note:**` line.
6. **Generated files are never edited by hand** (`INDEX/`, `50-phases/PH-*/01-features.md`,
   `60-verification/2*-trace*.md`, `finance-requirements/21-expected-results.md`,
   `finance-requirements/sample-company/*.csv`, `finance-requirements/90-traceability.md`,
   `standard/spec/`, `standard/vectors/`, `standard/release/*/coverage.md` and `manifest.json`;
   vectors are written as cases in `standard/tools/cases/`).
   Edit the sources, then regenerate.
7. **Validate before every commit:** `bash docs/tools/regen_all.sh` must end with
   `errors: 0` for both the system volume and the finance requirements, with 0 warnings and
   0 Must gaps in the traceability summary. When code changes, also run `cargo fmt --all
   --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and
   `python3 docs/tools/check_deps.py` (0 errors); CI runs all of these.
8. **Commits.** Clear messages; push to the branch you were asked to use. Do not open pull
   requests unless asked.
9. **Finance requirements are frozen.** Never change a released version of
   `docs/finance-requirements/`. A change needs the owner's confirmation, an entry in its
   `99-change-log.md`, a new version and a new tag `fin-req-vX.Y` (DEC-029). The folder must
   never refer to UBOS, the Java system or any path outside itself.
10. **Excluded topics** (charter §3.2, CST-008): tokens, cryptocurrencies, compute markets,
   "intent economy", organisational-governance protocols and "end of the company"
   narratives never become features.

## How the owner works

The owner designs the system with AI assistance and decides; you draft, check consistency
and keep the documents machine-checkable. Keep answers concrete, cite IDs, and give a
recommendation rather than a list of options.
