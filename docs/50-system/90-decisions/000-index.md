---
id: UBS-DEC-000
title: Decision Log
status: complete
phase: ALL
depends_on: [UBS-META-00, UBS-META-06]
---

# Decision Log

Decisions are never deleted. A replaced decision is marked
`Superseded by DEC-<NNN>`.

| ID | Title | Status |
|---|---|---|
| DEC-110 | PH-2 step 4f: escalation, the service-level report and the node's sweeps — escalation steps applied by the kernel's tick, `sla_report`, the leader loop's sweep of pending changes with the default expiry and `pending_change.expired`; STD-LGS-063 and -064 claimed, every clause of BPA 1.0-rc10 implemented | Accepted |
| DEC-111 | PH-2 step 4 closed: approvals and work (parts 4a–4f, DEC-105…110) — BPA 1.0-rc10, deferred commits and approvals, governed reopening, delegations, tasks and work queues, inbox, escalation and service levels in the kernel; 92 step-4 vectors pass in memory and on PostgreSQL; deferred items listed with their target steps | Accepted |
| DEC-112 | PH-2 step 5 opened in parts 5a–5g; part 5a: BPA 1.0-rc11 — the end of a rule's attachment (STD-LGS-070), live impact (STD-LGS-071), rule tests and coverage (STD-LGS-072), the test gate of merges (STD-LGS-073), simulation of logic changes (STD-LGS-074); 255 clauses, 2,208 vectors, MUST coverage 217/217 | Accepted |
| DEC-113 | PH-2 step 5b: decision tables in the kernel — `DecisionTable`, evaluation under the hit policies with aggregators, the authoring check `check_table`, `decide_table` and `run.decision` with the table versions in the rule binding; STD-LGS-040 and STD-ISA-053 claimed | Accepted |
| DEC-114 | PH-2 step 5c: simulation in the kernel — historical port calls re-run on sandbox branches with the candidate rules and, with `include: ["logic"]`, logic; effects dropped and re-runs kept out of the VAE's processes; `SimulationRun` records; STD-LGS-044 and -074 claimed, conformance test on BPA 1.0-rc11 | Accepted |

---

The table rows and records of DEC-001 … DEC-029 are in `01-records-dec-001-029.md`
(UBS-DEC-01); those of DEC-030 … DEC-059 in `02-records-dec-030-059.md` (UBS-DEC-02);
those of DEC-060 … DEC-089 in `03-records-dec-060-089.md` (UBS-DEC-03); those of
DEC-090 … DEC-109 in `04-records-dec-090-109.md` (UBS-DEC-04); the records from DEC-110 on
follow.

### DEC-110 — Escalation, service levels and the node's sweeps (PH-2 step 4, part 4f)
- **Date:** 2026-10-25 · **Status:** Accepted
- **Context:** Part 4f of PH-2 step 4 (DEC-105) implements the escalation chains of STD-LGS-063 and the service-level report of STD-LGS-064 (BPA 1.0-rc10, DSN-DVM-744, -754) and the node's due-date job (DSN-NOD-231). It also builds the default expiry and the expiry event of pending changes that part 4b left for this part (DSN-DVM-552, DEC-106). Before the part, the four escalation vectors failed and the two report vectors were not applicable. The owner asked to continue on 2026-10-25.
- **Decision:**
  1. **Escalation.** The kernel's tick, run when the clock advances (`set_now`, and every second by the node's leader loop), applies escalation steps.
     - **Scope.** It covers the open and claimed tasks of the branches where tasks were created (`kernel_meta` `work_branches`), for every step whose time (the due time plus `after`) has come.
     - **Order.** Steps are applied in the order of their times, then of branch and task, once per task and step.
     - **Effects.** Each step is a kernel commit that records it in `escalations` and `log`, reassigns or raises the priority as declared, and emits `task.escalated` with the commit's sequence number.
  2. **Report.** `sla_report {branch, from, to}` counts, per type and queue, the tasks with a due time completed in the window on time or late, the attainment (half-even, 4 places) and the open overdue tasks, with a total.
  3. **Node.** The leader loop's tick is the periodic job of DSN-NOD-231. Every `SWEEP_SECS` it also runs `host.sweep_pending`:
     - a pending change past its expiry, or past `PENDING_EXPIRY` (`P30D`) after its request when its requirements set none, becomes `expired`;
     - the effect `pending_change.expired` is emitted.

     The adapter has no default expiry, so the conformance vectors keep the expiry of STD-TXN-060.
  4. **Claim.** The conformance test claims STD-LGS-063 and STD-LGS-064, so every clause of BPA 1.0-rc10 is implemented in the kernel.
  5. **Design notes.** DSN-DVM-552, -744, -754 and DSN-NOD-231 get phase notes:
     - workflow timers come with step 6;
     - reminder delivery comes with step 13;
     - time in state and a maintained SLA projection come with step 13.
- **Consequences:**
  - The 78 vectors of the approval, delegation and work clauses pass in memory and on PostgreSQL 16, and the conformance test passes with every clause of BPA 1.0-rc10.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
- **Alternatives considered:**
  - An index of escalation due times per branch was deferred: the tick reads the tasks of the work branches. The benchmarks of step 18 show whether the index is needed.
  - Emitting `pending_change.expired` when a read finds a pending change expired was rejected: the adapter's operations would emit effects that STD-TXN-060 does not state.
- **Affects:** STD-LGS-063, STD-LGS-064, STD-TXN-060, DSN-DVM-552, DSN-DVM-744, DSN-DVM-754, DSN-NOD-208, DSN-NOD-231, FR-FLOW-081, FR-FLOW-082, FR-FLOW-083, DEC-105, DEC-106, DEC-109

### DEC-111 — PH-2 step 4 closed: approvals and work (PH-2 step 4, part 4g)
- **Date:** 2026-10-25 · **Status:** Accepted
- **Context:** PH-2 step 4 (DEC-105) was split into parts 4a–4g. Parts 4a–4f are recorded in DEC-105…110. Part 4g closes the step: it records what the step delivered and lists, in one place, what the parts deferred to later steps. The owner asked to continue on 2026-10-25.
- **Decision:**
  1. **Delivered.**
     - **4a (DEC-105).** BPA 1.0-rc10: 250 clauses, 2,181 vectors, MUST coverage 212/212, VER-INSP-9008. Five clauses were added (STD-LGS-060…064), with notes on STD-KIND-025, STD-TXN-060 and STD-LGS-043.
     - **4b (DEC-106).** Deferred commits, requirements, expiry and invalidation, eligibility, separation of duties and approval evidence in the kernel.
     - **4c (DEC-107).** The governed reopening of periods, the adapter's directory `principal`, and the conformance test on BPA 1.0-rc10.
     - **4d (DEC-108).** Delegations and approvals through a delegation. BPA 1.0-rc10 was corrected before its tag (slugs of kernel-written objects).
     - **4e (DEC-109).** Tasks, work queues, assignment rules, due rules at creation and the inbox.
     - **4f (DEC-110).** Escalation by the kernel's tick, the service-level report, and the node's sweep of pending changes.
  2. **Verification.** The 92 current vectors of the step-4 clauses (STD-TXN-060, STD-LGS-041…043, STD-LGS-060…064, STD-KIND-025) pass in one run in memory and on PostgreSQL 16. The kernel conformance test claims every clause of BPA 1.0-rc10.
  3. **Deferred.** Each item keeps its ID and moves to the step named:

     | Item | Source | Target |
     |---|---|---|
     | Approvals and human tasks inside workflow steps; escalation driven by workflow timers (DSN-DVM-744, -751) | DEC-105, DEC-110 | step 6 |
     | Delivery of escalation reminders and other notifications to people over channels | DEC-105, DEC-110 | step 13 |
     | The inbox as a saved query over tasks (DSN-DVM-743) | DEC-109 | step 13 |
     | Time in state and a maintained SLA projection over lifecycle history (DSN-DVM-754) | DEC-110 | step 13 |
     | Inbox and approval screens | DEC-105 | step 14 |
     | Re-running the original process of an invalidated pending change for a new preview (DSN-DVM-551) | DEC-106 | after BPA 1.0-rc10, if the standard takes it |
     | Delegation chains longer than one principal; queue membership by role | DEC-105 | after BPA 1.0-rc10 |
     | Indexes for the load of `least_loaded` and for escalation due times | DEC-109, DEC-110 | step 18, if the benchmarks need them |

  4. **Owner actions.** Merge the branch, then sign the tags `bpa-v1.0.0-rc8`, `bpa-v1.0.0-rc9` and `bpa-v1.0.0-rc10`.
  5. **Next.** Step 5 (decision tables and rule operations): decision tables, attaching and detaching rules with effective dates, simulation and impact analysis, rule tests, the simulator. Its plan goes to the owner first.
- **Consequences:**
  - The status table in `docs/reports/ph2-plan-zh.md` marks step 4 as done.
  - The deferred items are picked up by the steps named in the table above, whose plans cite this decision.
- **Alternatives considered:**
  - Leaving the deferred items spread over DEC-105…110 was rejected, as in DEC-104.
- **Affects:** DEC-105, DEC-106, DEC-107, DEC-108, DEC-109, DEC-110, DSN-DVM-551, DSN-DVM-743, DSN-DVM-744, DSN-DVM-751, DSN-DVM-754

### DEC-112 — Decision tables and rule operations, part 5a: BPA 1.0-rc11 (PH-2 step 5)
- **Date:** 2026-10-25 · **Status:** Accepted
- **Context:** PH-2 step 5 (UBS-PH-2 §11) implements decision tables and the operations around rules: attaching and detaching rules for periods, simulation and impact analysis, rule tests, and the simulator (CAP-RULE-04, -06, -07, -08, CAP-DEV-04).
  - **Baseline.** On the kernel, 31 of the 49 vectors of STD-LGS-040, STD-ISA-053 and STD-LGS-044 (BPA 1.0-rc8) fail or are not applicable: decision tables and simulations are missing. The 18 vectors of sheets with valid periods and scopes (STD-LGS-002, -010) pass.
  - **Gap in the standard.** Four capabilities have no clauses: the end of a time-boxed attachment with its event (FR-RULE-062), the live impact count (FR-RULE-074), rule tests with their merge gate and coverage (FR-RULE-081…083), and the simulation of logic changes (FR-DEV-041, -042). The standard precedes the code (AR-033).
  - **Confirmation.** The owner confirmed the plan of step 5 with the default suggestions on 2026-10-25.
- **Decision:**
  1. **Parts.** Step 5 is done in parts, one pull request each:
     - 5a writes BPA 1.0-rc11 (standard only);
     - 5b: decision tables in the kernel (STD-LGS-040, STD-ISA-053);
     - 5c: simulation (STD-LGS-044, -074);
     - 5d: the end of attachments and the live impact count (STD-LGS-070, -071);
     - 5e: rule tests and the merge gate (STD-LGS-072, -073);
     - 5f: `forge simulate` (DSN-FRG-205);
     - 5g closes the step.

     Recorded connector responses in simulations stay in step 12; DMN interchange and the impact shown while editing stay in step 16.
  2. **Release.** BPA 1.0-rc11 is opened with the scope rule of DEC-089 on base 1.0-rc10. It is released at the end of part 5a with the owner's signed tag `bpa-v1.0.0-rc11`, and is then frozen like the earlier candidates (DEC-045 item 3).
  3. **Clauses.** A new section 8 of the governance standard (Governance profile):
     - **STD-LGS-070.** An attachment ends where a valid period of a sheet or decision table ends and no later one starts. `RuleDetached` is emitted once when the business date reaches it, symmetric to `BecameEffective`; moving the end cancels the event.
     - **STD-LGS-071.** `rule_impact` compares the effective declarations and failing validations of the target's governed objects under the target's rules and under the candidate branch's sheets, property declarations and decision tables, and counts the changes per class.
     - **STD-LGS-072.** The genesis class `RuleTest`, of kind `cascade`, `decision`, `table` or `validation`, evaluated on a provisional input object that is never stored. `run_rule_tests` reports the results and the coverage of sheet blocks, table rows and validations.
     - **STD-LGS-073.** A change-set merge into a protected branch first runs, at the source head, the tests the change affects: the tests it changes, the tests whose target it changes, and the tests without a target when it changes rules or logic. A failure stops the merge with `RULE.TESTS_FAILED`.
     - **STD-LGS-074.** `simulate` with `include: ["logic"]` takes the source's `Logic` objects as candidates too; without it, simulation is as in 1.0-rc8.
  4. **Reference model.**
     - **Code.** `standard/tools/bpa/ruleops.py` and changes to `approvals.py` (`include`), `workflow.py` (the detach schedule, the merge gate), `workflows.py`, `classes.py`, `store.py` and `ops.py`.
     - **Cases.** `standard/tools/cases/ruleops.py`.
- **Consequences:**
  - BPA 1.0-rc11 has 255 clauses and 2,208 vectors (27 added, none superseded). MUST coverage is 217/217, and VER-INSP-9009 records it.
  - No released vector changed.
  - Parts 5b–5g implement 1.0-rc11 and the decision-table and simulation clauses of 1.0-rc8 in the kernel.
- **Alternatives considered:**
  - A separate attachment object with its own start and end was rejected: a sheet's valid periods already state where it applies (STD-LGS-002), and a second object would give two sources for one fact.
  - Rule tests that commit their input objects on a sandbox branch were rejected: a provisional object gives the same evaluation without branches or commits.
  - Running every rule test of the branch at each merge was rejected: FR-RULE-082 asks for the tests the change affects, and the cost would grow with the test suite.
- **Affects:** STD-LGS-002, STD-LGS-044, STD-LGS-070, STD-LGS-071, STD-LGS-072, STD-LGS-073, STD-LGS-074, STD-CONF-005, DSN-DVM-710, DSN-DVM-760, DSN-DVM-761, DSN-DVM-763, DSN-DVM-764, DSN-FRG-205, FR-RULE-062, FR-RULE-074, FR-RULE-081, FR-RULE-082, FR-RULE-083, FR-DEV-041, FR-DEV-042, VER-INSP-9009, DEC-045, DEC-089, DEC-111

### DEC-113 — Decision tables in the kernel (PH-2 step 5, part 5b)
- **Date:** 2026-10-25 · **Status:** Accepted
- **Context:** Part 5b of PH-2 step 5 (DEC-112) implements the decision tables of STD-LGS-040 and the instruction `run.decision` of STD-ISA-053 (BPA 1.0-rc8, DSN-DVM-722, -723). Before the part, 19 of their 25 vectors failed and 6 were not applicable: the kernel had no `DecisionTable` class, no table operations and no `run.decision`. The owner confirmed the part with the default suggestions on 2026-10-25.
- **Decision:**
  1. **Tables.** `crates/ubos-rules/src/tables.rs` follows the reference model:
     - **Declaration check.** Inputs, outputs, rows and hit policy; an aggregator only with `collect`, and `sum`, `min` and `max` over numeric outputs; every row gives every output, with values valid for the output's type; priorities in a `priority` table.
     - **Evaluation.** The inputs are checked against their declarations. Each cell is an L1 boolean over `params` (`-` matches any value), cached per cell and inputs. The hit policies `unique` (`RULE.NO_MATCH`, `RULE.MULTIPLE_MATCH`), `first`, `priority` and `collect` apply, with the aggregators `sum`, `min`, `max` and `count` (money in one currency, `MODEL.CURRENCY_MISMATCH`).
     - **Authoring check.** `check_table` evaluates the representative values of each input: the domain of booleans and enumerations, and for integers, decimals and dates the literals of the conditions with the values around and between them. It reports the gaps and the overlaps the hit policy forbids, up to 10,000 combinations; other inputs are listed as unchecked.
  2. **Kernel.** `DecisionTable` joins the genesis registry as a Definition class below `ConfigAsset`.
     - `decide_table` evaluates a table named by ID or URI at `asof` (default today).
     - `run.decision` evaluates one at the process `asof`, after the `evaluate` permission. It charges one unit per row and records the table version in the rule binding of the commit (`tables`, STD-TIME-040).
  3. **Claim.** The conformance test claims STD-LGS-040 and STD-ISA-053.
  4. **DSN-DVM-722.** A phase note records row evaluation with a cache instead of a compiled tree, and the authoring check as an operation rather than a refusal at commit.
- **Consequences:**
  - The 25 vectors pass in memory and on PostgreSQL 16, and the conformance test passes.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
- **Alternatives considered:**
  - Refusing a table with gaps at commit was rejected: STD-LGS-040 makes the check an authoring aid, and a table with a deliberate gap under `collect` or a default row is valid.
  - A compiled decision tree was deferred: the vectors' tables have a handful of rows, and the benchmarks of step 18 show whether one is needed.
- **Affects:** STD-LGS-040, STD-ISA-053, STD-TIME-040, DSN-DVM-722, DSN-DVM-723, FR-RULE-041, FR-RULE-042, FR-RULE-043, DEC-112

### DEC-114 — Simulation in the kernel (PH-2 step 5, part 5c)
- **Date:** 2026-10-25 · **Status:** Accepted
- **Context:** Part 5c of PH-2 step 5 (DEC-112) implements the simulation of rule changes of STD-LGS-044 (BPA 1.0-rc8, DSN-DVM-760, -761, -762) and of logic changes of STD-LGS-074 (BPA 1.0-rc11) in the kernel. Before the part, the simulation vectors were not applicable: `simulate` was missing. The owner confirmed the part with the default suggestions on 2026-10-25.
- **Decision:**
  1. **Selection.** `crates/ubos-dvm/src/simulation.rs` follows the reference model.
     - **Calls.** The historical port calls of the target are read from the journals of the VAE's named and unnamed processes. Each is selected by window (the date of its context's `now`) and by an optional port.
     - **Order.** Calls are ordered by date, then by the sequence number of their first commit on the target, then by process ID.
  2. **Re-run.** For each call within the budget of processes:
     - **Sandbox.** A sandbox branch `sandbox/sim-<run>-<n>` is created from the target as of just before the call's commit.
     - **Candidates.** The candidates of the source branch are committed on it: sheets, property declarations, decision tables, and with `include: ["logic"]` logic assets.
     - **Execution.** The journaled invocation is executed again in the `simulation` profile as a new process.
     - **Comparison.** The outcome (committed with the result hash, or failed with the error code) is compared with the actual one.
  3. **Isolation.** While a re-run executes, a thread-local flag marks it as a simulation. Its effects are dropped before the commit, and it is recorded neither in the run index nor as a duty, so later simulations, `process_info` and `metering_total` do not see it. The sandbox branches stay as evidence.
  4. **Record.** The run is stored as a `SimulationRun` System object on the target, with the budget, coverage, results and candidate versions. A run given a change set is added to the change set's `simulations`. `SimulationRun` joins the genesis registry.
  5. **Claim.** The conformance test claims STD-LGS-044 and STD-LGS-074 and runs the vectors of BPA 1.0-rc11 (`NEXT_RELEASE`).
  6. **Design notes.** DSN-DVM-760, -761 and -762 get phase notes:
     - effects are dropped, not captured, and connectors wait for step 12;
     - outcomes are compared by result hash;
     - budgets other than processes stay the simulation profile's limits.
- **Consequences:**
  - The 10 simulation vectors pass in memory and on PostgreSQL 16, and the conformance test passes on BPA 1.0-rc11.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
- **Alternatives considered:**
  - Re-running under the original process ID, as the reference model does before restoring its tables, was rejected: the kernel's process records are stored, and a re-run would overwrite the original's.
  - Capturing a re-run's effects on the run record was deferred: STD-LGS-044 requires only that they are not released.
- **Affects:** STD-LGS-044, STD-LGS-074, DSN-DVM-653, DSN-DVM-760, DSN-DVM-761, DSN-DVM-762, FR-RULE-071, FR-RULE-072, FR-RULE-073, FR-DEV-041, FR-DEV-042, DEC-112
