# BPA 1.0.0-rc9 — Release notes

Release candidate 9 of BPA 1.0 (PH-2 step 3a, DEC-098), the standard for money, accounts,
statements and projections of the PH-2 kernel. Tag `bpa-v1.0.0-rc9`, signed by the owner
(DSN-BPA-001). The manifest (`release/1.0-rc9/manifest.json`) lists every released file with its
SHA-256; its `content_id` identifies this release. BPA 1.0-rc8 stays frozen as tagged. The
standard precedes the code (AR-033): PH-2 step 3 implements this frozen text in parts 3b–3g.

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 245 (scope by phase, `scope.json`): the 238 clauses of BPA 1.0-rc8 and 7 written in step 3a |
| MUST clauses with a vector range | 207, 207 covered (`coverage.md`) |
| Vectors | 2,125: those of BPA 1.0-rc8 without VER-CONF-3003 (superseded), and 73 of step 3a |

## Changes since BPA 1.0-rc8

- **Currency reference data** (STD-FND-028, new): the ISO 4217 codes of list one with their minor units, as current on 2026-01-01; a VAE adds a code with a `Currency` object.
- **FX rates and conversion** (STD-FND-029, new): `FxRate` objects keyed by `(source, base, quote, rate_type)` with the rate of a date as the version valid at that date; the L1 function `fx_convert(m, quote, rate_type, source?)` with the inverse rate and half-even rounding; `convert` against a branch reads an `FxRate` object; the journal records the rate versions used (`rates`, STD-CTX-011).
- **Chart of accounts** (STD-KIND-029, new): `LedgerAccount` objects with type, currency and parent; a ledger class names its `account` field, and postings go to accounts that are live, leaves and in the account's currency at the entry's valid time.
- **Statement lines** (STD-KIND-030, new): entry ids derived from source, account, statement and line; idempotent ingestion with duplicates and `TXN.IDEMPOTENCY_MISMATCH` for a changed line.
- **Projections** (STD-TIME-021, new): `Projection` objects folding the entries of a ledger class by group with `count`, `sum`, `min`, `max` and `last`; `sync` and `async` maintenance with a watermark, `at_least` and `QRY.WATERMARK_TIMEOUT`; rebuilds equal to the fold at the head.
- **`read.projection`** (STD-ISA-037, new): reads a projection at the process snapshot, with `group`, `asof` and `rollup` over the account hierarchy. VER-CONF-3003 (the catalogue of 1.0-rc8) is superseded by VER-CONF-3004.
- **Compensation proposals** (STD-TIME-043, new): the appends of a re-run process are not committed on the correction branch; a restatement with `proposals` reports the adjustments per ledger key and currency, dated by `date_policy`.
- **Error registry:** `QRY.WATERMARK_TIMEOUT` (retryable).
- **Class declarations:** the `ledger` member takes `account` and `statement`.
- **Genesis classes:** `Currency`, `FxRate`, `LedgerAccount` and `Projection`.
- **Adapter operations** `currency`, `fx_rate`, `ingest_statement`, `read_projection`, `projection_lag` and `rebuild_projection`; `restate` takes `proposals` and `eval_expr` takes `fx_rates`.

## Superseded vectors

One vector of BPA 1.0-rc8 is kept as released and excluded from this release
(`vectors/superseded.txt`, STD-CONF-030):

- VER-CONF-3003 asserted the instruction catalogue of 1.0-rc8; `read.projection` joins the catalogue (VER-CONF-3004, DEC-098).

## Known limits

- **Kernel.** The vectors added in 1.0-rc8 and 1.0-rc9 are not tagged `prototype`. The kernel runs the money and period vectors of both releases as PH-2 step 3 implements them (parts 3b–3g, EXIT-2-01, VER-CONF-8405).
- **Simulated outside world.** An `async` projection of the reference model is current unless `projection_lag` stops its maintenance; a read with `at_least` above the watermark fails at once instead of waiting.
- **Not in this release:**
  - projections over Definition versions (DSN-DVM-422);
  - money validation by the valid time of `Currency` objects;
  - cross rates through a third currency;
  - captured reversals and adjustments of re-run processes (only `commit.append` is captured, STD-TIME-043);
  - the parsers of bank statement formats (DSN-NOD-305, PH-2 step 12).
