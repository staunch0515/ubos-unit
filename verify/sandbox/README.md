# L2 escape suite

`l2-escape-cases.json` holds the L2 escape cases of VER-SEC-0100 (EXIT-1-10): version 0.2,
382 cases, IDs L2E-001…382. L2E-001…100 are the PH-0 cases of VER-SEC-0001
(EXIT-0-06), migrated to the production host; the others were added in PH-1 step 5d-2
(DEC-058) and cover the instruction gateway, the context, typed handles, host abuse,
floats and decimals, resource exhaustion, determinism and parsing.

Each case has a `script`, a `category`, a `title` and an `expect`:

| `expect` | Meaning |
|---|---|
| `{"error": "LOGIC.…"}` | the execution fails with this registered code |
| `{"value": …}` | the execution returns this JSON value |
| `{"journaled": n}` | the execution completes and its journal holds `n` records |
| `{"deterministic": true}` | any outcome, as long as two independent runs agree |

Optional members: `profile` (default `strict`), `limits` (overrides of the `execute`
operation: `operations` as `units`, `instruction_calls`, `changes`, `depth`) and
`cancel_after_units`. In scripts, `{{R:n:text}}` repeats `text` `n` times and
`{{N:n:text}}` does the same with `$i` replaced by the index. Function definitions are
hoisted to the top level; the rest of a script is the body of the port function `run`.

Scripts see the read-only context `ctx`, the gateway `sys` (`sys.<family>.<op>(#{…})`,
`sys.uuid()`, `sys.random_bytes(n)`), the typed handle `this` of class `Invoice` (`number`
text, `amount` decimal, `qty` integer, `paid` boolean), `params` and `self_id`.

The runner is `ubos-l2-suite` (crate `ubos-dvm`): every case runs twice through the
engine's `execute` operation on a fresh in-memory engine with the L2 host enabled;
`--bench` adds the NR-PERF-010 and W-4 measurements. The recorded run is
`verify/reports/exit-1-10/`; the PH-0 run of the first 100 cases on the sandbox spike is
`verify/reports/exit-0-06/`. Case IDs are permanent; later suites add cases with new IDs.
