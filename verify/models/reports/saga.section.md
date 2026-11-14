<!-- Draft of the report section for reports/README.md and of the row for ../README.md
     (PH-2 step 2b). The coordinator integrates it; nothing else was edited. -->

## 7. PH-2 step 2b — sagas (EXIT-2-02)

| Item | Value |
|---|---|
| Date | 2026-11-14 |
| Tool | TLC 2.19 (`tla2tools.jar` release 1.7.4), as in §1 |
| Workstation | 4 vCPU, 15 GB RAM, Java 21, `-XX:+UseParallelGC`, 2 workers (another model checker ran on the other two CPUs) |
| Model | `saga.tla` (safety), `saga_live.tla` (the same model with the liveness configurations), VER-FORM-0101, DSN-DVM-566 |
| Verdict | **no property violated** in any run below. The invariants completed exhaustively at the stated bound; the liveness properties completed exhaustively at two reduced bounds and were simulated at the stated bound (DEC-031) |

### 7.1 Runs

Stated bound (`saga.cfg`): 3 steps × 2 coordinators, 2 coordinator crashes, 1 lost
invocation, 2 attempts per step, every declaration of the saga (recovery `backward` or
`forward` × timeout action `compensate` or `escalate` per step × compensation present or
absent per step: 72 declarations), compensations may fail.

| Run | Bound | Distinct states | Depth | Time | Evidence |
|---|---|---|---|---|---|
| Safety, exhaustive at the stated bound (`saga.cfg`, `check.sh full saga`) | as stated | @FULL_DISTINCT@ (@FULL_GEN@ generated) | @FULL_DEPTH@ | @FULL_TIME@ | `saga.exhaustive.txt` |
| Liveness, exhaustive (`saga_live.cfg`, `check.sh full saga_live`) | stated bound with **1 crash** and **1 attempt** | @LIVE_DISTINCT@ | @LIVE_DEPTH@ | @LIVE_TIME@ | `saga_live.exhaustive.txt` |
| Liveness, exhaustive (`saga_live_c1.cfg`) | stated bound with **1 coordinator** (2 crashes, 2 attempts) | @LC1_DISTINCT@ | @LC1_DEPTH@ | @LC1_TIME@ | `saga_live.exhaustive.txt` |
| Liveness, simulation at the stated bound (`saga_sim.cfg`) | as stated; 1,000,000 behaviours per worker requested, depth ≤ 100 | @SIM_STATES@ states checked | every behaviour runs to its end (about 24 states) | @SIM_TIME@ | `saga.simulation.txt` |
| CI, safety (`saga.ci.cfg`) | stated bound with 1 crash and 1 attempt | @CI_DISTINCT@ | @CI_DEPTH@ | @CI_TIME@ | — |
| CI, liveness (`saga_live.ci.cfg`) | 2 coordinators, 1 crash, 1 lost invocation, backward recovery, timeout action `compensate` or `escalate`, every step with a compensation that does not fail (8 declarations) | @CIL_DISTINCT@ | @CIL_DEPTH@ | @CIL_TIME@ | — |

Bound that did not complete: the liveness properties at the stated bound. A run of an
earlier variant of the model (128 declarations, of which 56 were redundant ones under
forward recovery; one fairness condition per key) was stopped after 60 min with 11,439,904
distinct states found, 2,343,559 states left on the queue and depth 29, no violation found
before stopping. TLC checks liveness at about 150,000 distinct states per minute on this
machine, against about 2,000,000 for invariants; the stated bound has 16.7 million states.
Rule 2 of DEC-031 is therefore applied to the liveness properties only.

Symmetry reduction is not used (the steps are ordered, and the coordinators appear in
liveness runs). The declaration of the saga is chosen in the initial state, so one run
covers all declarations. Two reductions of the declaration space are exact: a step without
a timeout is covered by a step that declares one (the timeout may pass at any moment or
never), and under forward recovery no compensation runs, so one compensation declaration
stands for all eight.

### 7.2 Properties

Invariants of `saga.cfg` and `saga.ci.cfg`:

| Invariant | Meaning |
|---|---|
| `TypeOK` | types |
| `ForwardAtMostOnce` | the forward effect of a step is committed at most once, over all attempts, repeats and coordinators |
| `CompensateAtMostOnce` | the compensation of a step is committed at most once (DSN-DVM-566) |
| `CompensationReverseOrder` | compensations are committed in reverse step order and none is passed over (FR-TXN-063) |
| `NoCompensationOfUnappliedStep` | only a step whose forward effect is committed is compensated |
| `NoForwardAfterCompensationStarts` | no forward effect is committed once the saga object has left `running` (compensating or ended) |
| `CompensationOnlyWhileCompensating` | compensations are committed only while the saga object is `compensating` |
| `StateMatchesEffects` | the lists `completed` and `compensated` of the saga object match the committed effects; the only gap is the current step between the commit of its process and the commit that records it (FR-TXN-062) |
| `CompletedAllAppliedOnce` | a `completed` saga has applied every step exactly once, in order, and compensated none |
| `CompensatedNetEmpty` | a `compensated` saga has no net effect: every applied step that declares a compensation is compensated exactly once |
| `TerminalStable` | a saga object in a terminal state is never changed |
| `EndJustified` | `compensated` only under backward recovery; `escalated` only by a step that declares it and whose attempt is recorded as timed out; `failed` only by a recorded failed compensation or by forward recovery whose every attempt is recorded as failed or timed out |

Temporal properties of `saga_live.cfg`, `saga_live_c1.cfg`, `saga_live.ci.cfg` and
`saga_sim.cfg` on `FairSpec` (weak fairness of each coordinator's work and invocations and
of the execution of invocations in flight; crashes, lost invocations and timeouts are not
fair):

| Property | Meaning |
|---|---|
| `Terminates` | every saga ends: `completed`, `compensated`, `failed` or `escalated` |
| `EndsCompletedOrCompensated` | DSN-DVM-566: under backward recovery without escalation every saga ends `completed` or `compensated`, unless a compensation fails (in `saga_live.ci.cfg` compensations do not fail, so the property is the statement of DSN-DVM-566 as written) |
| `EndsAsDeclared` | under backward recovery every saga ends `completed`, `compensated`, or `escalated` by a step that declares escalation, unless a compensation fails |

### 7.3 Sensitivity of the properties (mutation checks)

Each mutation is selected by the constant `Mutation` and checked against its target
property alone; the property must fail. Bounds: `saga_live.ci.cfg` (2 coordinators, 1 crash,
backward recovery, 8 declarations), with the invariant in place of the temporal properties;
`comp_error_skipped` with `CompCanFail = TRUE`.

| Mutation | Meaning | Property that failed (length of the counterexample) |
|---|---|---|
@MUTATIONS@

All ten mutations were detected, and every property of §7.2 except `TypeOK` is the target
of at least one. Command (per mutation `M` and property `P`): copy `saga_live.ci.cfg`, set
`Mutation = "M"`, `SPECIFICATION Spec` and `INVARIANTS P` (or keep `FairSpec` and set
`PROPERTIES P`), and run TLC on `saga_live.tla`.

### 7.4 Findings

The findings carry the prefix `S-`. None is a violation of a checked property in the
design as modelled; S-1, S-2 and S-5 are rules the model needs and the clauses do not
state (without them a property fails, see the mutations), S-3, S-4 and S-6 are wording.

| ID | Items | Finding | Proposed fix |
|---|---|---|---|
| S-1 | DSN-DVM-565, STD-TXN-080 (note *Timeouts*), DSN-DVM-564 | A timeout decided by the coordinator alone is unsafe when the step can still commit. STD-TXN-080 defines a timeout as a limit termination of the step's process: the process has ended and cannot commit later, which is sound but covers only a process the same kernel terminates. DSN-DVM-564 puts steps on other branches or VAEs, and DSN-DVM-565 speaks of "a step that never answers": there the coordinator's timer passes while the process may still commit. Counterexample (mutation `no_fence`, 11 states): step 1 is invoked; its timeout passes; the saga goes `compensating` and, nothing being completed, `compensated`; then the process of step 1 commits. The saga is `compensated` with the effect of step 1 in place and a step that is not in `completed` (`CompensatedNetEmpty`, `StateMatchesEffects`, `NoForwardAfterCompensationStarts` fail). | DSN-DVM-565: "On timeout the coordinator MUST first record the outcome `timed_out` under the key `(saga, step, attempt, phase)` of the attempt, by a process on the branch of the step, if no outcome is recorded under it. A step process whose key is taken MUST do nothing. If the step's process committed first, the timeout is void and the saga continues with the recorded outcome. Only then the declared action runs." Add the same sentence to the note *Timeouts* of STD-TXN-080 for steps whose process is not ended by the limit termination. |
| S-2 | STD-TXN-080 (note *Commits*), DSN-DVM-750, DSN-DVM-564, `workflows.py` (`saga_results`) | "A step whose process committed is not run again; its recorded outcome is used" covers committed processes only. A failed attempt must be recorded under the key as well and must not be run again: otherwise an invocation repeated after a crash, or made by a second coordinator, can succeed after the saga has started to compensate on the strength of the failure (mutation `rerun_failed`, 11 states, `NoForwardAfterCompensationStarts`). The reference model does this — `saga_results` keeps `ok` and `error` outcomes per `(saga, step, attempt, phase)` — but in a map in memory that survives the simulated crash; the clause and DSN-DVM-564 do not say where the outcome of a failed process is durable (DSN-DVM-508: an in-flight process leaves nothing). | STD-TXN-080: "The outcome of every step process, committed or failed, is recorded once under `(saga, step, attempt, phase)`; the saga's execution acts only on recorded outcomes, and a repeated invocation under a key that has an outcome does nothing." DSN-DVM-564: the outcome record is the process record of the step on the step's branch (FR-TXN-062), written in the same commit as the effect when the process commits and by a separate commit when it fails; DSN-DVM-750 (`(instance, step, attempt)`) names the phase as part of the key for sagas. |
| S-3 | DSN-DVM-566, VER-FORM-0101, STD-TXN-080 (note *States*) | "Every started saga ends completed or compensated" does not hold for the protocol of STD-TXN-080: a saga also ends `failed` (failed compensation; forward recovery that used `max_attempts`), and with DSN-DVM-565 by escalation. The model checks what holds: `Terminates`, `EndJustified`, `EndsCompletedOrCompensated` and `EndsAsDeclared` (§7.2). | Reword DSN-DVM-566 and the procedure of VER-FORM-0101: "every started saga ends (`completed`, `compensated`, `failed` or escalated); under backward recovery it ends `completed` or `compensated` unless a compensation fails or a step that declares escalation times out; compensations run at most once per step". |
| S-4 | DSN-DVM-565, STD-TXN-080 (`DAT-Saga`), FR-TXN-063 | The timeout declaration has no place in the standard: `DAT-Saga` has no member for a step's timeout or its action and no status for an escalated saga. DSN-DVM-565 says each step MUST have a timeout, FR-TXN-063 says a step MAY declare one. "Compensate or escalate" is not defined under forward recovery, where a failure is retried and never compensated (FR-TXN-063). The model reads `compensate` as "handled as a failure of the step under the saga's recovery" (compensation under backward recovery, the next attempt or `failed` under forward recovery; this is what the reference model does for a limit termination) and `escalate` as a terminal state `escalated` that waits for manual resolution (FR-TXN-064). | A new release candidate (needs a DEC record): `DAT-Saga` steps get `timeout` (duration, optional; a profile default applies, which reconciles MUST and MAY) and `on_timeout: fail (default) \| escalate`; `status` gets `escalated`, with `error.step` and `error.code` naming the timed-out step. DSN-DVM-565: "compensate" becomes "fail (handled per the saga's recovery)". Alternative without a new status: escalation ends the saga `failed` with `error.reason: timeout`; not recommended, because FR-TXN-064 lists stuck and failed sagas separately. |
| S-5 | DSN-DVM-564, `workflows.py` (`_saga_set`) | The design item does not say how two coordinators are kept apart (a restarted node and a slow one that is not dead; PH-2 has several nodes). The reference model commits every transition with the head of the saga object as base. That compare-and-set is sufficient: with it and S-1, S-2 all properties hold for two coordinators acting at once; without it a stale coordinator rewrites a saga that has ended (mutation `no_cas`, `TerminalStable`, 10 states). A lease on the saga (as for jobs, DEC-076) is then an optimisation, not a safety condition. | DSN-DVM-564: "Each state transition is a commit whose base is the head of the saga object; a coordinator whose commit fails with a conflict reads the saga object again. The coordinator derives every decision from the saga object and the recorded outcomes of the step processes, never from its memory." |
| S-6 | DSN-DVM-565, STD-TXN-080 (note *States*), STD-TXN-051 | Compensations. (a) DSN-DVM-565 does not say whether the timeout applies to compensation processes; the model has no timeout for them and re-invokes a compensation until an outcome is recorded. (b) The reference model ends the saga `failed` at the first failure of a compensation, without retry, also when `max_attempts` is greater than 1, whereas STD-TXN-051 retries the compensation of a pre-commit call until it succeeds ("at least once"). (c) "A step without a compensation counts as compensated": `CompensatedNetEmpty` therefore holds only over the steps that declare a compensation — a `compensated` saga can leave the effects of the others in place. | (a) DSN-DVM-565: a compensation that times out is fenced as in S-1 and ends the saga `failed` (or escalated), so that the saga appears in the stuck list. (b) Decide whether `max_attempts` applies to compensations; recommended: yes, a failed compensation is retried under a new attempt up to `max_attempts`, then `failed`. (c) State in the note *States* that `compensated` means "every declared compensation of a completed step has run", and let `validate` warn about a step without a compensation that is followed by another step. |

The model agrees with the reference model (`standard/tools/bpa/workflows.py`,
`_saga_loop`): `Trans` is its loop, branch by branch, with the timeout outcome and the
state `escalated` added (S-4). Without crashes and timeouts a completed saga of N steps
takes 3N + 2 commits — 2N + 2 on the saga object and N step processes — as in the vector
VER-CONF-2225 (8 commits for 2 steps); `completed` keeps the completed steps while
`compensated` lists them in reverse order (VER-CONF-2226).

### 7.5 Scope limits

- One saga. Sagas do not share state in the model; conflicts between the steps of different
  sagas on the same objects are ordinary commit conflicts (`commit_cas`, `group_commit`) and
  end a step process with a failure.
- A step process is one atomic step: its effect and its outcome record are one commit
  (DSN-DVM-508); a failed process records its failure (S-2). The effect of a step is
  abstract (applied or not); what a compensation writes is not modelled, only that it runs.
- The fence of S-1 is part of the model. An implementation without it is not covered by the
  result (it fails, see `no_fence`).
- Time is not modelled: a timeout may pass at any moment while no outcome is recorded.
  Compensations have no timeout (S-6); they fail only where the configuration allows it.
- Bounds of the fault injection: 2 coordinator crashes, 1 lost invocation, 2 attempts, 2
  coordinators. Commits on the saga object fail only by the compare-and-set; storage
  failures are covered by `group_commit`.
- Operator actions of FR-TXN-064 (retry, skip, cancel, resolution of a `failed` or
  `escalated` saga) are not modelled; both states are terminal.
- Liveness holds under weak fairness: a coordinator that is up keeps working, a crashed
  one restarts, an invocation in flight is eventually executed. It is exhaustive at the
  two reduced bounds of §7.1 only.

---

## Row for the table of `verify/models/README.md`

| `saga.tla`, `saga_live.tla` | VER-FORM-0101 | STD-TXN-080, DSN-DVM-564, DSN-DVM-565, DSN-DVM-566, DSN-DVM-750, FR-TXN-061, FR-TXN-062, FR-TXN-063 | `ForwardAtMostOnce`, `CompensateAtMostOnce` (each effect committed at most once per step, over attempts, repeated invocations and coordinators), `CompensationReverseOrder`, `NoCompensationOfUnappliedStep`, `NoForwardAfterCompensationStarts`, `CompensationOnlyWhileCompensating`, `StateMatchesEffects` (the saga object matches the committed effects), `CompletedAllAppliedOnce`, `CompensatedNetEmpty`, `TerminalStable`, `EndJustified` (`compensated`, `failed` and `escalated` only as declared and on recorded outcomes); temporal properties of `saga_live.tla` under weak fairness: `Terminates` (every saga ends), `EndsCompletedOrCompensated`, `EndsAsDeclared` |

Paragraph for the introduction of `verify/models/README.md`:

> `saga` models one saga of three steps with two coordinators that crash and restart, step
> and compensation processes that are idempotent by `(saga, step, attempt, phase)`, lost and
> repeated invocations, failures and step timeouts with the declared action (compensate or
> escalate). Like `group_commit` and `approval` it has a constant `Mutation` for the
> sensitivity checks (`reports/README.md` §7.3). Its temporal properties are checked by
> `saga_live.tla` (the same model with the specification `FairSpec`), at smaller bounds and
> by simulation at the stated bound (DEC-031).

For `check.sh`: add `saga saga_live` to the default models (`saga.cfg`, `saga.ci.cfg`,
`saga_live.cfg`, `saga_live.ci.cfg`); `saga_live_c1.cfg` and `saga_sim.cfg` are run by hand
with `saga_live.tla` (commands in the evidence files).
