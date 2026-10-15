# Rust crates (`ubos`)

Kernel, node and Forge crates of the reference implementation (DEC-028: component `ubos`,
folder `crates/`). The crate list, layers and allowed dependencies are normative in
UBS-ARC-07 §2 and enforced by `docs/tools/check_deps.py` (AR-031).

PH-0 prototype crates (UBS-PH-0 §2, DEC-033): `ubos-types`, `ubos-model` (minimal class
registry), `ubos-store` (memory and PostgreSQL), `ubos-tree`, `ubos-version`, `ubos-time`,
`ubos-dvm` (adapter of STD-CONF-005) and `ubos-forge` (runner). All 453 BPA 0.1 vectors
tagged `prototype` pass on both backends (EXIT-0-03).

PH-0 step 6 (DEC-034) adds `ubos-runtime` with a prototype L2 (Rhai) host and the escape
suite runner `ubos-sandbox-spike`; all 100 initial escape cases are blocked (EXIT-0-06):

```sh
cargo build --release -p ubos-runtime
./target/release/ubos-sandbox-spike --cases verify/sandbox/l2-escape-cases.json
```

PH-0 step 5 preparation (`verify/reports/exit-0-05/`): `ubos-tree` updates stored nodes in
batches (DSN-DVM-007), and `ubos-bench tree` measures it. The stores keep commits as deltas
with branch ordinals: an object index with `cseq` intervals, ledger entries, stored tree
nodes and a state root per commit, written with the head compare-and-set in one
transaction (DSN-DVM-004, -005, -007, -008). A branch reads its own rows and falls back to
its parent at the fork point. Tables of the step-4 layout must be dropped before use.
`ubos-bench load|check|run` generates and loads WL-GENERIC and drives VER-BENCH-0001;
commits can be grouped (`Engine::commit_group`, DSN-DVM-006).

PH-1 step 11 (DEC-070) adds `ubos-rules`, the pure engine of governance sheets (STD-LGS):
grammar, AST, property registry, authoring checks, specificity, the rank and overlap
analysis of the cascade and the cascade itself; the sheet objects, their checks at commit
and the evaluation of qualifiers over a state are the kernel's (`ubos-dvm/src/sheets.rs`).
Commit enforcement, validations, invariants, the validation preview and the decisions before
command ports are `ubos-dvm/src/rules.rs` (STD-LGS-030, -031).
Sheets are compiled once per version hash and indexed by class (`ubos-rules::Cache`);
`ubos-bench sheets` and `ubos-bench invariants` measure NR-PERF-009 and DSN-DVM-708 in
memory (`verify/reports/exit-1-14/workstation/`).

PH-1 step 12 (DEC-071) adds `ubos-flow`, the pure engine of lifecycles (STD-CLS-004,
STD-ISA-017, STD-ISA-036, STD-ISA-050): the shape of a lifecycle declaration, the extension
rule for subclass lifecycles, the reachability findings of the BPU class verification, guards
and `set` effects as L1 expressions, port availability by state and the action list of an
object. The lifecycle objects, the state in the version envelope, `commit.transition`,
`run.port` availability, `read.actions` and the operation `actions` are the kernel's
(`ubos-dvm/src/flow.rs`, with the commit path in `stateful.rs` and the instructions in
`execution.rs`).
The process of an execution follows the state machine of DSN-DVM-501 (`ubos-dvm/src/process.rs`,
UBS-SYS-DVM-05 §1); `process_info` returns the phases passed and the attempts, and
`ubos-props` checks the machine (`process_state_machine`, VER-PROP-0100).

PH-1 step 13 (DEC-073) adds `ubos-query`, the pure part of criteria queries (STD-EXPR-010,
-011, STD-ISA-080): the textual form, the normalised `DAT-Query` and its hash, the key
bounds a `where` yields for the planner, the scan budgets of the profiles, HMAC-signed
snapshot cursors and the aggregates of the query form. Field indexes are declared to the
kernel (`declare_index`, before a class has instances; the fields of unique `keys` are
indexed by default) and maintained by the stores at flush (`field_index`, migration 0007;
keys encoded order-preserving per type by `ubos-types`); the kernel's `query`
(`ubos-dvm/src/expressions.rs`) reads candidates from the most selective comparison on an
indexed field, else scans the class within the budget (`QRY.SCAN_LIMIT`), and `search.query`
uses the same planner.

`ubos-authz` (step 13b, DEC-073) embeds the `cedar-policy` crate (DEC-065) and generates the
entity model from the registry: `UBOS::User` (in its `Role`s), `UBOS::VAE`, `UBOS::Branch`,
`UBOS::Class` and one entity type per business class under `UBOS::Model` whose attributes
are the policy-visible fields (classification `public` or `internal`); the actions are those
of STD-ISA-005 plus the port permissions, `query`, `merge`, `approve` and `export`; the
context is the decision context of STD-LGS-031. Policies are `Policy` objects (`name`,
`source`, `enabled`) validated against the model at commit; the adapter operation
`policy_schema` prints the schema. Optional attributes need a `has` guard
(`resource has country && resource.country == "US"`). The kernel (`ubos-dvm/src/authz.rs`)
denies by default once a branch holds policies and keeps the `grants` of STD-ISA-005 alone
without them; it enforces every instruction on its resource and on the branch, reports an
invisible resource as `IAM.NOT_FOUND`, filters query rows and logs denials
(`decision_log`).

Step 13c (DEC-073) adds the history queries and the audit trail (`ubos-dvm/src/audit.rs`):
`changes {branch, from?, to?, class?, author?, process_id?, object_id?, field?, limit?,
cursor?}` and `transitions {…, from_state?, to_state?}` walk the first-parent history of a
branch (a bound is an instant, `{commit}` or `{cseq}`) and page with signed snapshot
cursors in `(cseq, object_id)` order; `audit {branch, object_id}` reports per commit who
(principal, on behalf of whom), what (operation, field changes, states), when (transaction
and valid time), why (message, reason, change set), how (channel, client, process kind and
trigger) and the rule binding. The security log is hash-chained and append-only
(`security_log {from?, to?, limit?}`, `verify_security_log`): denials of the decision log,
policy changes, key rotation, archive export and import, limits and index declarations.
`view {branch, object_id | class, mode?, staged?, role?, device?, locale?}`
(`ubos-dvm/src/views.rs`) resolves the declared view of the mode (STD-CLS-023; a `views`
item `{mode, view: uri}` of a class, the view object's `layout` or the `layout` of the
first matching `variants` item) or generates one, and returns the data, the field states
(hidden, read-only, required, masked) and the actions for the principal, the violations of
staged edits and a `payload_hash`. `branch_info` of an overlay reports its local commits,
versions and objects.

Step 13d (DEC-074) bounds the index path: a query over one class that orders by the
indexed field of its best bound, ascending, reads `offset + limit` index rows in key order
and stops once that many rows pass the `where`, the temporal and state checks and the row
filter of a policy set (four times more rows while rows are filtered, up to the scan
budget); `declare_index` on a class with instances rebuilds the index over the heads of
every branch (`Store::put_field_rows`) and the planner uses it from each branch's rebuild
commit (`index_since`). `ubos-bench query` is the MET-PERF-016 suite (`eq`, `range`,
`eq_filter`, `page2`, `key`; memory or PostgreSQL); the workstation report is
`verify/reports/exit-1-17/workstation/`.

Run the BPA vectors tagged `prototype`:

```sh
cargo build --release -p ubos-forge
./target/release/forge conform --vectors standard/vectors --tag prototype --report report.json
./target/release/forge conform --vectors standard/vectors --tag prototype \
    --backend postgres --database "host=127.0.0.1 user=postgres dbname=bpa"
```

Toolchain: Rust stable pinned in `rust-toolchain.toml` (DEC-007). Before a commit:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 docs/tools/check_deps.py
```

Licence: AGPL-3.0-only (`LICENSE`), with a commercial licence available (DEC-019).
