------------------------------- MODULE approvals -------------------------------
(***************************************************************************)
(* Approvals and deferred commits (VER-FORM-0100).                        *)
(*                                                                         *)
(* Verifies: STD-TXN-060, STD-VER-021, STD-LGS-041, STD-LGS-042,           *)
(*           DSN-DVM-550, DSN-DVM-551, DSN-DVM-746, FR-FLOW-043,           *)
(*           FR-FLOW-045 (BPA 1.0-rc8, DEC-091).                           *)
(*                                                                         *)
(* Editors and approvers change objects directly and make proposals. A pending change    *)
(* (STD-TXN-060) records the bases of its objects; its preview is fixed. A *)
(* change set (STD-VER-021) has a preview that moves with every commit on  *)
(* its source branch. Approvers decide on proposals; each decision         *)
(* records the preview it saw. Separation of duties (STD-LGS-042) denies  *)
(* the maker, a second decision of a principal and a principal holding an  *)
(* incompatible duty on an object of the proposal (a committed call of a   *)
(* listed port). A pending change whose bases are no longer the heads is   *)
(* invalidated when it is approved or applied; a proposal may expire or be *)
(* rejected. A proposal is applied when the approvals on its current      *)
(* preview reach the required count; the applying commit records what it   *)
(* counted, so the properties are checked on the history independently of  *)
(* the guards.                                                             *)
(*                                                                         *)
(* An object version is a counter; a commit raises it.                     *)
(*                                                                         *)
(* Mutation selects a deliberately wrong variant for the sensitivity       *)
(* checks of reports/README.md; "none" is the design.                      *)
(***************************************************************************)
EXTENDS Integers, Sequences, FiniteSets, TLC

CONSTANTS Approvers,    \* principals who approve (and may hold incompatible duties)
          Editors,      \* principals who edit and make proposals
          Objs,         \* objects
          Required,     \* approvals required (STD-LGS-041 count)
          MaxVer,       \* bound on the version of an object
          MaxProps,     \* bound on proposals
          MaxRev,       \* bound on commits on the source of a change set
          Mutation      \* "none" or a mutation of reports/README.md

ASSUME Mutation \in {"none", "count_stale", "no_maker_check", "double_vote", "no_duty_check", "no_revalidate"}

Kinds == {"pending", "changeset"}
Open == "open"

VARIABLES head,     \* version of each object on the target
          duties,   \* set of <<principal, object>>: committed calls of an incompatible port
          props,    \* sequence of proposals
          log       \* sequence of applying commits with what they counted

vars == <<head, duties, props, log>>

Symm == Permutations(Approvers) \cup Permutations(Editors) \cup Permutations(Objs)

\* The preview a decision on proposal p sees now: fixed for a pending change, the source
\* revision for a change set.
Preview(p) == IF p.kind = "pending" THEN 0 ELSE p.rev

Stale(p) == p.kind = "pending" /\ \E o \in p.objs : p.bases[o] # head[o]

\* Approvals that count: decision approve on the current preview.
Counted(p) == IF Mutation = "count_stale"
              THEN {a \in p.decisions : a.d = "approve"}
              ELSE {a \in p.decisions : a.d = "approve" /\ a.preview = Preview(p)}

\* STD-LGS-042: whether principal w may decide on proposal p.
SoDOk(w, p) == /\ (Mutation = "no_maker_check" \/ w # p.maker)
               /\ (Mutation = "double_vote" \/ ~\E a \in p.decisions : a.who = w /\ a.preview = Preview(p))
               /\ (Mutation = "double_vote" \/ ~\E a \in p.decisions : a.who = w)
               /\ (Mutation = "no_duty_check" \/ ~\E o \in p.objs : <<w, o>> \in duties)

Init == /\ head = [o \in Objs |-> 0]
        /\ duties = {}
        /\ props = <<>>
        /\ log = <<>>

\* An editor changes an object directly; an approver's commit through an incompatible port
\* records a duty.
Edit(w, o) == /\ head[o] < MaxVer
              /\ head' = [head EXCEPT ![o] = @ + 1]
              /\ duties' = IF w \in Approvers THEN duties \cup {<<w, o>>} ELSE duties
              /\ UNCHANGED <<props, log>>

Propose(e, k, S) == /\ Len(props) < MaxProps
                    /\ S # {}
                    /\ props' = Append(props, [kind |-> k, maker |-> e, objs |-> S,
                                               bases |-> [o \in S |-> head[o]], rev |-> 0,
                                               decisions |-> {}, state |-> Open])
                    /\ UNCHANGED <<head, duties, log>>

\* A commit on the source of an open change set changes its preview.
SourceCommit(i) == /\ props[i].kind = "changeset" /\ props[i].state = Open
                   /\ props[i].rev < MaxRev
                   /\ props' = [props EXCEPT ![i].rev = @ + 1]
                   /\ UNCHANGED <<head, duties, log>>

\* A decision. A stale pending change is invalidated instead (STD-TXN-060).
Decide(w, i, d) ==
    LET p == props[i] IN
    /\ p.state = Open
    /\ IF Stale(p) /\ Mutation # "no_revalidate"
       THEN props' = [props EXCEPT ![i].state = "invalidated"]
       ELSE /\ SoDOk(w, p)
            /\ props' = [props EXCEPT ![i].decisions = @ \cup
                             {[who |-> w, d |-> d, preview |-> Preview(p),
                               held |-> {o \in p.objs : <<w, o>> \in duties}]},
                                      ![i].state = IF d = "reject" THEN "rejected" ELSE Open]
    /\ UNCHANGED <<head, duties, log>>

Expire(i) == /\ props[i].state = Open
             /\ props' = [props EXCEPT ![i].state = "expired"]
             /\ UNCHANGED <<head, duties, log>>

\* The final approval applies the proposal in a new process that re-validates the bases.
Apply(i) ==
    LET p == props[i] IN
    /\ p.state = Open
    /\ Cardinality({a.who : a \in Counted(p)}) >= Required
    /\ IF Stale(p) /\ Mutation # "no_revalidate"
       THEN /\ props' = [props EXCEPT ![i].state = "invalidated"]
            /\ UNCHANGED <<head, log>>
       ELSE /\ \A o \in p.objs : head[o] < MaxVer
            /\ head' = [o \in Objs |-> IF o \in p.objs THEN head[o] + 1 ELSE head[o]]
            /\ props' = [props EXCEPT ![i].state = "applied"]
            /\ log' = Append(log, [prop |-> i, kind |-> p.kind, maker |-> p.maker, objs |-> p.objs,
                                   bases |-> p.bases, heads |-> [o \in p.objs |-> head[o]],
                                   preview |-> Preview(p), counted |-> Counted(p),
                                   decisions |-> p.decisions])
    /\ UNCHANGED duties

Next == \/ \E w \in Editors \cup Approvers, o \in Objs : Edit(w, o)
        \/ \E e \in Editors \cup Approvers, k \in Kinds, S \in SUBSET Objs : Propose(e, k, S)
        \/ \E i \in 1..Len(props) : SourceCommit(i) \/ Expire(i) \/ Apply(i)
        \/ \E w \in Approvers, i \in 1..Len(props), d \in {"approve", "reject"} : Decide(w, i, d)

Spec == Init /\ [][Next]_vars

------------------------------------------------------------------------------
(* Properties, checked on the history of applying commits.                 *)

TypeOK == /\ head \in [Objs -> 0..MaxVer]
          /\ Len(log) <= Len(props)

Entries == {log[j] : j \in 1..Len(log)}

\* Approvals on the preview the commit applied, from the recorded decisions.
Valid(e) == {a \in e.decisions : a.d = "approve" /\ a.preview = e.preview}

\* No commit without the required number of distinct, valid approvals.
NoCommitWithoutApprovals == \A e \in Entries : Cardinality({a.who : a \in Valid(e)}) >= Required

\* Stale approvals never count: everything counted was given on the applied preview.
StaleNeverCount == \A e \in Entries : \A a \in e.counted : a.preview = e.preview

\* Separation of duties holds for every counted approval: not the maker, one decision per
\* principal, no incompatible duty on an object of the proposal when deciding.
SoDHolds == \A e \in Entries :
               /\ \A a \in e.counted : a.who # e.maker /\ a.held = {}
               /\ \A a, b \in e.decisions : a.who = b.who => a = b

\* A pending change is applied on the bases it was approved on (no lost update).
NoLostUpdate == \A e \in Entries : e.kind = "pending" => \A o \in e.objs : e.bases[o] = e.heads[o]

\* A proposal is applied at most once, and only while open.
AppliedOnce == \A j, k \in 1..Len(log) : log[j].prop = log[k].prop => j = k

=============================================================================
