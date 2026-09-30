# BPA standard (`bpa-standard`)

The public standard of the UBOS Business Processing Architecture (DEC-028: component
`bpa-standard`, folder `standard/`). The normative source text is drafted in
`docs/50-system/30-standard/` and is released from here (DSN-BPA-001).

| Folder | Content (UBS-SYS-BPA-04) |
|---|---|
| `spec/` | clauses, JSON Schemas, ABNF grammars |
| `vectors/<profile>/` | conformance vectors `VER-CONF-nnnn.json` |
| `proposals/` | change proposals `NNNN-title.md` with status |
| `errata/` | errata |
| `registry/` | extension and algorithm registries |

Releases are tagged `bpa-vX.Y.Z`, signed by the owner, and published with a manifest of file
hashes (DSN-BPA-001, DEC-030). BPA 0.1 is PH-0 step 3 (EXIT-0-01) and is frozen as tagged
(`bpa-v0.1.0`). BPA 1.0-rc1 (PH-1 step 1, EXIT-1-01, DEC-039) is released as `bpa-v1.0.0-rc1`
(DEC-045) and frozen once tagged; `spec/` and `vectors/` hold it until a decision opens
BPA 1.0-rc2. Its scope follows a rule stated in its `scope.json`: the
scope of 0.1 plus the Core, Bitemporal, Ledger and Proof clauses that serve PH-1 gate 1a; the
builder rejects a scope that differs from the rule. Vectors are only added; the 0.1 vectors
keep their identifiers and content.

## Building a release (DEC-032)

The normative text lives in `docs/50-system/30-standard/`; everything under `spec/`, `vectors/`
and `release/<version>/` except `scope.json` is generated — never edit it by hand.

```sh
python3 standard/tools/genvectors.py      # vectors from the cases, expected results from the oracle
python3 standard/tools/build_release.py   # release in development (1.0-rc1): clauses, schemas, checks, coverage, manifest
```

Both run from `bash docs/tools/regen_all.sh`. Python 3.9 or later, standard library only.

| Path | Content |
|---|---|
| `release/<version>/scope.json` | clauses and profiles of the release (edited by hand; for 1.0-rc1 with its scope rule) |
| `release/<version>/notes.md` | release notes: content, changes, known limits (edited by hand, listed in the manifest) |
| `release/<version>/coverage.md` | MUST-clause coverage by vectors (MET-QUAL-003) |
| `release/<version>/manifest.json` | SHA-256 of every released file and the manifest content identifier |
| `tools/bpa/` | reference oracle: canonical JSON, values, classes, store, tree, adapter operations (STD-CONF-005) |
| `tools/cases/` | vector cases per area |
| `tools/genvectors.py`, `tools/build_release.py` | generator and release builder |

An adapter runs a vector by executing `setup` and `steps` in order and comparing each result
with `expect` (STD-CONF-001, STD-CONF-005). Vectors tagged `prototype` are required of the
PH-0 prototype (EXIT-0-03).

Nothing here depends on the implementation (DEC-028). Licences: text CC BY 4.0
(`LICENSE`); schemas and vectors Apache-2.0 (`LICENSE-APACHE`).
