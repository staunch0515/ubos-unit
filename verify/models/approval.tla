------------------------------- MODULE approval -------------------------------
(***************************************************************************)
(* Approvals and deferred commits on one branch (VER-FORM-0100).          *)
(*                                                                         *)
(* Verifies: STD-TXN-060, STD-VER-021, STD-LGS-041, STD-LGS-042,           *)
(*           STD-LGS-043, DSN-DVM-550, DSN-DVM-551, DSN-DVM-552,           *)
(*           DSN-DVM-746, FR-FLOW-043, FR-FLOW-045.                         *)
(*                                                                         *)
(* A principal submits a change whose decision requires approval. The      *)
(* flush stores the staged changes as a pending change with the base       *)
(* version of every object and the preview hash, and leaves the target     *)
(* untouched (DSN-DVM-550). Approvers decide on the pending change; every  *)
(* decision names the preview it was made on and is recorded as evidence   *)
(* (STD-LGS-043). A decision is denied unless the approver is eligible     *)
(* (STD-LGS-041) and separation of duties holds for the approver and the   *)
(* principal named in on_behalf_of (STD-LGS-042): neither initiated the    *)
(* change, neither decided on it before, neither made a committed call of  *)
(* an incompatible port on an object of the change. The approval that      *)
(* meets the required count starts the applying process (DSN-DVM-551),     *)
(* which re-validates the bases against the head under the sequencer: if   *)
(* they are unchanged it commits the stored changes and references the     *)
(* pending change, its preview and the approvals; otherwise the pending    *)
(* change becomes invalidated and its approvals are void (STD-TXN-060).    *)
(* An invalidated change may be re-run from the current head, which gives  *)
(* a new preview and needs new approvals (DSN-DVM-551). Meanwhile editors  *)
(* commit direct edits of the same objects, and principals make committed  *)
(* calls of incompatible ports. A pending change may expire at any time    *)
(* (DSN-DVM-552).                                                          *)
(*                                                                         *)
(* A version of an object is a counter per object; 0 is the initial        *)
(* version. A change set is a function from the written objects to their   *)
(* bases. The preview hash is taken as injective, so the preview is the    *)
(* change set itself (STD-TXN-060: content ID of {branch, changes}). The   *)
(* commit of the applying process is one atomic step; the compare-and-set  *)
(* between concurrent flushes is checked by commit_cas and group_commit.   *)
(*                                                                         *)
(* A pending change that ended keeps only what the properties refer to:    *)
(* an expired one nothing, a rejected one its initiator and decisions, an  *)
(* applied one its commit, an invalidated one the objects it writes.       *)
(*                                                                         *)
(* Mutation selects a deliberately wrong variant for the sensitivity       *)
(* checks of reports/README.md; "none" is the design.                      *)
(***************************************************************************)
EXTENDS Integers, FiniteSets, TLC

CONSTANTS Editors,     \* principals who are not eligible approvers
          Approvers,   \* principals named in the approval requirement (STD-LGS-041)
          Objs,        \* objects
          Changes,     \* identifiers of pending changes (one per deferring process)
          Counts,      \* values of `count` a requirement may have
          MaxEdits,    \* bound on direct edits
          MaxDuty,     \* bound on committed calls of incompatible ports
          MaxReruns,   \* bound on re-runs per pending change
          Mutation     \* "none" or a mutation of reports/README.md

ASSUME Mutation \in {"none", "no_count_check", "not_eligible", "keep_approvals", "no_preview_check",
                     "no_maker_checker", "double_decision", "no_duty_check", "no_base_check",
                     "rerun_commits", "eager_write", "no_expiry"}

Principals == Editors \cup Approvers
NoOne == "nobody"
MaxVer == MaxEdits + Cardinality(Changes)
Final == {"applied", "rejected", "expired", "invalidated"}

VARIABLES head,    \* branch state: current version of every object
          edits,   \* number of direct edits so far
          duty,    \* <<principal, object>>: committed calls of an incompatible port
          state,   \* pending change: "none", "pending", "approved", or a state of Final
          init,    \* pending change: its initiator
          need,    \* pending change: `count` of its approval requirement
          ch,      \* pending change: object -> base version; also its preview
          appr,    \* pending change: recorded decisions (DAT-Approval) that are not void
          reruns,  \* pending change: number of re-runs
          com      \* the commit that applied the pending change

vars == <<head, edits, duty, state, init, need, ch, appr, reruns, com>>

Symm == Permutations(Editors) \cup Permutations(Approvers) \cup Permutations(Objs)
        \cup Permutations(Changes)

NoCommit == [pv |-> <<>>, over |-> <<>>, apprs |-> {}, sod |-> TRUE]

Written(c) == DOMAIN ch[c]

\* The bases of the pending change are still the heads (STD-TXN-060).
Fresh(c) == \A o \in Written(c) : head[o] = ch[c][o]

Actors(a) == {a.by} \cup (IF a.obo = NoOne THEN {} ELSE {a.obo})

\* Principals who already decided on the pending change, directly or as on_behalf_of.
Done(c) == UNION {Actors(a) : a \in appr[c]}

\* Principals with a committed call of an incompatible port on an object of the change.
DutyOn(c) == {p \in Principals : \E o \in Written(c) : <<p, o>> \in duty}

\* An invalidated change keeps the objects it writes; its bases and preview are void.
Stale(c) == [o \in Written(c) |-> 0]

Approvals(S) == {a \in S : a.dec = "approve"}

Bump(W) == [o \in Objs |-> IF o \in W THEN head[o] + 1 ELSE head[o]]

Init == /\ head = [o \in Objs |-> 0]
        /\ edits = 0
        /\ duty = {}
        /\ state = [c \in Changes |-> "none"]
        /\ init = [c \in Changes |-> NoOne]
        /\ need = [c \in Changes |-> 0]
        /\ ch = [c \in Changes |-> <<>>]
        /\ appr = [c \in Changes |-> {}]
        /\ reruns = [c \in Changes |-> 0]
        /\ com = [c \in Changes |-> NoCommit]

\* A process of p whose decision requires `n` approvals is deferred: its changes of the
\* objects W are stored with their bases, the target is unchanged (DSN-DVM-550).
Submit(c, p, W, n) ==
    /\ state[c] = "none"
    /\ W # {}
    /\ state' = [state EXCEPT ![c] = "pending"]
    /\ init' = [init EXCEPT ![c] = p]
    /\ need' = [need EXCEPT ![c] = n]
    /\ ch' = [ch EXCEPT ![c] = [o \in W |-> head[o]]]
    /\ head' = IF Mutation = "eager_write" THEN Bump(W) ELSE head
    /\ UNCHANGED <<edits, duty, appr, reruns, com>>

\* An editor commits a direct edit of object o (a concurrent edit).
Edit(o) ==
    /\ edits < MaxEdits
    /\ edits' = edits + 1
    /\ head' = Bump({o})
    /\ UNCHANGED <<duty, state, init, need, ch, appr, reruns, com>>

\* Principal p commits a call of an incompatible port on object o (STD-LGS-042). The call
\* may write o (then it is also a concurrent edit) or leave it unchanged.
DutyCall(p, o, writes) ==
    /\ Cardinality(duty) < MaxDuty
    /\ <<p, o>> \notin duty
    /\ duty' = duty \cup {<<p, o>>}
    /\ IF writes
       THEN /\ edits < MaxEdits
            /\ edits' = edits + 1
            /\ head' = Bump({o})
       ELSE UNCHANGED <<head, edits>>
    /\ UNCHANGED <<state, init, need, ch, appr, reruns, com>>

\* The previews an approval request may name: the design accepts the current one only.
Previews(c) == IF Mutation = "no_preview_check"
               THEN {f \in [Written(c) -> 0..MaxVer] : \A o \in Written(c) : f[o] <= ch[c][o]}
               ELSE {ch[c]}

\* Approver a, possibly on behalf of obo, approves or rejects pending change c, having
\* decided on preview pv (STD-LGS-041, -042, -043). The pending change is refreshed first:
\* a change whose bases moved takes no decision (see Invalidate).
Decide(a, c, obo, pv, dec) ==
    LET actors == {a} \cup (IF obo = NoOne THEN {} ELSE {obo})
        rec == [by |-> a, obo |-> obo, dec |-> dec, pv |-> pv,
                \* evidence of separation of duties at the time of the decision
                sod |-> actors \cap DutyOn(c) = {}]
        all == appr[c] \cup {rec}
        count == IF Mutation = "no_count_check" THEN 1 ELSE need[c]
    IN /\ state[c] = "pending"
       /\ Fresh(c)
       /\ obo # a
       /\ pv \in Previews(c)
       /\ a \in Approvers \/ Mutation = "not_eligible"
       /\ init[c] \notin actors \/ Mutation = "no_maker_checker"
       /\ actors \cap Done(c) = {} \/ Mutation = "double_decision"
       /\ actors \cap DutyOn(c) = {} \/ Mutation = "no_duty_check"
       /\ rec \notin appr[c]
       /\ appr' = [appr EXCEPT ![c] = all]
       /\ state' = [state EXCEPT ![c] =
                      IF dec = "reject" THEN "rejected"
                      ELSE IF Cardinality(Approvals(all)) >= count THEN "approved"
                      ELSE "pending"]
       /\ ch' = IF dec = "reject" THEN [ch EXCEPT ![c] = <<>>] ELSE ch
       /\ UNCHANGED <<head, edits, duty, init, need, reruns, com>>

\* The applying process started by the final approval (DSN-DVM-551): bases unchanged ->
\* commit with the approvals referenced; bases changed -> invalidated, approvals void.
Apply(c) ==
    /\ state[c] = "approved"
    /\ LET W == Written(c)
           now == [o \in W |-> head[o]]
       IN IF Fresh(c) \/ Mutation = "no_base_check"
          THEN /\ com' = [com EXCEPT ![c] = [pv |-> ch[c], over |-> now, apprs |-> appr[c],
                                             sod |-> Done(c) \cap DutyOn(c) = {}]]
               /\ head' = Bump(W)
               /\ state' = [state EXCEPT ![c] = "applied"]
               /\ ch' = [ch EXCEPT ![c] = <<>>]
               /\ appr' = [appr EXCEPT ![c] = {}]
          ELSE IF Mutation = "rerun_commits"
          THEN \* re-run and commit the new changes on the strength of the old approvals
               /\ com' = [com EXCEPT ![c] = [pv |-> now, over |-> now, apprs |-> appr[c],
                                             sod |-> Done(c) \cap DutyOn(c) = {}]]
               /\ head' = Bump(W)
               /\ state' = [state EXCEPT ![c] = "applied"]
               /\ ch' = [ch EXCEPT ![c] = <<>>]
               /\ appr' = [appr EXCEPT ![c] = {}]
          ELSE /\ state' = [state EXCEPT ![c] = "invalidated"]
               /\ appr' = [appr EXCEPT ![c] = {}]
               /\ ch' = [ch EXCEPT ![c] = Stale(c)]
               /\ UNCHANGED <<head, com>>
    /\ UNCHANGED <<edits, duty, init, need, reruns>>

\* Refresh at a read or an approval: a pending change whose bases are no longer the heads
\* becomes invalidated and its approvals are void (STD-TXN-060, STD-LGS-043, FR-FLOW-045).
Invalidate(c) ==
    /\ state[c] = "pending"
    /\ ~Fresh(c)
    /\ state' = [state EXCEPT ![c] = "invalidated"]
    /\ appr' = [appr EXCEPT ![c] = IF Mutation = "keep_approvals" THEN @ ELSE {}]
    /\ ch' = [ch EXCEPT ![c] = Stale(c)]
    /\ UNCHANGED <<head, edits, duty, init, need, reruns, com>>

\* Re-run of the original process from the current head (DSN-DVM-551): new bases, new
\* preview, new approvals needed.
Rerun(c) ==
    /\ state[c] = "invalidated"
    /\ reruns[c] < MaxReruns
    /\ reruns' = [reruns EXCEPT ![c] = @ + 1]
    /\ ch' = [ch EXCEPT ![c] = [o \in Written(c) |-> head[o]]]
    /\ state' = [state EXCEPT ![c] = "pending"]
    /\ UNCHANGED <<head, edits, duty, init, need, appr, com>>

\* Expiry (DSN-DVM-552, STD-LGS-041 expires_in). Time is not modelled: a pending change
\* may expire at any moment and, under fairness, does so unless it ends otherwise.
Expire(c) ==
    /\ Mutation # "no_expiry"
    /\ state[c] = "pending"
    /\ state' = [state EXCEPT ![c] = "expired"]
    /\ init' = [init EXCEPT ![c] = NoOne]
    /\ need' = [need EXCEPT ![c] = 0]
    /\ ch' = [ch EXCEPT ![c] = <<>>]
    /\ appr' = [appr EXCEPT ![c] = {}]
    /\ UNCHANGED <<head, edits, duty, reruns, com>>

Next == \/ \E c \in Changes, p \in Principals, W \in SUBSET Objs, n \in Counts : Submit(c, p, W, n)
        \/ \E o \in Objs : Edit(o)
        \/ \E p \in Principals, o \in Objs, w \in BOOLEAN : DutyCall(p, o, w)
        \/ \E c \in Changes, a \in Principals, obo \in Principals \cup {NoOne},
              dec \in {"approve", "reject"} : \E pv \in Previews(c) : Decide(a, c, obo, pv, dec)
        \/ \E c \in Changes : Apply(c) \/ Invalidate(c) \/ Rerun(c) \/ Expire(c)

Spec == Init /\ [][Next]_vars

\* Fairness: an applying process finishes, and a pending change that stays pending expires.
FairSpec == Spec /\ \A c \in Changes : WF_vars(Apply(c)) /\ WF_vars(Expire(c))

(* ---- properties ---- *)

TypeOK == /\ head \in [Objs -> 0..MaxVer]
          /\ edits \in 0..MaxEdits
          /\ duty \subseteq Principals \X Objs
          /\ state \in [Changes -> {"none", "pending", "approved"} \cup Final]
          /\ init \in [Changes -> Principals \cup {NoOne}]
          /\ reruns \in [Changes -> 0..MaxReruns]
          /\ \A c \in Changes : /\ Written(c) \subseteq Objs
                                /\ \A a \in appr[c] \cup com[c].apprs :
                                     /\ a.by \in Principals
                                     /\ a.obo \in Principals \cup {NoOne}
                                     /\ a.dec \in {"approve", "reject"}

\* The approvals a commit may count: approvals of eligible approvers made on the committed
\* preview.
Valid(c) == {a \in Approvals(com[c].apprs) : a.by \in Approvers /\ a.pv = com[c].pv}

\* No commit without valid approvals (DSN-DVM-746, STD-LGS-041): an applied change has at
\* least `count` valid approvals of distinct approvers, none of them a rejection.
NoCommitWithoutValidApprovals ==
    \A c \in Changes : state[c] = "applied" =>
        /\ Cardinality({a.by : a \in Valid(c)}) >= need[c]
        /\ \A a \in com[c].apprs : a.dec = "approve"

\* Stale approvals never count (STD-VER-021, STD-LGS-043, FR-FLOW-045): every approval a
\* commit references was made on the committed preview, and every approval that is not void
\* was made on the current preview of its pending change.
StaleApprovalsNeverCount ==
    \A c \in Changes :
        /\ \A a \in com[c].apprs : a.pv = com[c].pv
        /\ state[c] \in {"pending", "approved"} => \A a \in appr[c] : a.pv = ch[c]

\* Separation of duties (STD-LGS-042, FR-FLOW-043), for every decision that is not void
\* and every decision a commit references: the initiator is neither the approver nor the
\* principal acted for; no principal decides twice, directly or through on_behalf_of; no
\* actor had made a committed call of an incompatible port on an object of the change.
SoDHolds ==
    \A c \in Changes : \A a \in appr[c] \cup com[c].apprs :
        /\ init[c] \notin Actors(a)
        /\ a.sod
        /\ \A b \in appr[c] \cup com[c].apprs : a # b => Actors(a) \cap Actors(b) = {}

\* What is committed is what was approved (DSN-DVM-551): the commit writes exactly the
\* objects of the approved preview, each over the base the approvers saw.
CommittedEqualsApproved ==
    \A c \in Changes : state[c] = "applied" =>
        /\ com[c].over = com[c].pv
        /\ \A a \in Approvals(com[c].apprs) : a.pv = com[c].over

\* The target state is unchanged while a change is pending (DSN-DVM-550): an object's head
\* moves only by a direct edit or by the commit that applies an approved change writing it.
TargetUnchangedWhilePending ==
    [][\A o \in Objs : head'[o] # head[o] =>
          \/ edits' = edits + 1
          \/ \E c \in Changes : /\ state[c] = "approved" /\ state'[c] = "applied"
                                /\ o \in Written(c)]_vars

\* Liveness under FairSpec: every pending change ends applied (committed), rejected,
\* expired, or invalidated without a further re-run.
Terminates == \A c \in Changes : (state[c] = "pending") ~> [](state[c] \in Final)

\* NOT a property of the design (finding F-5): separation of duties re-evaluated at the
\* commit. A principal can make an incompatible call that leaves the object unchanged after
\* approving, and the change is still applied.
SoDAtCommit == \A c \in Changes : com[c].sod
===============================================================================
