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
| DEC-115 | PH-2 step 5d: the end of rule attachments and the live impact count in the kernel — detach timers with `RuleDetached`, `rule_impact` per class; STD-LGS-070 and -071 claimed | Accepted |
| DEC-116 | PH-2 step 5e: rule tests and the merge gate in the kernel — `RuleTest` with its declaration check, `run_rule_tests` on provisional objects with coverage, the test gate of change-set merges into protected branches; STD-LGS-072 and -073 claimed | Accepted |
| DEC-117 | PH-2 step 5f: `forge simulate` — the Server's administrative simulation route, audited; the command with window, port, budget, logic and change-set options, text and JSON reports, exit status by changed outcomes | Accepted |
| DEC-118 | PH-2 step 5 closed: decision tables and rule operations (parts 5a–5f, DEC-112…117) — BPA 1.0-rc11, decision tables, simulation of rule and logic changes, the end of attachments, live impact, rule tests and the merge gate in the kernel, `forge simulate`; 58 step-5 vectors pass in memory and on PostgreSQL; deferred items listed with their target steps | Accepted |
| DEC-119 | PH-2 step 6 opened in parts 6a–6g; part 6a: BPA 1.0-rc12 — saga timeouts, monitoring and intervention (STD-TXN-081), workflow tasks and approvals (STD-TXN-093), workflow timers, business calendars and timeouts (STD-TXN-094), `ask.input` (STD-ISA-101); 259 clauses, 2,275 vectors, MUST coverage 221/221 | Accepted |
| DEC-120 | PH-2 step 6b: workflows in the kernel — `WorkflowDefinition` and `WorkflowInstance`, the step interpreter with idempotent actions, decisions, parallel branches and joins, sub-workflows, events with correlation and buffering, human tasks, timers fired by the tick, and migration; STD-TXN-090…092 claimed, conformance test on BPA 1.0-rc12 | Accepted |
| DEC-121 | PH-2 step 6c: workflow timers as rows written with the commit and fired by the tick of the VAE's lease holder, in place of a scan of the running instances | Accepted |
| DEC-122 | PH-2 step 6d: sagas and quarantine in the kernel — `Saga`, `Quarantine`, `QuarantineRelease`, `run_saga` with resumption, step timeouts and escalation, `sagas` and `saga_action`, saga jobs on the node, the quarantine of failing asset versions; STD-TXN-080, -081 and STD-CTX-031 claimed | Accepted |
| DEC-123 | PH-2 step 6e: workflow tasks and approvals, timers with business calendars and timeouts in the kernel — `human_task` steps on work queues create `Task`s that continue the instance, deferred actions wait for their pending change, `at` timers, timeouts, timer rows for timeouts and expiries, due times recomputed by migration; STD-TXN-093 and -094 claimed | Accepted |
| DEC-124 | PH-2 step 6f-1: `ask.input` in the kernel — `needs_input` without a commit, requests kept by invocation and name, answers validated and journaled, consumed when the process commits; STD-ISA-101 claimed, every clause of BPA 1.0-rc12 in the kernel's scope implemented | Accepted |

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

### DEC-115 — The end of attachments and the live impact count in the kernel (PH-2 step 5, part 5d)
- **Date:** 2026-10-25 · **Status:** Accepted
- **Context:** Part 5d of PH-2 step 5 (DEC-112) implements the end of a rule's attachment of STD-LGS-070 and the live impact count of STD-LGS-071 (BPA 1.0-rc11, DSN-DVM-710, -763) in the kernel. Before the part, their 9 vectors were not applicable or failed. The owner confirmed the part with the default suggestions on 2026-10-25.
- **Decision:**
  1. **Detach timers.** A commit writing a sheet or a decision table adds a timer for every end of its timeline where no later segment starts and the business date has not reached it. The timer is written with the flush, like the timers of `BecameEffective` (STD-TIME-050), and its key carries `detach` in place of a version, which both stores keep.
  2. **Event.** When the clock reaches the end and the object's timeline still ends there, the kernel's tick emits `RuleDetached` once, with the effect ID `rule-detached/<branch>/<object>/<date>` and the payload `{class, valid_to}`. An end moved before it is reached fires nothing at the old date. `due_effective` leaves detach timers out.
  3. **Impact.** `rule_impact` in `crates/ubos-dvm/src/ruleops.rs` re-evaluates every governed object of the target: the live Definition objects that are not configuration or logic assets, optionally limited to classes.
     - **Comparison.** Each object is evaluated over the target's state and over the same state with the source branch's sheets, property declarations and decision tables. Their effective declarations and failing validations are compared.
     - **Output.** The objects that differ are counted per class.
  4. **Claim.** The conformance test claims STD-LGS-070 and STD-LGS-071.
  5. **Design notes.** DSN-DVM-710 and -763 get phase notes:
     - an attachment is the rule's valid period;
     - the impact is counted by re-evaluation rather than through the selector index, and the count while editing comes with step 16.
- **Consequences:**
  - The 9 vectors pass in memory and on PostgreSQL 16, and the conformance test passes.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
- **Alternatives considered:**
  - A separate detach schedule in `kernel_meta`, as the reference model keeps it, was rejected: the flush's timers already persist with the commit and are fired by the same tick.
  - Counting the impact through the selector index (DSN-DVM-763) was deferred: re-evaluation is exact, and the benchmarks of step 18 show whether the index is needed for large VAEs.
- **Affects:** STD-LGS-070, STD-LGS-071, STD-TIME-050, DSN-DVM-710, DSN-DVM-763, FR-RULE-062, FR-RULE-074, DEC-112

### DEC-116 — Rule tests and the merge gate in the kernel (PH-2 step 5, part 5e)
- **Date:** 2026-10-25 · **Status:** Accepted
- **Context:** Part 5e of PH-2 step 5 (DEC-112) implements the rule tests of STD-LGS-072 and the test gate of STD-LGS-073 (BPA 1.0-rc11, DSN-DVM-764) in the kernel. Before the part, their 14 vectors were not applicable or failed. The owner confirmed the part with the default suggestions on 2026-10-25.
- **Decision:**
  1. **Class.** `RuleTest` joins the genesis registry as a Definition class below `ConfigAsset`. A commit that writes one checks its kind, its input and its expectation (`MODEL.INVALID_VALUE`, `MODEL.DECLARATION_INCOMPLETE`).
  2. **Evaluation.** `run_rule_tests` in `crates/ubos-dvm/src/ruleops.rs` follows the reference model. The input object of a test is provisional: its body is evaluated over the state of the branch and never stored.
     - **Cascade.** The test compares the effective values of the expected paths.
     - **Decision.** The test compares the outcome and its steps. The decisions are those of `decide`, through `decide_body`, which `decide_in` now uses for stored objects too.
     - **Table.** The test compares the outputs of the named table at `asof`.
     - **Validation.** The test compares the names of the failing validations.
     - **Errors.** An evaluation error is the actual outcome `{error}`, and a test may expect it.
  3. **Coverage.** The report counts the sheet blocks the tests' objects matched, the table rows the table tests hit, and the validations the validation tests evaluated, each of the totals of the branch.
  4. **Gate.** Before a change set merges into a protected branch, the kernel runs at the source head the tests the change affects:
     - the tests it changes;
     - the tests whose target it changes;
     - the tests without a target when rules or logic change.

     A failure stops the merge with `RULE.TESTS_FAILED` and the failing tests.
  5. **Claim.** The conformance test claims STD-LGS-072 and STD-LGS-073.
  6. **DSN-DVM-764.** A phase note records tests as objects and the gate on affected tests.
- **Consequences:**
  - The 14 vectors pass in memory and on PostgreSQL 16, and the conformance test passes.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
- **Alternatives considered:**
  - Running every rule test at each protected merge was rejected, as in DEC-112.
  - A provisional object stored on a sandbox branch was rejected: evaluating the body over the branch state gives the same result without commits.
- **Affects:** STD-LGS-072, STD-LGS-073, DSN-DVM-764, FR-RULE-081, FR-RULE-082, FR-RULE-083, DEC-112

### DEC-117 — `forge simulate` (PH-2 step 5, part 5f)
- **Date:** 2026-10-25 · **Status:** Accepted
- **Context:** Part 5f of PH-2 step 5 (DEC-112) builds the simulator of DSN-FRG-205 on the kernel's simulation (DEC-114). UBTP has no simulation message, and a simulation creates sandbox branches and a `SimulationRun` on the target branch. The owner confirmed the part with the default suggestions on 2026-10-25.
- **Decision:**
  1. **Route.** The Server's administration interface gains `POST /api/v1/admin/simulate?vae=<name>`. It passes the body to the VAE's `simulate` operation and writes the audit entry `admin.simulate` with the caller, the branches, the run and whether it succeeded.
  2. **Permission.** The route needs an administrator of the tenant. A permission of its own for business analysts waits for the Web workspace (PH-2 step 14).
  3. **Command.** `forge simulate --server <host:port> --source <branch>` with `--target` (default `main`), `--vae`, `--from`, `--to`, `--port`, `--budget` (default 100 processes), `--include-logic`, `--change-set`, `--token` or `UBOS_TOKEN`, and `--json`. The command draws the run's UUID itself.
  4. **Report.** The text report gives the coverage and one line per changed call: date, process, old and new outcome, and the sandbox branch whose commits hold the new values. `--json` prints the stored run.
  5. **Exit status.** The command exits with 0 when no outcome changed, 1 when some changed, and 2 on a usage, transport or Server error, so a pipeline can stop on a changed outcome.
  6. **DSN-FRG-205.** A phase note records the route, the permission and the report.
- **Consequences:**
  - The command and the route reuse the kernel's simulation; no new clause or vector.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
- **Alternatives considered:**
  - A UBTP message for simulation was rejected for now: it would extend the protocol for one administrative operation.
  - Running the simulation inside `forge` on a pulled copy of the history was rejected: the history, its rules and the evidence of the run belong to the VAE.
- **Affects:** DSN-FRG-205, DSN-DVM-760, DSN-DVM-761, STD-LGS-044, STD-LGS-074, FR-DEV-041, FR-DEV-042, DEC-112, DEC-114

### DEC-118 — PH-2 step 5 closed: decision tables and rule operations (PH-2 step 5, part 5g)
- **Date:** 2026-10-25 · **Status:** Accepted
- **Context:** PH-2 step 5 (DEC-112) was split into parts 5a–5g. Parts 5a–5f are recorded in DEC-112…117. Part 5g closes the step: it records what the step delivered and lists, in one place, what the parts deferred to later steps. The owner confirmed the part with the default suggestions on 2026-10-25.
- **Decision:**
  1. **Delivered.**
     - **5a (DEC-112).** BPA 1.0-rc11: 255 clauses, 2,208 vectors, MUST coverage 217/217, VER-INSP-9009. Five clauses were added (STD-LGS-070…074).
     - **5b (DEC-113).** Decision tables with hit policies and aggregators, the authoring check, `decide_table` and `run.decision`.
     - **5c (DEC-114).** Simulation of rule and logic changes on sandbox branches, effects dropped, `SimulationRun` records; the conformance test on BPA 1.0-rc11.
     - **5d (DEC-115).** The end of rule attachments with `RuleDetached`, and the live impact count.
     - **5e (DEC-116).** Rule tests with coverage and the test gate of merges into protected branches.
     - **5f (DEC-117).** `forge simulate` on the Server's audited administrative route.
  2. **Verification.** The 58 current vectors of the step-5 clauses (STD-LGS-040, STD-ISA-053, STD-LGS-044, STD-LGS-070…074) pass in one run in memory and on PostgreSQL 16. The kernel conformance test claims every clause of BPA 1.0-rc11.
  3. **Correction.** DEC-117 named the Web workspace as step 9; it is step 14 (UBS-PH-2 §11), and DEC-117 now says so.
  4. **Deferred.** Each item keeps its ID and moves to the step named:

     | Item | Source | Target |
     |---|---|---|
     | Recorded connector responses in simulations, and the capture of a re-run's effects on the run record (DSN-DVM-760) | DEC-112, DEC-114 | step 12 |
     | A permission of its own to run simulations, for business analysts | DEC-117 | step 14 |
     | DMN interchange of decision tables, and the impact shown while a rule is edited (DSN-DVM-763) | DEC-112, DEC-115 | step 16 |
     | Old and new values of each changed call in the simulator's report, beyond outcomes, result hashes and sandbox branches (DSN-FRG-205) | DEC-117 | step 16 |
     | A compiled decision tree for large tables; the impact count through the selector index (DSN-DVM-763) | DEC-113, DEC-115 | step 18, if the benchmarks need them |
     | A UBTP message for simulation | DEC-117 | when a client other than `forge` needs it |

  5. **Owner actions.** Merge the branch, then sign the tags `bpa-v1.0.0-rc8`, `bpa-v1.0.0-rc9`, `bpa-v1.0.0-rc10` and `bpa-v1.0.0-rc11`.
  6. **Next.** Step 6 (long-running work): workflows and sagas, long-running processes, effects after commit, replay and rollback, with the approvals in workflow steps that DEC-111 moved there. Its plan goes to the owner first.
- **Consequences:**
  - The status table in `docs/reports/ph2-plan-zh.md` marks step 5 as done.
  - The deferred items are picked up by the steps named in the table above, whose plans cite this decision.
- **Alternatives considered:**
  - Leaving the deferred items spread over DEC-112…117 was rejected, as in DEC-104 and DEC-111.
- **Affects:** DEC-112, DEC-113, DEC-114, DEC-115, DEC-116, DEC-117, DSN-DVM-760, DSN-DVM-763, DSN-FRG-205

### DEC-119 — Durable work, part 6a: BPA 1.0-rc12 (PH-2 step 6)
- **Date:** 2026-10-25 · **Status:** Accepted
- **Context:** PH-2 step 6 (UBS-PH-2 §11) implements durable work: workflows and sagas, long-running processes, post-commit effects, replay and backfill (CAP-FLOW-06, -07, CAP-TXN-05, -06, CAP-EVT-07). It also takes the items earlier steps moved to it: approvals and human tasks inside workflow steps with escalation by workflow timers (DEC-111), the maintenance of `async` projections by a job with the `timeout_ms` wait (DEC-100, DEC-104), and projection backfill (DEC-098).
  - **Baseline.** On the kernel, the 51 vectors of STD-TXN-080 and STD-TXN-090…092 and the 10 of STD-CTX-031 (BPA 1.0-rc8) fail: workflow definitions, sagas and quarantines are missing. The 23 vectors of STD-TXN-051 and STD-ISA-090 are not applicable without connectors. The vectors of STD-TXN-050 and -070 pass.
  - **Gap in the standard.** Four capabilities have no clauses, and the standard precedes the code (AR-033):
    - saga step timeouts with monitoring and intervention (FR-TXN-063, -064);
    - human tasks on work queues and approvals inside workflows (FR-FLOW-062, DEC-111);
    - business-calendar timers and timeouts of waits (FR-FLOW-063, -082);
    - actions that ask for input (FR-FLOW-071).

    Replay to a subscription (DSN-NOD-206) and projection backfill (DSN-NOD-207) need no clause. Subscriptions are a node platform class outside BPA 1.0 (DEC-076), and STD-TIME-021 already fixes what an `async` projection reads.
  - **Confirmation.** The owner confirmed the plan of step 6 with the default suggestions on 2026-10-25.
- **Decision:**
  1. **Parts.** Step 6 is done in parts:
     - 6a writes BPA 1.0-rc12 (standard only);
     - 6b: workflow definitions, instances and migration in the kernel (STD-TXN-090, -092);
     - 6c: events, timers and human tasks (STD-TXN-091), with the node's timer service (DSN-NOD-227);
     - 6d: sagas as jobs (STD-TXN-080, DSN-NOD-230) and quarantine (STD-CTX-031);
     - 6e: the workflow and saga clauses of 1.0-rc12 (STD-TXN-081, -093, -094);
     - 6f: post-commit and replay work: `async` projections maintained by a job, the `timeout_ms` wait, projection backfill, event replay to a subscription with the `replay` mark (DSN-NOD-206, -207), and `ask.input` (STD-ISA-101);
     - 6g closes the step.

     Pre-commit connector calls (STD-TXN-051, STD-ISA-090) stay in step 12 with the connector framework. No new tests are written. Resumption after a crash is verified by the `interrupt_after` vectors, and fault and simulation scenarios for workflows come with steps 11 and 20.
  2. **Release.** BPA 1.0-rc12 is opened with the scope rule of DEC-089 on base 1.0-rc11. It is released at the end of part 6a with the owner's signed tag `bpa-v1.0.0-rc12`, and is then frozen like the earlier candidates (DEC-045 item 3).
  3. **Clauses.**
     - **STD-TXN-081 (Core).** A saga step may declare a `timeout`. A step that times out, by its timeout or by a limit termination, follows its `on_timeout`: `recover`, or `escalate` to the state `stuck` with the effect `SagaStuck`. `sagas` lists sagas by state, by age of the step in progress and by step. `saga_action` lets the saga's `operators` retry or skip the step of a stuck or forward-failed saga, or cancel it; each action is logged.
     - **STD-TXN-093 (Governance).** A `human_task` step with a `queue` creates a `Task` (STD-LGS-061) with the member `workflow`. Completing the task continues the instance with its outcome; cancelling it follows `on_cancel`. An `action` step with `defer` waits for its pending change: an applied change continues to `next`, any other decision to `on_reject`.
     - **STD-TXN-094 (Core).** A `timer` is due `after` a duration or `at` an L1 expression evaluated on entry, with the entry instant as the context's `now` and the step's business `calendars`. `wait_event` and `human_task` steps may declare `timeout` with `on_timeout`; a timeout cancels the step's task. A migration recomputes due times from the entry instant.
     - **STD-ISA-101 (Core).** `ask.input` ends a process `needs_input` without a commit and returns the request. The same call repeated with `answers` before the expiry continues it, and later answers fail with `FLOW.INPUT_EXPIRED`.
     - **Errors.** `FLOW.TIMEOUT` and `FLOW.INPUT_EXPIRED` join the registry (STD-ERR-002).
  4. **Compatibility.** New members are written only when a declaration uses them, so no released vector of workflows, sagas or tasks changes. Business calendars are a member of the timer step, not of `WorkflowDefinition`: a new member of that class would change the version hashes in the released workflow vectors.
  5. **Reference model.**
     - **Code.** Changes to `workflows.py` (timers, timeouts, tasks, approvals, saga timeouts, `sagas`, `saga_action`), `exec.py` (`ask.input`), `work.py` and `approvals.py` (the continuation hooks), `classes.py` and `ops.py`.
     - **Cases.** `standard/tools/cases/durable.py`.
- **Consequences:**
  - BPA 1.0-rc12 has 259 clauses and 2,275 vectors: 68 added and one superseded. VER-CONF-3004, the instruction catalogue of 1.0-rc9…rc11, is superseded by VER-CONF-3005. MUST coverage is 221/221, and VER-INSP-9010 records it.
  - Parts 6b–6g implement 1.0-rc12 and the workflow, saga and quarantine clauses of 1.0-rc8 in the kernel.
- **Alternatives considered:**
  - Workflow human tasks that stay separate from `Task` objects were rejected: they would keep two inboxes and two escalation mechanisms (FR-FLOW-046, DEC-111).
  - A separate wall-clock watchdog for saga steps was rejected: the commit that starts a step records its start, so a resumed saga can judge the timeout deterministically.
  - Dialogues that keep a suspended process were rejected: re-running the deterministic process with the answers commits in one step and keeps nothing between the calls (FR-FLOW-071).
- **Affects:** STD-TXN-080, STD-TXN-081, STD-TXN-090, STD-TXN-091, STD-TXN-092, STD-TXN-093, STD-TXN-094, STD-ISA-001, STD-ISA-101, STD-LGS-061, STD-ERR-002, STD-CONF-005, DSN-DVM-564, DSN-DVM-565, DSN-DVM-735, DSN-DVM-744, DSN-DVM-751, DSN-NOD-206, DSN-NOD-207, DSN-NOD-227, FR-TXN-063, FR-TXN-064, FR-FLOW-062, FR-FLOW-063, FR-FLOW-071, FR-FLOW-082, VER-INSP-9010, DEC-045, DEC-089, DEC-098, DEC-100, DEC-111

### DEC-120 — Workflows in the kernel (PH-2 step 6, part 6b)
- **Date:** 2026-10-25 · **Status:** Accepted
- **Context:** Part 6b of PH-2 step 6 (DEC-119) implements the workflow clauses of BPA 1.0-rc8 in the kernel: STD-TXN-090, -091 and -092 (DSN-DVM-750…753). Before the part, their 38 vectors failed: the kernel had no workflow classes or operations. The vectors of definitions and migration also complete human tasks and deliver events, so the part takes STD-TXN-091 at the engine level too. The owner confirmed the part with the default suggestions on 2026-10-25.
- **Decision:**
  1. **Classes.** `WorkflowDefinition` (Definition, below `ConfigAsset`) and `WorkflowInstance` (System) join the genesis registry with the declarations of the reference model. A commit that writes a definition checks it, including the step members of BPA 1.0-rc12. An instance is written only by the kernel (`LOGIC.FORBIDDEN_TARGET`).
  2. **Interpreter.** `crates/ubos-dvm/src/workflows.rs` follows the reference model: one kernel-written commit per step.
     - **Actions.** An action runs as a `job` process linked by `workflow`. Its outcome is kept under `(instance, token, step, visit, attempt)`, so that a step is not run twice.
     - **Branching.** Decisions use their cases or a decision table (`decide_table`). Parallel steps fork tokens, and a join fires once.
     - **Sub-workflows.** A child instance has an ID derived from the parent's token and step.
  3. **Events, tasks and timers.** `deliver_event` wakes the step that has waited longest for the type and key, or buffers the event for a later wait within its window. `complete_task` is idempotent per visit. The kernel's tick fires due timers in the order of their due times, after `BecameEffective` and before the escalation of tasks, as in the reference model.
  4. **State.** The kernel's own workflow state is kept in `kernel_meta`: the instances in start order, action outcomes, delivered events and buffered events per branch. It works on both stores without a migration.
  5. **Claim.** The conformance test claims STD-TXN-090, -091 and -092 and runs the vectors of BPA 1.0-rc12 (`NEXT_RELEASE`).
  6. **Part 6c.** It now covers the timer rows written in the flush and claimed by the node's timer service (DSN-DVM-751, DSN-NOD-227), in place of the scan of running instances, and the advance of workflows on a Server.
  7. **Design notes.** DSN-DVM-750, -751, -752 and -753 get phase notes.
- **Consequences:**
  - The 38 vectors pass in memory and on PostgreSQL 16, and the conformance test passes on BPA 1.0-rc12.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
- **Alternatives considered:**
  - Compiling definitions into a stored state graph was rejected for now: the checked definition version is immutable and pinned, so interpreting it gives the same behaviour without a second representation.
  - An index of correlation keys was deferred to the benchmarks of PH-2 step 18 (DSN-DVM-752).
- **Affects:** STD-TXN-090, STD-TXN-091, STD-TXN-092, DSN-DVM-750, DSN-DVM-751, DSN-DVM-752, DSN-DVM-753, DSN-NOD-227, DEC-119

### DEC-121 — Workflow timer rows and the node's timer service (PH-2 step 6, part 6c)
- **Date:** 2026-10-31 · **Status:** Accepted
- **Context:** Part 6c of PH-2 step 6 (DEC-119, as adjusted by DEC-120) stores workflow timers as rows written with the commit (DSN-DVM-751) and fires them on a Server (DSN-NOD-227). After part 6b, the tick found due timers by scanning every running instance. The owner confirmed the part with the default suggestions.
- **Decision:**
  1. **Rows.** A commit that writes a `WorkflowInstance` whose token waits for a due time adds a timer row for it to the flush, in the store's timer table beside the `BecameEffective` rows (DEC-054).
     - Its key is `[branch, instance, due, "wf:<token>:<seq>"]`. The mark takes the place of the version, which both stores keep, as the `detach` mark does (DEC-115).
     - It has instant granularity.
  2. **Firing.** The tick reads the due rows with the other timers and removes them. After the `BecameEffective` effects, it fires them in the order of `(due, branch, instance, token)`.
     - A row whose token no longer waits for that due time at that visit sequence is stale and fires nothing: an event ended the wait, the instance ended, or a migration moved the token.
     - Task escalation follows, as before.
  3. **Node.** On a Server, the lease holder of each VAE calls the tick every second (DSN-NOD-208), so workflow timers fire there without a new call. Only the holder ticks, so a row fires once. The claim with leases of IF-DVM-034 waits for PH-2 step 11 and is needed only if several workers of one VAE fire timers.
  4. **Design notes.** DSN-DVM-751 and DSN-NOD-227 get phase notes.
- **Consequences:**
  - The 38 workflow vectors pass in memory and on PostgreSQL 16 with timer rows.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
  - No storage migration is needed.
- **Alternatives considered:**
  - A timer table of its own for workflows was rejected: the existing table already has the `(due_at, target, step)` shape that DSN-DVM-751 names, and it is written in the same flush.
  - Removing stale rows when a wait ends was rejected for now: a stale row is skipped when it fires, and removing it would add a write to every event and completion.
- **Affects:** DSN-DVM-751, DSN-NOD-208, DSN-NOD-227, IF-DVM-034, STD-TXN-091, DEC-054, DEC-115, DEC-119, DEC-120

### DEC-122 — Sagas and quarantine in the kernel (PH-2 step 6, part 6d)
- **Date:** 2026-10-31 · **Status:** Accepted
- **Context:** Part 6d of PH-2 step 6 (DEC-119) implements the sagas of STD-TXN-080 and the quarantine of STD-CTX-031 (BPA 1.0-rc8, DSN-DVM-564, DSN-DVM-624), and runs saga coordinators as jobs (DSN-NOD-230). Before the part, their 23 vectors failed. The saga clause of BPA 1.0-rc12, STD-TXN-081 (DSN-DVM-565), extends the same code, so the part takes it too. The owner confirmed the part with the default suggestions.
- **Decision:**
  1. **Classes.** `Saga` (Definition), `Quarantine` and `QuarantineRelease` (System) join the genesis registry with the declarations of the reference model. A commit that writes a saga checks it. A change to a saga's state members outside its execution fails with `FLOW.STATE_DIRECT_WRITE`.
  2. **Sagas.** `crates/ubos-dvm/src/sagas.rs` follows the reference model.
     - **Commits.** Every state transition is one kernel commit, made while a thread-local flag allows the state members.
     - **Steps.** A step's process is a `job` linked by `saga` and `saga_step`. Its outcome is kept in `kernel_meta`, so a resumed saga repeats no step whose process committed.
     - **Simulated crash.** `interrupt_after` stops the run after that many commits.
  3. **Timeouts and intervention (1.0-rc12).**
     - **Timeouts.** A forward attempt of a step with a timeout records `started_at`, and a resumed step past its timeout times out with `FLOW.TIMEOUT`. With `on_timeout: escalate`, a timeout or a limit termination stops the saga `stuck` with the effect `SagaStuck`.
     - **Operations.** `sagas` lists sagas by state, age and step. `saga_action` lets the saga's operators retry, skip or cancel, then continues the saga.
  4. **Quarantine.** The policy, the failure counts in the window and the quarantined versions are kept in `kernel_meta`.
     - **Counting.** A failure or limit termination of a process counts towards its entry asset version; replays and simulated re-runs do not count. Reaching the threshold commits a `Quarantine` with the effect `AssetQuarantined`.
     - **Fail fast.** Resolving a quarantined version fails with `LOGIC.ASSET_QUARANTINED`.
     - **Release.** A releaser's `release_quarantine` commits a `QuarantineRelease` and restarts the counts.
  5. **Jobs.** The node runs jobs of kind `saga`: the job calls `run_saga`, which resumes from the committed state after a lost lease. Operators may enqueue saga jobs through the administration interface.
  6. **Claim.** The conformance test claims STD-TXN-080, STD-TXN-081 and STD-CTX-031.
  7. **Part 6e.** It keeps the workflow clauses of 1.0-rc12: STD-TXN-093 and -094.
  8. **Design notes.** DSN-DVM-564, -565, -624 and DSN-NOD-230 get phase notes.
- **Consequences:**
  - The 40 vectors of the three clauses pass in memory and on PostgreSQL 16, and the conformance test passes.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
- **Alternatives considered:**
  - A System-kind `Saga` written only by the kernel was rejected, as in DEC-097: the author writes the steps, and only the state members are reserved.
  - Counting quarantine failures in the process records was rejected: the window needs only the recent failures per version, which `kernel_meta` holds directly.
- **Affects:** STD-TXN-080, STD-TXN-081, STD-CTX-031, DSN-DVM-564, DSN-DVM-565, DSN-DVM-624, DSN-NOD-230, DEC-097, DEC-119

### DEC-123 — Workflow tasks, approvals, calendars and timeouts in the kernel (PH-2 step 6, part 6e)
- **Date:** 2026-10-31 · **Status:** Accepted
- **Context:** Part 6e of PH-2 step 6 (DEC-119, as adjusted by DEC-122) implements the workflow clauses of BPA 1.0-rc12 in the kernel: STD-TXN-093 and -094. With them come the approvals and human tasks inside workflow steps and the escalation by workflow timers that DEC-111 moved to step 6. Before the part, 26 of their 34 vectors failed; the 8 declaration checks already passed (DEC-120). The owner confirmed the part with the default suggestions.
- **Decision:**
  1. **Tasks.** A `human_task` step with a `queue` creates a `Task` through the kernel's task creation, with the ID derived from the instance, token, step and visit, and the member `workflow`. `Task` gains the member `workflow` in the genesis registry.
     - **Completion.** The `task` operation that completes or cancels such a task continues the instance in the same operation. A completion sets the step's result from the outcome; a cancellation follows `on_cancel` or fails the instance.
  2. **Approvals.** An `action` step with `defer` calls its port with `defer`. When a pending change is stored, the token waits for it.
     - **Decisions.** `approve` continues the instance after it applies or rejects the change, or finds it invalidated.
     - **Expiry.** The expiry is a timer row.
  3. **Timers.** A `timer` with `at` evaluates its expression on entry, with the entry instant as the context's `now` and the step's `calendars`. An error ends the instance `failed`.
     - **Timeouts.** `wait_event` and `human_task` steps with `timeout` record `timeout_at` and the entry instant.
     - **Rows.** The timer rows of DEC-121 now mark what is due: `wf:` for a timer, `wfto:` for a timeout, `wfpc:` for the expiry of a pending change.
     - **Firing.** The tick fires them in the order of `(due, branch, instance, token, kind)`. A timeout cancels the step's task with the reason `timeout` and continues to `on_timeout`.
  4. **Migration.** A migration recomputes the due times of waiting tokens from their entry instants under the new version of their steps.
  5. **Claim.** The conformance test claims STD-TXN-093 and -094. Every clause of BPA 1.0-rc12 is now implemented except `ask.input` (STD-ISA-101, part 6f) and the pre-commit connector calls of BPA 1.0-rc8 (step 12).
  6. **Design notes.** DSN-DVM-744 and -751 get phase notes.
- **Consequences:**
  - The 34 vectors pass in memory and on PostgreSQL 16.
  - The workflow, saga, approval and work vectors of the earlier parts still pass, and the conformance test passes.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
- **Alternatives considered:**
  - Scanning pending changes at each tick for expiries was rejected: the expiry is known when the token begins to wait, so it is a timer row like the others.
- **Affects:** STD-TXN-093, STD-TXN-094, STD-LGS-061, DSN-DVM-743, DSN-DVM-744, DSN-DVM-751, DEC-110, DEC-111, DEC-119, DEC-121, DEC-122

### DEC-124 — Actions that ask for input in the kernel (PH-2 step 6, part 6f-1)
- **Date:** 2026-10-31 · **Status:** Accepted
- **Context:** The owner split part 6f of PH-2 step 6 (DEC-119) in two. Part 6f-1 implements `ask.input` of BPA 1.0-rc12 (STD-ISA-101, DSN-DVM-735) in the kernel. Part 6f-2 takes the post-commit and replay work of the node. Before the part, the 16 vectors of STD-ISA-101 failed because the instruction was not available. The owner confirmed the split and the part with the default suggestions.
- **Decision:**
  1. **Instruction.** `ask.input` joins the instructions the runtime implements. It runs through the journaling helper of the host instructions, so its result, the validated answers, is the journal record a replay reads (STD-CTX-012).
  2. **Requests.** `crates/ubos-dvm/src/dialogs.rs` keeps each request in `kernel_meta` under the content ID of the invocation (without `process`, `answers` and `max_attempts`) and the request name.
     - **Lookup.** A repeated call finds the same request and expiry; an expired or consumed request starts again.
     - **Context.** The invocation being run is held on a thread-local stack with a guard, so every exit of the run leaves it.
  3. **Outcome.** A request without answers ends the run with the outcome `needs_input` and the request. The process record ends `aborted`, and nothing is committed or released.
     - **Answers.** Answers are checked: a missing required field, a value of another type or an unknown field fails with `MODEL.INVALID_VALUE`. A request never made or already consumed fails with `IAM.NOT_FOUND`, and late answers with `FLOW.INPUT_EXPIRED`.
     - **Consumption.** A process that commits consumes the requests it answered.
  4. **Claim.** The conformance test claims STD-ISA-101. Every clause of BPA 1.0-rc12 is implemented in the kernel, except the pre-commit connector calls of BPA 1.0-rc8 (STD-TXN-051, STD-ISA-090), which stay in step 12 (DEC-119).
  5. **Design note.** DSN-DVM-735 gets a phase note.
- **Consequences:**
  - The 16 vectors and the instruction catalogue of 1.0-rc12 (VER-CONF-3005) pass in memory and on PostgreSQL 16, and the conformance test passes.
  - The CI suites, the cargo tests, fmt, clippy, the dependency check and the SDK registry check pass.
- **Alternatives considered:**
  - Keeping a suspended process between the calls was rejected, as in DEC-119: re-running the deterministic process with the answers commits in one step and holds nothing open.
- **Affects:** STD-ISA-101, STD-ISA-001, STD-CTX-012, DSN-DVM-735, FR-FLOW-071, DEC-119
