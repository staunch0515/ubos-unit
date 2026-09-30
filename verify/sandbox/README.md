# L2 escape suite

`l2-escape-cases.json` holds the 100 initial L2 escape cases of VER-SEC-0001 (PH-0 step 6,
EXIT-0-06), IDs L2E-001…100, in seven categories: file 12, network 12, time 12, entropy 12,
float 14, host abuse 20, resource exhaustion 18.

Each case has a `script`, a `category`, a `title` and an `expect`:

| `expect` | Meaning |
|---|---|
| `{"error": "LOGIC.…"}` | the execution fails with this registered code |
| `{"value": …}` | the execution returns this JSON value |
| `{"journaled": n}` | the execution completes and its journal holds `n` records |
| `{"deterministic": true}` | any outcome, as long as two independent runs agree |

Optional members: `profile` (default `strict`), `limits` (overrides of `operations`,
`cpu_ms`, `wall_ms`, `memory`) and `cancel_after_ms`. In scripts, `{{R:n:text}}` repeats
`text` `n` times and `{{N:n:text}}` does the same with `$i` replaced by the index.

Scripts see three names: the constant `ctx` (`now`, `tenant`), the constant gateway `sys`
(`uuid()`, `random_bytes(n)`) and a typed handle `rec` of class `Invoice` (`number` string,
`amount` decimal, `qty` integer, `paid` boolean).

The runner is `ubos-sandbox-spike` in crate `ubos-runtime`; the report of the recorded run
is `verify/reports/exit-0-06/`. Case IDs are permanent; later suites add cases with new IDs.
