-------------------------------- MODULE saga --------------------------------
(***************************************************************************)
(* The saga protocol (VER-FORM-0101, EXIT-2-02).                           *)
(*                                                                         *)
(* Verifies: STD-TXN-080, DSN-DVM-564, DSN-DVM-565, DSN-DVM-566,           *)
(*           DSN-DVM-750 (idempotency by (instance, step, attempt)),       *)
(*           FR-TXN-061, FR-TXN-062, FR-TXN-063.                            *)
(*                                                                         *)
(* One saga of N steps. The saga object is the only durable state of the   *)
(* coordinator: every state transition is one compare-and-set commit on    *)
(* the saga object (STD-TXN-080 "Commits"). The transition function Trans  *)
(* is the loop of the executable reference model                           *)
(* (standard/tools/bpa/workflows.py, _saga_loop): pending -> running ->    *)
(* completed; on a failure under backward recovery running ->              *)
(* compensating -> compensated; a failed compensation, or forward recovery *)
(* that has used MaxAttempts attempts, ends the saga failed. The model     *)
(* adds the step timeout of DSN-DVM-565 with its declared action           *)
(* ("compensate" or "escalate") and the terminal state "escalated".        *)
(*                                                                         *)
(* Coordinators run in the host (DSN-DVM-564). A coordinator holds only a  *)
(* volatile view of the saga object; it can crash at any point and         *)
(* restart, and several coordinators (a restarted node and one that is     *)
(* slow, not dead) can act on the same saga. The compare-and-set on the    *)
(* saga object arbitrates between them.                                    *)
(*                                                                         *)
(* A step or compensation is a process on its own branch or VAE, invoked   *)
(* under the key (step, attempt, phase) of this saga. The process commits  *)
(* its effect and its outcome record under that key in one commit, so an   *)
(* invocation repeated after a crash is not applied twice. An invocation   *)
(* stays in flight independently of the coordinator that made it: it can   *)
(* be executed late, be lost (bounded by MaxLost) and be repeated.         *)
(*                                                                         *)
(* Outcomes are write-once records under the key: "ok", "error" (a failed  *)
(* process) and "cancelled". A coordinator acts only on a recorded         *)
(* outcome. On a timeout it does not decide by itself: it commits the      *)
(* outcome "cancelled" under the key of the attempt, on the branch of the  *)
(* step, if no outcome is recorded yet. This fences the attempt: a step    *)
(* that would still commit after its timeout finds the key taken and does  *)
(* nothing. If the step committed first, the timeout finds "ok" and the    *)
(* saga continues (findings F-1, F-2 of reports/saga.section.md).          *)
(*                                                                         *)
(* The declaration of the saga (recovery, timeout action and compensation  *)
(* per step) is chosen in the initial state from the configured sets, so   *)
(* one run covers every declaration. A timeout may pass at any moment or   *)
(* never, so a step without a timeout is covered by one that declares one. *)
(*                                                                         *)
(* Mutation selects a deliberately wrong variant for the sensitivity       *)
(* checks of reports/saga.section.md; "none" is the design.                *)
(***************************************************************************)
EXTENDS Integers, Sequences, FiniteSets, TLC

CONSTANTS N,              \* number of steps
          Coords,         \* coordinators (host nodes that may run the saga)
          MaxAttempts,    \* max_attempts of forward recovery
          Recoveries,     \* subset of {"backward", "forward"} to choose the declaration from
          TimeoutActions, \* subset of {"none", "compensate", "escalate"} per step
          CompChoices,    \* subset of BOOLEAN: a step has a compensation
          CompCanFail,    \* a compensation process may fail
          MaxCrashes,     \* bound on coordinator crashes
          MaxLost,        \* bound on lost invocations (executor crashes before its commit)
          Mutation        \* "none" or a mutation of reports/saga.section.md

ASSUME Mutation \in {"none", "no_dedupe", "rerun_failed", "no_fence", "timeout_as_ok",
                     "comp_forward_order", "comp_failed_step", "comp_error_skipped",
                     "no_cas", "no_reinvoke"}

Steps == 1..N
Keys == [step : Steps, attempt : 1..MaxAttempts, phase : {"forward", "compensate"}]
Key(s, a, p) == [step |-> s, attempt |-> a, phase |-> p]
NoCur == [step |-> 0, attempt |-> 0, phase |-> "none"]
NoErr == [step |-> 0, why |-> "none"]
End == {"completed", "compensated", "failed", "escalated"}

VARIABLES decl,       \* declaration: [recovery, onTimeout[s], comp[s]]; constant in a behaviour
          saga,       \* durable: the saga object [ver, status, completed, compensated, cur, err]
          results,    \* durable, on the branches of the steps: outcome record per key
          fx,         \* durable effects: steps in the order their forward effects were committed
          ux,         \* durable effects: steps in the order their compensations were committed
          inflight,   \* invocations in flight (keys)
          cst,        \* coordinator: "down", "idle" or "loaded"
          cview,      \* coordinator: its view of the saga object (volatile)
          crashes,    \* number of coordinator crashes so far
          lost,       \* number of lost invocations so far
          invoked,    \* keys invoked so far (maintained by mutation "no_reinvoke" only)
          okFwd,      \* history: every forward effect was committed while the saga was running
          okUndo,     \* history: every compensation was committed while the saga was compensating
          okTerminal  \* history: no commit changed a saga object in a terminal state

vars == <<decl, saga, results, fx, ux, inflight, cst, cview, crashes, lost, invoked,
          okFwd, okUndo, okTerminal>>

Range(q) == {q[i] : i \in DOMAIN q}
Count(q, s) == Cardinality({i \in DOMAIN q : q[i] = s})
Max(X) == CHOOSE x \in X : \A y \in X : y <= x
Min(X) == CHOOSE x \in X : \A y \in X : x <= y

InitSaga == [ver |-> 0, status |-> "pending", completed |-> <<>>, compensated |-> <<>>,
             cur |-> NoCur, err |-> NoErr]

\* The declarations. Under forward recovery no compensation ever runs, so one compensation
\* declaration stands for all of them.
Decls == {d \in [recovery : Recoveries, onTimeout : [Steps -> TimeoutActions],
                 comp : [Steps -> CompChoices]] :
              d.recovery = "forward" => \A s \in Steps : d.comp[s] = (TRUE \in CompChoices)}

Init == /\ decl \in Decls
        /\ saga = InitSaga
        /\ results = [k \in Keys |-> "none"]
        /\ fx = <<>>
        /\ ux = <<>>
        /\ inflight = {}
        /\ cst = [c \in Coords |-> "idle"]
        /\ cview = [c \in Coords |-> InitSaga]
        /\ crashes = 0
        /\ lost = 0
        /\ invoked = {}
        /\ okFwd = TRUE
        /\ okUndo = TRUE
        /\ okTerminal = TRUE

(***************************************************************************)
(* The transition function of the coordinator (reference model:           *)
(* _saga_loop). v is a view of the saga object.                            *)
(***************************************************************************)

\* Positions (in v.completed) of the completed steps that are not compensated yet.
TodoIdx(v) == {i \in DOMAIN v.completed : v.completed[i] \notin Range(v.compensated)}

\* The step to compensate next: the last completed one (reverse order, FR-TXN-063).
TodoHead(v) == v.completed[IF Mutation = "comp_forward_order" THEN Min(TodoIdx(v))
                                                              ELSE Max(TodoIdx(v))]

\* The view waits for the outcome of the process under the key v.cur.
InCall(v) ==
    \/ /\ v.status = "running"
       /\ Len(v.completed) < N
       /\ v.cur.step = Len(v.completed) + 1
       /\ v.cur.phase = "forward"
    \/ /\ v.status = "compensating"
       /\ TodoIdx(v) # {}
       /\ decl.comp[TodoHead(v)]
       /\ v.cur.step = TodoHead(v)
       /\ v.cur.phase = "compensate"

\* The recorded outcome the view waits for.
Res(v) == IF InCall(v) THEN results[v.cur] ELSE "none"

\* A forward step failed or timed out (why is "error" or "timeout").
Fail(v, s, why) ==
    IF decl.recovery = "forward"
    THEN IF v.cur.attempt < MaxAttempts
         THEN [v EXCEPT !.cur.attempt = @ + 1]
         ELSE [v EXCEPT !.status = "failed", !.cur = NoCur, !.err = [step |-> s, why |-> why]]
    ELSE [v EXCEPT !.status = "compensating", !.cur = NoCur, !.err = [step |-> s, why |-> why],
                   !.completed = IF Mutation = "comp_failed_step" THEN Append(@, s) ELSE @]

\* The next state of the saga object, given the view v and the recorded outcome r of the
\* process it waits for ("none": no outcome yet). Returns v when there is nothing to commit.
Trans(v, r) ==
    CASE v.status = "pending" -> [v EXCEPT !.status = "running"]
      [] v.status = "running" ->
           IF Len(v.completed) = N
           THEN [v EXCEPT !.status = "completed", !.cur = NoCur]
           ELSE LET s == Len(v.completed) + 1
                    done == [v EXCEPT !.completed = Append(@, s), !.cur = NoCur]
                IN IF ~InCall(v)
                   THEN [v EXCEPT !.cur = Key(s, 1, "forward")]
                   ELSE (CASE r = "none" -> v
                           [] r = "ok" -> done
                           [] r = "error" -> Fail(v, s, "error")
                           [] r = "cancelled" ->
                                IF Mutation = "timeout_as_ok" THEN done
                                ELSE IF decl.onTimeout[s] = "escalate"
                                THEN [v EXCEPT !.status = "escalated", !.cur = NoCur,
                                               !.err = [step |-> s, why |-> "timeout"]]
                                ELSE Fail(v, s, "timeout"))
      [] v.status = "compensating" ->
           IF TodoIdx(v) = {}
           THEN [v EXCEPT !.status = "compensated", !.cur = NoCur]
           ELSE LET u == TodoHead(v)
                    undone == [v EXCEPT !.compensated = Append(@, u), !.cur = NoCur]
                IN IF ~decl.comp[u]
                   THEN undone  \* a step without a compensation counts as compensated
                   ELSE IF ~InCall(v)
                   THEN [v EXCEPT !.cur = Key(u, 1, "compensate")]
                   ELSE (CASE r = "none" -> v
                           [] r = "ok" -> undone
                           [] OTHER ->
                                IF Mutation = "comp_error_skipped" THEN undone
                                ELSE [v EXCEPT !.status = "failed", !.cur = NoCur,
                                               !.err = [step |-> u, why |-> "compensation"]])
      [] OTHER -> v

(***************************************************************************)
(* Coordinators.                                                           *)
(***************************************************************************)

\* Commit n over the view v: compare-and-set on the version of the saga object.
Commit(c, v, n) ==
    /\ IF Mutation = "no_cas" \/ saga.ver = v.ver
       THEN /\ saga' = [n EXCEPT !.ver = saga.ver + 1]
            /\ okTerminal' = (okTerminal /\ saga.status \notin End)
       ELSE UNCHANGED <<saga, okTerminal>>
    /\ cst' = [cst EXCEPT ![c] = "idle"]
    /\ cview' = [cview EXCEPT ![c] = InitSaga]

\* Read the saga object.
Load(c) ==
    /\ cst[c] = "idle"
    /\ cst' = [cst EXCEPT ![c] = "loaded"]
    /\ cview' = [cview EXCEPT ![c] = saga]
    /\ UNCHANGED <<decl, saga, results, fx, ux, inflight, crashes, lost, invoked, okFwd, okUndo, okTerminal>>

\* Commit the next state transition, from the view and the recorded outcome.
Advance(c) ==
    /\ cst[c] = "loaded"
    /\ LET v == cview[c]
           n == Trans(v, Res(v))
       IN /\ n # v
          /\ Commit(c, v, n)
    /\ UNCHANGED <<decl, results, fx, ux, inflight, crashes, lost, invoked, okFwd, okUndo>>

\* Invoke (again) the process the view waits for. The coordinator cannot know whether an
\* earlier invocation is lost, in flight or committed.
Invoke(c) ==
    /\ cst[c] = "loaded"
    /\ InCall(cview[c])
    /\ cview[c].cur \notin inflight
    /\ Mutation = "no_reinvoke" => cview[c].cur \notin invoked
    /\ inflight' = inflight \cup {cview[c].cur}
    /\ invoked' = IF Mutation = "no_reinvoke" THEN invoked \cup {cview[c].cur} ELSE invoked
    /\ UNCHANGED <<decl, saga, results, fx, ux, cst, cview, crashes, lost, okFwd, okUndo, okTerminal>>

\* The timeout of a forward step passes (DSN-DVM-565). The coordinator fences the attempt:
\* a process on the branch of the step records "cancelled" under the key, if the key is free.
Timeout(c) ==
    /\ cst[c] = "loaded"
    /\ LET v == cview[c]
       IN /\ InCall(v)
          /\ v.cur.phase = "forward"
          /\ decl.onTimeout[v.cur.step] # "none"
          /\ results[v.cur] = "none"
          /\ IF Mutation = "no_fence"
             THEN \* wrong: the coordinator decides alone and records it in the saga object only
                  /\ Commit(c, v, Trans(v, "cancelled"))
                  /\ UNCHANGED results
             ELSE /\ results' = [results EXCEPT ![v.cur] = "cancelled"]
                  /\ UNCHANGED <<saga, cst, cview, okTerminal>>
    /\ UNCHANGED <<decl, fx, ux, inflight, crashes, lost, invoked, okFwd, okUndo>>

Crash(c) ==
    /\ cst[c] # "down"
    /\ crashes < MaxCrashes
    /\ crashes' = crashes + 1
    /\ cst' = [cst EXCEPT ![c] = "down"]
    /\ cview' = [cview EXCEPT ![c] = InitSaga]
    /\ UNCHANGED <<decl, saga, results, fx, ux, inflight, lost, invoked, okFwd, okUndo, okTerminal>>

Restart(c) ==
    /\ cst[c] = "down"
    /\ cst' = [cst EXCEPT ![c] = "idle"]
    /\ UNCHANGED <<decl, saga, results, fx, ux, inflight, cview, crashes, lost, invoked, okFwd, okUndo, okTerminal>>

(***************************************************************************)
(* Step and compensation processes (on the branches of the steps).        *)
(***************************************************************************)

\* The key is free: the process runs. Otherwise the invocation is a duplicate and does nothing.
Free(k) == \/ results[k] = "none"
           \/ Mutation = "no_dedupe"
           \/ Mutation = "rerun_failed" /\ results[k] = "error"

\* An invocation in flight is executed: the effect and the outcome record in one commit, or
\* a failure recorded under the key, or nothing (duplicate), or the invocation is lost.
Exec(k) ==
    /\ k \in inflight
    /\ inflight' = inflight \ {k}
    /\ \/ /\ Free(k)                      \* the process commits
          /\ results' = [results EXCEPT ![k] = "ok"]
          /\ IF k.phase = "forward"
             THEN /\ fx' = Append(fx, k.step)
                  /\ okFwd' = (okFwd /\ saga.status = "running")
                  /\ UNCHANGED <<ux, okUndo>>
             ELSE /\ ux' = Append(ux, k.step)
                  /\ okUndo' = (okUndo /\ saga.status = "compensating")
                  /\ UNCHANGED <<fx, okFwd>>
          /\ UNCHANGED lost
       \/ /\ results[k] = "none"          \* the process fails; the failure is recorded
          /\ k.phase = "compensate" => CompCanFail
          /\ results' = [results EXCEPT ![k] = "error"]
          /\ UNCHANGED <<fx, ux, lost, okFwd, okUndo>>
       \/ /\ ~Free(k)                     \* duplicate: the key is taken
          /\ UNCHANGED <<results, fx, ux, lost, okFwd, okUndo>>
       \/ /\ results[k] = "none"          \* lost before its commit
          /\ lost < MaxLost
          /\ lost' = lost + 1
          /\ UNCHANGED <<results, fx, ux, okFwd, okUndo>>
    /\ UNCHANGED <<decl, saga, cst, cview, crashes, invoked, okTerminal>>

Next == \/ \E c \in Coords : Load(c) \/ Advance(c) \/ Invoke(c) \/ Timeout(c) \/ Crash(c) \/ Restart(c)
        \/ \E k \in Keys : Exec(k)

\* Fairness: a coordinator that is up keeps working (it reads the saga object, commits the
\* next transition and invokes the process it waits for) and a crashed one restarts; an
\* invocation in flight is eventually executed. Timeouts and crashes are not fair. Work(c)
\* joins three actions of which at most one is enabled in a state.
Work(c) == Load(c) \/ Advance(c) \/ Restart(c)

Fairness == /\ \A c \in Coords : WF_vars(Work(c)) /\ WF_vars(Invoke(c))
            /\ WF_vars(\E k \in Keys : Exec(k))

Spec == Init /\ [][Next]_vars /\ Fairness

(***************************************************************************)
(* Properties.                                                             *)
(***************************************************************************)

SagaType == [ver : Nat,
             status : {"pending", "running", "compensating"} \cup End,
             completed : Seq(Steps), compensated : Seq(Steps),
             cur : Keys \cup {NoCur},
             err : [step : 0..N, why : {"none", "error", "timeout", "compensation"}]]

TypeOK == /\ decl \in Decls
          /\ saga \in SagaType
          /\ results \in [Keys -> {"none", "ok", "error", "cancelled"}]
          /\ fx \in Seq(Steps) /\ ux \in Seq(Steps)
          /\ inflight \subseteq Keys
          /\ cst \in [Coords -> {"down", "idle", "loaded"}]
          /\ \A c \in Coords : cview[c] \in SagaType
          /\ crashes \in 0..MaxCrashes /\ lost \in 0..MaxLost
          /\ invoked \subseteq Keys
          /\ okFwd \in BOOLEAN /\ okUndo \in BOOLEAN /\ okTerminal \in BOOLEAN

\* The forward effect of a step is applied at most once, over all attempts and repeats.
ForwardAtMostOnce == \A s \in Steps : Count(fx, s) <= 1

\* DSN-DVM-566: compensations run at most once per step.
CompensateAtMostOnce == \A s \in Steps : Count(ux, s) <= 1

\* FR-TXN-063: compensations run in reverse order, and none is passed over.
CompensationReverseOrder ==
    /\ \A i, j \in DOMAIN ux : i < j => ux[i] > ux[j]
    /\ \A s \in Range(ux) : \A t \in Range(fx) : (t > s /\ decl.comp[t]) => t \in Range(ux)

\* Only a step whose forward effect is committed is compensated.
NoCompensationOfUnappliedStep == \A s \in Range(ux) : s \in Range(fx)

\* No forward effect is committed once the saga has left "running" (in particular not
\* after compensation has started, nor after the saga has ended).
NoForwardAfterCompensationStarts == okFwd

\* Compensations are committed only while the saga is compensating.
CompensationOnlyWhileCompensating == okUndo

\* The state in the saga object matches the effects (FR-TXN-062); the only gap is the
\* current step, between the commit of its process and the commit that records it.
StateMatchesEffects ==
    /\ \A s \in Range(saga.completed) : Count(fx, s) = 1
    /\ \A s \in Range(fx) : s \notin Range(saga.completed) =>
           saga.status = "running" /\ saga.cur.step = s /\ saga.cur.phase = "forward"
    /\ \A s \in Range(saga.compensated) :
           s \in Range(saga.completed) /\ (decl.comp[s] => Count(ux, s) = 1)
    /\ \A s \in Range(ux) : s \notin Range(saga.compensated) =>
           saga.status = "compensating" /\ saga.cur.step = s /\ saga.cur.phase = "compensate"

\* A completed saga has applied every step exactly once, in order, and compensated none.
CompletedAllAppliedOnce ==
    saga.status = "completed" => fx = [i \in 1..N |-> i] /\ ux = <<>>

\* A compensated saga has no net effect: every applied step that has a compensation is
\* compensated exactly once (a step without a compensation counts as compensated).
CompensatedNetEmpty ==
    saga.status = "compensated" =>
        \A s \in Steps : decl.comp[s] => Count(fx, s) = Count(ux, s)

\* A saga object in a terminal state is never changed.
TerminalStable == okTerminal

\* Every terminal state other than "completed" is justified by the declaration and by
\* recorded outcomes.
EndJustified ==
    /\ saga.status = "compensated" => decl.recovery = "backward"
    /\ saga.status = "escalated" =>
           /\ saga.err.step \in Steps
           /\ decl.onTimeout[saga.err.step] = "escalate"
           /\ \E a \in 1..MaxAttempts : results[Key(saga.err.step, a, "forward")] = "cancelled"
    /\ saga.status = "failed" =>
           /\ saga.err.step \in Steps
           /\ \/ results[Key(saga.err.step, 1, "compensate")] = "error"
              \/ /\ decl.recovery = "forward"
                 /\ \A a \in 1..MaxAttempts :
                        results[Key(saga.err.step, a, "forward")] \in {"error", "cancelled"}

\* Liveness: every saga ends.
Terminates == <>(saga.status \in End)

\* The saga ended failed because a compensation process failed (STD-TXN-080 "States").
CompensationFailed ==
    /\ saga.status = "failed"
    /\ \E s \in Steps : results[Key(s, 1, "compensate")] = "error"

\* DSN-DVM-566: under backward recovery without escalation every saga ends completed or
\* compensated, unless a compensation fails (never, where CompCanFail is FALSE).
EndsCompletedOrCompensated ==
    (decl.recovery = "backward" /\ \A s \in Steps : decl.onTimeout[s] # "escalate")
    => <>(saga.status \in {"completed", "compensated"} \/ CompensationFailed)

\* With escalation: a saga under backward recovery ends completed, compensated, or
\* escalated by a step that declares it, unless a compensation fails.
EndsAsDeclared ==
    decl.recovery = "backward"
    => <>(\/ saga.status \in {"completed", "compensated"}
          \/ saga.status = "escalated" /\ decl.onTimeout[saga.err.step] = "escalate"
          \/ CompensationFailed)
=============================================================================
