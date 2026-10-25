---
id: UBS-SYS-DVM-02-05
title: DVM — Rules and Flow Design
status: draft
phase: PH-1
depends_on: [UBS-SYS-DVM-02-04, UBS-STD-13]
---

# DVM — Rules and Flow Design (DSN-DVM-7xx)

This file designs the crates `ubos-rules` and `ubos-flow`. Reading order:
1. governance sheets (701–710);
2. decisions and decision tables (720–726);
3. lifecycles, ports and guards (730–735);
4. approvals, tasks and delegation (740–746);
5. workflows and timers (750–754);
6. simulation and rule tests (760–764).

## Governance sheets

### DSN-DVM-701 — Sheet parser and compiler
- **Statement:** `ubos-rules` MUST parse the textual grammar (STD-LGS-001) into the sheet object (STD-LGS-002) and compile each rule into `(selector matcher, specificity, declarations)`. Unknown properties MUST fail with `RULE.UNKNOWN_PROPERTY`. Compiled sheets MUST be cached by sheet version hash.
- **Rationale:** FR-RULE-011, FR-RULE-015.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given the sheet grammar vectors, when parsed and printed, then the round trip is byte-identical.
- **Verification:** CONF
- **Origin:** FR-RULE-011, FR-RULE-012, FR-RULE-015, STD-LGS-001, STD-LGS-002

### DSN-DVM-702 — Property registry
- **Statement:** The property registry (STD-LGS-010) MUST combine kernel properties (STD-LGS-011) with Buk-defined properties, each with type, inheritance behaviour, default and allowed combination. Buk properties MUST be namespaced.
- **Rationale:** FR-RULE-013, FR-RULE-014.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a Buk property with the same name as a kernel property, when installed, then it is namespaced and does not shadow the kernel property.
- **Verification:** CONF
- **Origin:** FR-RULE-013, FR-RULE-014, STD-LGS-010, STD-LGS-011

### DSN-DVM-703 — Selector index
- **Statement:** For each `(model hash, sheet set hash)` the engine MUST build a selector index: rules bucketed by the most specific class in the selector, then by attribute predicates. Evaluating the effective declarations of an object MUST visit only buckets for its class linearization.
- **Rationale:** FR-RULE-024, NR-PERF-009.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given 5,000 rules and a 5-level class, when evaluating, then latency meets NR-PERF-009.
- **Verification:** BENCH
- **Origin:** FR-RULE-024, NR-PERF-009, STD-LGS-020

### DSN-DVM-704 — Cascade
- **Statement:** The cascade MUST sort matching declarations per property by the key `(locked, layer, specificity (d, a, s, x), priority, position)` (STD-LGS-021, STD-LGS-022). Ties at every level MUST raise `RULE.AMBIGUOUS_CASCADE` at sheet commit time where statically detectable, else at evaluation.
- **Rationale:** FR-RULE-021.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given the cascade vectors, when evaluated, then winners equal the vectors.
- **Verification:** CONF, PROP
- **Origin:** FR-RULE-021, STD-LGS-021, STD-LGS-022

### DSN-DVM-705 — Locked declarations
- **Statement:** A declaration marked locked in a higher layer MUST win over any lower layer. A lower-layer attempt to override it MUST be rejected at sheet commit time with `RULE.LOCKED_OVERRIDE` (or `TEN.OVERRIDE_LOCKED` for tenant layers).
- **Rationale:** FR-RULE-023.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a platform-locked retention rule, when a tenant sheet sets a shorter retention, then the commit fails.
- **Verification:** CONF, SEC
- **Origin:** FR-RULE-023

### DSN-DVM-706 — Explanation
- **Statement:** `read.explain` MUST return a `CascadeExplanation` (STD-LGS-023): for each property, the winning declaration with sheet, rule, layer and specificity, and the losing candidates with the reason they lost.
- **Rationale:** FR-RULE-022.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given a property set in three sheets, when explained, then all three appear with reasons.
- **Verification:** CONF
- **Origin:** FR-RULE-022, STD-LGS-023, STD-ISA-035

### DSN-DVM-707 — Validations and invariants
- **Statement:** Validations (per object) and invariants (cross-object) declared in sheets (STD-LGS-030) MUST be evaluated in the flush with severities error, warning and info. Errors abort; warnings and info are attached to the result. All violations MUST be reported at once (FR-RULE-034).
- **Rationale:** FR-RULE-031, FR-RULE-032, FR-RULE-033.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given an object with two errors and one warning, when staged, then the failure lists two errors and one warning.
- **Verification:** CONF
- **Origin:** FR-RULE-031, FR-RULE-032, FR-RULE-033, FR-RULE-034, STD-LGS-030

### DSN-DVM-708 — Invariant dependency tracking
- **Statement:** Cross-object invariants MUST declare or have inferred the classes and fields they read. The flush MUST evaluate an invariant only when the batch touched one of them.
- **Rationale:** Invariants scale with change size, not model size.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given 200 invariants and a commit touching one class, when flushed, then only invariants reading that class are evaluated.
- **Verification:** BENCH, CONF
- **Origin:** FR-RULE-032, NR-PERF-009

### DSN-DVM-709 — Validation preview
- **Statement:** The host API MUST allow validating staged changes without flushing (dry run), returning the same violations the flush would.
- **Rationale:** FR-RULE-035.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a form with errors, when previewed, then violations match those of a real submit.
- **Verification:** CONF
- **Origin:** FR-RULE-035

### DSN-DVM-710 — Sheet validity and attachment
- **Statement:** Sheets MUST have valid-time periods (FR-RULE-061); attachments MAY be time-boxed (FR-RULE-062) and scoped to tenant, VAE, branch or class (FR-RULE-063). Evaluation MUST select sheets effective at the process's `asof`.
- **Rationale:** Rules change on dates, not on deployment.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a new fee rule effective 1 July, when a June-dated process evaluates, then the old rule applies.
- **Verification:** CONF
- **Origin:** FR-RULE-061, FR-RULE-062, FR-RULE-063

## Decisions and decision tables

### DSN-DVM-720 — Action decisions
- **Statement:** Before every mutating instruction and port call, the runtime MUST evaluate the applicable action decisions (STD-LGS-031) with outcomes allow, deny (`RULE.DECISION_DENIED`) or requires (`RULE.DECISION_REQUIRES`, leading to a deferred commit). Deny from any applicable decision MUST veto (FR-RULE-052).
- **Rationale:** FR-RULE-051, FR-RULE-052, FR-RULE-053.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given two decisions, one allow and one deny, when an action is attempted, then it is denied.
  2. Given an agent principal, when the action is attempted, then the same decisions apply.
- **Verification:** CONF, SEC
- **Origin:** FR-RULE-051, FR-RULE-052, FR-RULE-053, STD-LGS-031

### DSN-DVM-721 — Decision results
- **Statement:** Every decision evaluation MUST produce a `DecisionResult` with outcome, reasons, inputs hash and the rule binding, recorded in the journal.
- **Rationale:** Explainable decisions.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given a denied action, when the journal is read, then the result lists the denying rule.
- **Verification:** CONF
- **Origin:** STD-LGS-031, FR-FLOW-032

### DSN-DVM-722 — Decision table compiler
- **Statement:** Decision tables (STD-LGS-040) MUST be compiled into a decision tree over input columns with hit policies unique, first, priority, collect. At commit time the compiler MUST check completeness and overlap (FR-RULE-042) and report gaps as counter-example input tuples.
- **Rationale:** FR-RULE-041, FR-RULE-042.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given a table missing a range, when committed, then a gap with a concrete input tuple is reported.
- **Verification:** CONF, PROP
- **Origin:** FR-RULE-041, FR-RULE-042, STD-LGS-040

### DSN-DVM-723 — No-match and multiple-match
- **Statement:** Evaluation MUST raise `RULE.NO_MATCH` when no row matches and no default exists, and `RULE.MULTIPLE_MATCH` for a unique table with several matches.
- **Rationale:** Explicit failure instead of silent defaults.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given an unmatched input, when evaluated, then `RULE.NO_MATCH` includes the input values.
- **Verification:** CONF
- **Origin:** STD-LGS-040

### DSN-DVM-724 — DMN interchange
- **Statement:** Decision tables MUST be importable from and exportable to DMN 1.4 decision tables for the supported subset; unsupported constructs MUST be reported, not dropped.
- **Rationale:** FR-RULE-044.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, BRG
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given a DMN table in the subset, when imported and exported, then it is semantically equivalent.
- **Verification:** CONF
- **Origin:** FR-RULE-044

### DSN-DVM-725 — Declaration requirements
- **Statement:** Required declarations (for example "every fund must have a valuation policy") MUST be checked at commit time and raise `RULE.DECLARATION_UNSATISFIED`.
- **Rationale:** Configuration completeness.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-ImplementationConsultant
- **Acceptance:**
  1. Given a fund without a valuation policy, when activated, then the commit fails naming the property.
- **Verification:** CONF
- **Origin:** FR-RULE-013

### DSN-DVM-726 — Typed property access
- **Statement:** Logic MUST access effective declarations through typed accessors; untyped access MUST fail with `RULE.UNTYPED_ACCESS`.
- **Rationale:** Type safety across sheets and logic.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-LogicDeveloper
- **Acceptance:**
  1. Given a script reading a property by dynamic name, when analysed, then it is reported.
- **Verification:** CONF
- **Origin:** FR-RULE-013

## Lifecycles, ports and guards

### DSN-DVM-730 — Lifecycle engine
- **Statement:** Lifecycles (FR-FLOW-011) MUST be compiled into a transition table per class, inherited and narrowed through the linearization (FR-FLOW-015). The lifecycle state field MUST be writable only by `commit.transition` (`FLOW.STATE_DIRECT_WRITE`).
- **Rationale:** FR-FLOW-011, FR-FLOW-012.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given an update setting the state field directly, when staged, then it fails.
- **Verification:** CONF
- **Origin:** FR-FLOW-011, FR-FLOW-012, FR-FLOW-015, STD-ISA-017

### DSN-DVM-731 — Transitions and guards
- **Statement:** `commit.transition` MUST check that the transition exists (`FLOW.TRANSITION_NOT_ALLOWED`), evaluate guards (`FLOW.GUARD_FAILED` with explanations), run exit effects, change the state, and run entry effects, all in the same process. A guard failure MUST leave no partial effect (FR-FLOW-033).
- **Rationale:** FR-FLOW-013, FR-FLOW-031, FR-FLOW-033.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given a guard that fails, when the transition is attempted, then the error explains the failing condition, and no effect is staged.
- **Verification:** CONF
- **Origin:** FR-FLOW-013, FR-FLOW-031, FR-FLOW-032, FR-FLOW-033

### DSN-DVM-732 — Available actions
- **Statement:** The DVM MUST compute, for an object and principal, the list of available ports and transitions with, for unavailable ones, the reasons (guard, decision, permission).
- **Rationale:** FR-FLOW-024.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM, WSP
- **Personas:** PER-BusinessUser
- **Acceptance:**
  1. Given an order in draft, when asked, then "submit" is available and "ship" is unavailable with the reason.
- **Verification:** CONF
- **Origin:** FR-FLOW-024

### DSN-DVM-733 — Uniform and batch invocation
- **Statement:** Ports MUST be invocable with the same semantics from UI, API, SDK, flows and agents (FR-FLOW-022), singly or in batch (FR-FLOW-023). A batch MUST report per-item outcomes and MAY be atomic or independent as requested.
- **Rationale:** One behaviour for every channel.
- **Priority:** Must · **Phase:** PH-1 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given a batch of 100 with 3 invalid items in independent mode, when invoked, then 97 succeed, and 3 errors are reported.
- **Verification:** CONF
- **Origin:** FR-FLOW-021, FR-FLOW-022, FR-FLOW-023

### DSN-DVM-734 — Lifecycle evolution
- **Statement:** A lifecycle change that removes a state MUST declare a mapping for objects in that state; it MUST be applied by a migration process.
- **Rationale:** FR-FLOW-014.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given a removed state with 50 objects in it and no mapping, when committed, then it fails.
- **Verification:** CONF
- **Origin:** FR-FLOW-014

### DSN-DVM-735 — Actions that ask for input
- **Statement:** A port MAY return a "needs input" result with a typed form request (FR-FLOW-071); the caller resubmits with the answers. For agents, the same request MUST be exposed as a structured dialogue (FR-FLOW-072).
- **Rationale:** Interactive actions without UI-specific logic.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessUser, PER-AiAgent
- **Acceptance:**
  1. Given a port requiring a reason, when invoked without it, then a typed request for the reason is returned, and nothing is staged.
- **Verification:** CONF
- **Origin:** FR-FLOW-071, FR-FLOW-072

## Approvals, tasks and delegation

### DSN-DVM-740 — Approval requirements
- **Statement:** Approval requirements (FR-FLOW-041) MUST be expressed as decisions returning "requires" with an approval spec (roles, count, order, segregation-of-duties constraints). One approval mechanism MUST serve change sets, pending changes and workflow steps (FR-FLOW-046).
- **Rationale:** FR-FLOW-041, FR-FLOW-046.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a two-approver rule, when one approval is given, then the change remains pending.
- **Verification:** CONF
- **Origin:** FR-FLOW-041, FR-FLOW-042, FR-FLOW-046

### DSN-DVM-741 — Maker–checker and SoD
- **Statement:** The approval engine MUST reject approvals by the maker or by principals violating the SoD matrix (FR-FLOW-043), including through delegation chains and agents acting for the maker.
- **Rationale:** FR-FLOW-043, FR-AI-025.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given an agent acting for the maker, when it approves, then the approval is rejected.
- **Verification:** SEC, CONF
- **Origin:** FR-FLOW-043, FR-AI-025, CR-SOX-002

### DSN-DVM-742 — Approval evidence
- **Statement:** Approvals MUST be stored as `Approval` objects with approver, time, preview hash, comment and optional signature (FR-FLOW-044), and MUST become stale when the preview hash changes (FR-FLOW-045).
- **Rationale:** FR-FLOW-044, FR-FLOW-045.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-InternalAuditor
- **Acceptance:**
  1. Given an approval followed by a change, when merge is attempted, then it is refused as stale.
- **Verification:** CONF
- **Origin:** FR-FLOW-044, FR-FLOW-045

### DSN-DVM-743 — Tasks
- **Statement:** Tasks (FR-FLOW-051) MUST be System-kind objects with assignee rules (FR-FLOW-052), and claim, release and reassign operations with optimistic concurrency (FR-FLOW-053). The inbox (FR-FLOW-054) MUST be a saved query over tasks filtered by the principal.
- **Rationale:** Human work as data.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given two users claiming one task concurrently, when committed, then exactly one owns it.
- **Verification:** SIM, CONF
- **Origin:** FR-FLOW-051, FR-FLOW-052, FR-FLOW-053, FR-FLOW-054
- **Phase note:** DEC-109: built in PH-2 step 4e as BPA 1.0-rc10 states it (STD-LGS-061, -062): the inbox is the adapter operation `inbox`, computed over the tasks, queues, delegations and pending changes of the branch rather than a saved query; a saved query over tasks follows with the saved queries of PH-2 step 13.

### DSN-DVM-744 — Due dates and escalation
- **Statement:** Tasks MUST get due dates from due-date rules evaluated on a business calendar (FR-FLOW-081). Escalation chains (FR-FLOW-082) MUST be driven by timers (DSN-DVM-751).
- **Rationale:** FR-FLOW-081, FR-FLOW-082, FR-FLOW-083.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a 2-business-day rule on Friday, when computed, then the due date is Tuesday.
- **Verification:** CONF
- **Origin:** FR-FLOW-081, FR-FLOW-082, FR-FLOW-083
- **Phase note:** DEC-110: built in PH-2 steps 4e and 4f as BPA 1.0-rc10 states it (STD-LGS-063): due rules on the queue's calendars at creation, escalation steps applied when the clock reaches them (the kernel's tick) rather than by workflow timers, which come with PH-2 step 6.

### DSN-DVM-745 — Delegation
- **Statement:** Delegation objects (FR-FLOW-091) MUST have scope, validity and a delegate; the context MUST carry the delegation chain; expired delegations MUST fail with `IAM.DELEGATION_EXPIRED`. Delegation MUST NOT exceed the delegator's rights (FR-FLOW-092).
- **Rationale:** FR-FLOW-091, FR-FLOW-092, FR-FLOW-093.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-Approver
- **Acceptance:**
  1. Given a delegation that ended yesterday, when used, then `IAM.DELEGATION_EXPIRED` is returned.
- **Verification:** CONF, SEC
- **Origin:** FR-FLOW-091, FR-FLOW-092, FR-FLOW-093
- **Phase note:** DEC-108: built in PH-2 step 4d as BPA 1.0-rc10 states it (STD-LGS-060): `Delegation` System objects, the roles of the adapter's directory, approvals through a delegation; the chain carried by an action is one principal (`on_behalf_of`). Tasks through a delegation follow with the tasks of step 4e.

### DSN-DVM-746 — Approval formal model
- **Statement:** The approval and deferred-commit protocol MUST be modelled formally with properties: no commit without valid approvals; stale approvals never count; SoD always holds.
- **Rationale:** Governance guarantees are proven.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-KernelEngineer
- **Acceptance:**
  1. Given the model with 2 approvers and concurrent edits, when checked, then no property is violated.
- **Verification:** FORM
- **Origin:** FR-FLOW-043, FR-FLOW-045

## Workflows and timers

### DSN-DVM-750 — Durable workflow instances
- **Statement:** Workflow definitions (FR-FLOW-061) MUST compile into a state graph; instances (FR-FLOW-062) MUST be System-kind objects whose state advances only through processes. Each step MUST be idempotent by `(instance, step, attempt)`.
- **Rationale:** Workflows survive restarts and are auditable.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a node crash mid-workflow, when restarted, then the instance continues from its last committed step.
- **Verification:** FAULT
- **Origin:** FR-FLOW-061, FR-FLOW-062

### DSN-DVM-751 — Timers
- **Statement:** Timers MUST be stored as rows `(due_at, instance, step)` written in the flush; the host scheduler claims due timers and starts processes. Business-calendar timers (FR-FLOW-063) MUST be resolved to instants at creation and recomputed if the calendar changes.
- **Rationale:** FR-FLOW-063.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, NOD
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given a holiday added after timer creation, when the calendar commits, then affected timers move.
- **Verification:** CONF
- **Origin:** FR-FLOW-063

### DSN-DVM-752 — Event correlation
- **Statement:** Waiting workflow steps MUST register correlation keys; incoming events MUST be matched by `(event type, correlation key)` through an index and deliver to at most one waiting step per key.
- **Rationale:** FR-FLOW-064.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-IsvDeveloper
- **Acceptance:**
  1. Given 10,000 waiting instances, when one matching event arrives, then exactly one resumes.
- **Verification:** CONF, BENCH
- **Origin:** FR-FLOW-064

### DSN-DVM-753 — Workflow versioning
- **Statement:** Instances MUST stay pinned to the definition version they started with, unless an explicit migration maps them to a new version.
- **Rationale:** Running work is not broken by changes.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessArchitect
- **Acceptance:**
  1. Given an instance on v1, when v2 is published, then it completes on v1.
- **Verification:** CONF
- **Origin:** FR-FLOW-061, STD-CTX-030

### DSN-DVM-754 — SLA reporting
- **Statement:** The DVM MUST derive SLA metrics (time in state, overdue counts) from lifecycle and task history through projections.
- **Rationale:** FR-FLOW-083.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-FundOperationsManager
- **Acceptance:**
  1. Given 1,000 completed tasks, when the SLA projection is read, then it equals a recomputation from history.
- **Verification:** PROP
- **Origin:** FR-FLOW-083
- **Phase note:** DEC-110: PH-2 step 4f builds the service-level report of BPA 1.0-rc10 (STD-LGS-064) as the operation `sla_report`, computed from the tasks at the head of the branch. Time in state and a maintained SLA projection over lifecycle history follow with the reports of PH-2 step 13.

## Simulation and rule tests

### DSN-DVM-760 — Simulation branches
- **Statement:** Simulations MUST run on simulation branches in the simulation profile: effects to the outbox are captured but never released, connectors are mocked (DSN-DVM-653), and the branch is discarded or kept as evidence.
- **Rationale:** What-if without risk.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given a simulation that emits notifications, when completed, then no notification is delivered.
- **Verification:** CONF
- **Origin:** FR-RULE-071, STD-CTX-004

### DSN-DVM-761 — Historical rule simulation
- **Statement:** Rule-change simulation (FR-RULE-071) MUST replay a set of historical processes (selected by query and period) from their journals with the candidate sheets substituted, and report changed outcomes per process with diffs.
- **Rationale:** See the effect of a rule change on real history.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a threshold change and 10,000 historical approvals, when simulated, then the processes whose outcome changes are listed.
- **Verification:** CONF, BENCH
- **Origin:** FR-RULE-071, FR-RULE-072, NR-PERF-017

### DSN-DVM-762 — Simulation budgets and evidence
- **Statement:** Simulations MUST run within a budget (processes, CPU, time) (FR-RULE-072) and produce a stored run record with inputs, budget used and results (FR-RULE-073).
- **Rationale:** FR-RULE-072, FR-RULE-073.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM
- **Personas:** PER-ComplianceOfficer
- **Acceptance:**
  1. Given a budget exceeded mid-run, when stopped, then a partial record states coverage.
- **Verification:** CONF
- **Origin:** FR-RULE-072, FR-RULE-073

### DSN-DVM-763 — Live impact count
- **Statement:** While editing a rule, the DVM MUST return the number of current objects whose effective declaration would change, computed from the selector index over the current head.
- **Rationale:** FR-RULE-074.
- **Priority:** Should · **Phase:** PH-2 · **Systems:** DVM, STU
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given a selector change, when counted, then the count equals a full evaluation.
- **Verification:** PROP
- **Origin:** FR-RULE-074

### DSN-DVM-764 — Rule tests and coverage
- **Statement:** Sheets and decision tables MAY carry test cases (FR-RULE-081). Merging rule changes to protected branches MUST require all tests to pass (`RULE.TESTS_FAILED`). The engine MUST report rule coverage (rules and table rows hit by tests, FR-RULE-083).
- **Rationale:** FR-RULE-081, FR-RULE-082, FR-RULE-083.
- **Priority:** Must · **Phase:** PH-2 · **Systems:** DVM, FRG
- **Personas:** PER-BusinessAnalyst
- **Acceptance:**
  1. Given a failing rule test, when merge is attempted, then it is refused listing the test.
- **Verification:** CONF
- **Origin:** FR-RULE-081, FR-RULE-082, FR-RULE-083
