# BPA 1.0.0-rc11 — Release notes

Release candidate 11 of BPA 1.0 (PH-2 step 5a, DEC-112), the standard for the rule operations of
the PH-2 kernel. Tag `bpa-v1.0.0-rc11`, signed by the owner (DSN-BPA-001). The manifest
(`release/1.0-rc11/manifest.json`) lists every released file with its SHA-256; its `content_id`
identifies this release. BPA 1.0-rc10 stays frozen as tagged. The standard precedes the code
(AR-033): PH-2 step 5 implements this frozen text and the decision-table and simulation clauses
of 1.0-rc8 in parts 5b–5g.

## Content

| Item | Value |
|---|---|
| Profiles | Core, Bitemporal, Ledger, Proof, Governance, Protocol |
| Clauses | 255 (scope by phase, `scope.json`): the 250 clauses of BPA 1.0-rc10 and 5 written in step 5a |
| MUST clauses with a vector range | 217, 217 covered (`coverage.md`) |
| Vectors | 2,208: those of BPA 1.0-rc10 and 27 of step 5a |

## Changes since BPA 1.0-rc10

- **The end of an attachment** (STD-LGS-070, new): a sheet or decision table whose valid period ends stops applying then, and `RuleDetached` is emitted once when the business date reaches the end; moving the end before it is reached cancels the event.
- **Live impact** (STD-LGS-071, new): `rule_impact` counts, per class, the current objects whose effective declarations or failing validations change under a candidate branch's rules.
- **Rule tests** (STD-LGS-072, new): `RuleTest` objects of kind `cascade`, `decision`, `table` or `validation` with a provisional input object and an expectation; `run_rule_tests` reports the results and the coverage of sheet blocks, table rows and validations.
- **Test gate** (STD-LGS-073, new): a change set merging into a protected branch runs the rule tests it affects at the source head and fails with `RULE.TESTS_FAILED` when one fails.
- **Simulation of logic changes** (STD-LGS-074, new): `simulate` with `include: ["logic"]` also takes the source's `Logic` objects as candidates.
- **Genesis class:** `RuleTest`.
- **Adapter operations** `rule_impact` and `run_rule_tests`; `simulate` takes `include`.

## Superseded vectors

None. No vector of BPA 1.0-rc10 changed.

## Known limits

- **Kernel.** The kernel runs the decision-table, simulation and rule-operation vectors as PH-2 step 5 implements them (parts 5b–5g, EXIT-2-01, VER-CONF-8405).
- **Not in this release:**
  - recorded connector responses in simulations (PH-2 step 12); a simulation makes no external call;
  - the DMN interchange of decision tables (DSN-DVM-724, PH-2 step 16);
  - the impact count while a rule is edited in Studio (PH-2 step 16).
