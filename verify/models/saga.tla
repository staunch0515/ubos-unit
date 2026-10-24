--------------------------------- MODULE saga ---------------------------------
(***************************************************************************)
(* Sagas (VER-FORM-0101).                                                  *)
(*                                                                         *)
(* Verifies: STD-TXN-080, DSN-DVM-564, DSN-DVM-566, FR-TXN-061,            *)
(*           FR-TXN-063 (BPA 1.0-rc8, DEC-092, DEC-097).                    *)
(*                                                                         *)
(* A saga of N steps runs each step as its own process. The coordinator    *)
(* commits every state change of the saga (status, completed steps,        *)
(* compensated steps, the step in progress); a step process commits its    *)
(* effect and its process record atomically, linked by (step, attempt,     *)
(* phase). A step ends ok, fails, or times out (a limit termination). On a *)
(* failure, backward recovery compensates the completed steps in reverse  *)
(* order; forward recovery retries the step up to MaxAttempts and then     *)
(* ends the saga failed. A failed compensation ends the saga failed.       *)
(* The coordinator may crash at any point: it loses only the process it    *)
(* was waiting for, which then never commits; it resumes from the          *)
(* committed saga state and does not run again a step whose process        *)
(* record exists.                                                          *)
(*                                                                         *)
(* The recovery mode and whether compensations may fail are chosen in the  *)
(* initial state, so one run covers all four combinations. effects and     *)
(* compOrder are the outside world: what step and compensation processes   *)
(* committed, in order.                                                    *)
(*                                                                         *)
(* Mutation selects a deliberately wrong variant for the sensitivity       *)
(* checks of reports/README.md; "none" is the design.                      *)
(***************************************************************************)
EXTENDS Integers, Sequences, FiniteSets, TLC

CONSTANTS N,            \* steps
          MaxAttempts,  \* attempts per step under forward recovery
          MaxCrashes,   \* bound on coordinator crashes
          Mutation      \* "none" or a mutation of reports/README.md

ASSUME Mutation \in {"none", "rerun", "forward_order", "comp_all", "lost_wait"}

Steps == 1..N
Terminal == {"completed", "compensated", "failed"}
NoCur == [step |-> 0, attempt |-> 0, phase |-> "none"]
Lost == [step |-> 0, attempt |-> 0, phase |-> "lost"]   \* "lost_wait": a crash leaves the wait behind

VARIABLES recovery,    \* "backward" or "forward" (chosen at start)
          compMayFail, \* whether a compensation can fail (chosen at start)
          status,      \* committed saga state
          done,        \* committed: steps 1..done are completed
          comped,      \* committed: compensated steps
          cur,         \* committed: the step in progress
          cause,       \* committed: why the saga failed
          records,     \* process records: set of [key, outcome]
          inflight,    \* volatile: the key of the process the coordinator waits for, or NoCur
          crashes,     \* number of crashes so far
          exec,        \* outside world: committed step processes per step
          comp,        \* outside world: committed compensations per step
          compOrder    \* outside world: compensated steps in commit order

vars == <<recovery, compMayFail, status, done, comped, cur, cause, records, inflight, crashes, exec, comp, compOrder>>

Key(c) == <<c.step, c.attempt, c.phase>>
HasRecord(k) == \E r \in records : r.key = k
RecordOf(k) == (CHOOSE r \in records : r.key = k).outcome

Init == /\ recovery \in {"backward", "forward"}
        /\ compMayFail \in BOOLEAN
        /\ status = "pending"
        /\ done = 0
        /\ comped = {}
        /\ cur = NoCur
        /\ cause = "none"
        /\ records = {}
        /\ inflight = NoCur
        /\ crashes = 0
        /\ exec = [s \in Steps |-> 0]
        /\ comp = [s \in Steps |-> 0]
        /\ compOrder = <<>>

Coord(vs) == UNCHANGED <<recovery, compMayFail, records, inflight, crashes, exec, comp, compOrder>> /\ vs

Start == /\ status = "pending"
         /\ Coord(status' = "running" /\ UNCHANGED <<done, comped, cur, cause>>)

\* The coordinator commits the next forward step, or completion.
NextStep == /\ status = "running" /\ cur = NoCur /\ inflight = NoCur
            /\ IF done = N
               THEN Coord(status' = "completed" /\ UNCHANGED <<done, comped, cur, cause>>)
               ELSE Coord(cur' = [step |-> done + 1, attempt |-> 1, phase |-> "forward"]
                          /\ UNCHANGED <<status, done, comped, cause>>)

\* The coordinator commits the next compensation: the highest completed step not compensated.
NextComp == /\ status = "compensating" /\ cur = NoCur /\ inflight = NoCur
            /\ LET todo == {s \in (IF Mutation = "comp_all" THEN Steps ELSE 1..done) : s \notin comped} IN
               IF todo = {}
               THEN Coord(status' = "compensated" /\ UNCHANGED <<done, comped, cur, cause>>)
               ELSE LET s == IF Mutation = "forward_order"
                             THEN CHOOSE x \in todo : \A y \in todo : x <= y
                             ELSE CHOOSE x \in todo : \A y \in todo : x >= y
                    IN Coord(cur' = [step |-> s, attempt |-> 1, phase |-> "compensate"]
                             /\ UNCHANGED <<status, done, comped, cause>>)

\* The coordinator starts the process of the step in progress, unless its record exists.
Invoke == /\ cur # NoCur /\ inflight = NoCur
          /\ (Mutation = "rerun" \/ ~HasRecord(Key(cur)))
          /\ inflight' = cur
          /\ UNCHANGED <<recovery, compMayFail, status, done, comped, cur, cause, records, crashes, exec, comp, compOrder>>

\* The process ends: its effect (when ok) and its record commit atomically.
ProcessEnd == /\ inflight \notin {NoCur, Lost}
              /\ \E o \in (IF inflight.phase = "forward" THEN {"ok", "fail", "timeout"}
                           ELSE IF compMayFail THEN {"ok", "fail"} ELSE {"ok"}) :
                    /\ records' = records \cup {[key |-> Key(inflight), outcome |-> o]}
                    /\ exec' = IF o = "ok" /\ inflight.phase = "forward"
                               THEN [exec EXCEPT ![inflight.step] = @ + 1] ELSE exec
                    /\ comp' = IF o = "ok" /\ inflight.phase = "compensate"
                               THEN [comp EXCEPT ![inflight.step] = @ + 1] ELSE comp
                    /\ compOrder' = IF o = "ok" /\ inflight.phase = "compensate"
                                    THEN Append(compOrder, inflight.step) ELSE compOrder
              /\ inflight' = NoCur
              /\ UNCHANGED <<recovery, compMayFail, status, done, comped, cur, cause, crashes>>

\* The coordinator commits the saga state that the record of the step in progress implies.
Advance == /\ cur # NoCur /\ inflight = NoCur /\ HasRecord(Key(cur))
           /\ LET o == RecordOf(Key(cur)) IN
              IF cur.phase = "forward"
              THEN IF o = "ok"
                   THEN Coord(done' = done + 1 /\ cur' = NoCur /\ UNCHANGED <<status, comped, cause>>)
                   ELSE IF recovery = "forward"
                        THEN IF cur.attempt < MaxAttempts
                             THEN Coord(cur' = [cur EXCEPT !.attempt = @ + 1] /\ UNCHANGED <<status, done, comped, cause>>)
                             ELSE Coord(status' = "failed" /\ cause' = "exhausted" /\ cur' = NoCur
                                        /\ UNCHANGED <<done, comped>>)
                        ELSE Coord(status' = "compensating" /\ cause' = "step_failed" /\ cur' = NoCur
                                   /\ UNCHANGED <<done, comped>>)
              ELSE IF o = "ok"
                   THEN Coord(comped' = comped \cup {cur.step} /\ cur' = NoCur /\ UNCHANGED <<status, done, cause>>)
                   ELSE Coord(status' = "failed" /\ cause' = "comp_failed" /\ cur' = NoCur
                              /\ UNCHANGED <<done, comped>>)

\* A crash loses the process the coordinator waits for; it never commits. The coordinator
\* resumes from the committed saga state and the process records.
Crash == /\ crashes < MaxCrashes
         /\ crashes' = crashes + 1
         /\ inflight' = IF Mutation = "lost_wait" /\ inflight # NoCur THEN Lost ELSE NoCur
         /\ UNCHANGED <<recovery, compMayFail, status, done, comped, cur, cause, records, exec, comp, compOrder>>

Progress == Start \/ NextStep \/ NextComp \/ Invoke \/ ProcessEnd \/ Advance

Next == Progress \/ Crash

Spec == Init /\ [][Next]_vars /\ WF_vars(Progress)

------------------------------------------------------------------------------
(* Properties.                                                             *)

TypeOK == /\ status \in {"pending", "running", "compensating"} \cup Terminal
          /\ done \in 0..N
          /\ comped \subseteq Steps
          /\ crashes \in 0..MaxCrashes

\* Every step and every compensation commits at most once.
AtMostOnce == \A s \in Steps : exec[s] <= 1 /\ comp[s] <= 1

\* Only a step that committed is compensated.
CompAfterExec == \A s \in Steps : comp[s] <= exec[s]

\* Compensations run in reverse order of the steps.
ReverseOrder == \A i, j \in 1..Len(compOrder) : i < j => compOrder[i] > compOrder[j]

\* A completed saga ran every step once and compensated none.
CompletedOK == status = "completed" => \A s \in Steps : exec[s] = 1 /\ comp[s] = 0

\* A compensated saga compensated every step that committed.
CompensatedOK == status = "compensated" => \A s \in Steps : comp[s] = exec[s]

\* A saga fails only when forward recovery is exhausted or a compensation fails.
FailedJustified == status = "failed" =>
                     \/ cause = "exhausted" /\ recovery = "forward"
                     \/ cause = "comp_failed" /\ compMayFail

\* DSN-DVM-566 as worded: under backward recovery with compensations that do not fail, a
\* saga ends completed or compensated.
EndsCompletedOrCompensated == (recovery = "backward" /\ ~compMayFail) => status # "failed"

\* Every started saga reaches a terminal state.
Termination == <>(status \in Terminal)

=============================================================================
