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
hashes (DSN-BPA-001, DEC-030). BPA 0.1 is PH-0 step 3 (EXIT-0-01).

Nothing here depends on the implementation (DEC-028). Licences: text CC BY 4.0
(`LICENSE`); schemas and vectors Apache-2.0 (`LICENSE-APACHE`).
