------------------------------- MODULE timeline -------------------------------
(***************************************************************************)
(* Timeline algebra and bitemporal reads (VER-FORM-0003).                 *)
(*                                                                         *)
(* Verifies: STD-TIME-010, STD-TIME-011, STD-TIME-012, STD-TIME-031,       *)
(*           DSN-DVM-303, DSN-DVM-326; materialisation rule AR-016.        *)
(*                                                                         *)
(* A branch is a log of commits; each commit holds timeline operations     *)
(* (STD-TIME-011). The implementation keeps a materialised index of the    *)
(* current timelines, updated incrementally at each commit (AR-016). The   *)
(* definitional timeline TL(o, t) is recomputed from the log. Derived      *)
(* results record the version they read; a retroactive change computes an  *)
(* impact set (STD-TIME-031, one derivation level).                        *)
(*                                                                         *)
(* Valid time is a small set of dates 1..Inf-1; Inf stands for an open     *)
(* period end. Transaction time is the commit index (STD-VER-003).         *)
(***************************************************************************)
EXTENDS Naturals, Sequences, FiniteSets, TLC

CONSTANTS Objs,        \* objects
          Inf,         \* the open end of valid time; dates are 1..Inf-1
          MaxOps,      \* bound on timeline operations in the whole run
          MaxCommits,  \* bound on commits
          MaxDerivs    \* bound on derived results

Dates == 1..(Inf - 1)
Bounds == 1..Inf
None == 0              \* "not found" (versions are positive)

VARIABLES log,       \* sequence of commits; a commit is a sequence of operations
          pending,   \* operations staged for the next commit
          nops,      \* operations staged so far (versions are 1..MaxOps)
          index,     \* materialised current timelines (AR-016)
          answers,   \* reads recorded at each commit: [t, o, a, v]
          derivs,    \* derived results: [id, o, a, v]
          impacted   \* ids of derived results in some impact set

vars == <<log, pending, nops, index, answers, derivs, impacted>>

Symm == Permutations(Objs)

Seg(f, t, v) == [f |-> f, t |-> t, v |-> v]

(* ---- the timeline algebra of STD-TIME-011 ---- *)

\* Remove the valid-time interval [a, b) from a set of segments, splitting segments.
Remove(S, a, b) ==
    {Seg(s.f, IF s.t < a THEN s.t ELSE a, s.v) : s \in {x \in S : x.f < a}}
    \cup {Seg(IF s.f > b THEN s.f ELSE b, s.t, s.v) : s \in {x \in S : x.t > b}}

Min(X) == CHOOSE x \in X : \A y \in X : x <= y

NextStart(S, d) == Min({s.f : s \in {x \in S : x.f > d}} \cup {Inf})

\* set_period is rejected when an existing segment intersects [a, b) without lying
\* inside it (conservative reading of "same version lineage", see report, OQ-004).
SetPeriodRejected(S, a, b) == \E s \in S : s.f < b /\ a < s.t /\ ~(a <= s.f /\ s.t <= b)

Exists(S) == S # {}

Allowed(S, op) ==
    CASE op.kind = "create"         -> S = {}
      [] op.kind = "set_period"     -> Exists(S) /\ ~SetPeriodRejected(S, op.a, op.b)
      [] OTHER                      -> Exists(S)

Apply(S, op) ==
    CASE op.kind = "create"         -> {Seg(op.a, Inf, op.v)}
      [] op.kind = "supersede_from" ->
            LET e == NextStart(S, op.a) IN Remove(S, op.a, e) \cup {Seg(op.a, e, op.v)}
      [] op.kind = "set_period"     -> Remove(S, op.a, op.b) \cup {Seg(op.a, op.b, op.v)}
      [] op.kind = "replace_future" -> Remove(S, op.a, Inf) \cup {Seg(op.a, Inf, op.v)}
      [] op.kind = "end"            -> Remove(S, op.a, Inf)

\* Valid range changed by an operation (the R of STD-TIME-031).
Range(S, op) ==
    CASE op.kind = "supersede_from" -> {d \in Dates : op.a <= d /\ d < NextStart(S, op.a)}
      [] op.kind = "set_period"     -> {d \in Dates : op.a <= d /\ d < op.b}
      [] OTHER                      -> {d \in Dates : op.a <= d}

RECURSIVE ApplyAll(_, _, _)
ApplyAll(S, o, ops) ==
    IF ops = <<>> THEN S
    ELSE ApplyAll(IF Head(ops).o = o THEN Apply(S, Head(ops)) ELSE S, o, Tail(ops))

RECURSIVE Flatten(_)
Flatten(cs) == IF cs = <<>> THEN <<>> ELSE Head(cs) \o Flatten(Tail(cs))

\* The definitional timeline TL(o, X, t): replay the first t commits from empty.
TL(o, t) == ApplyAll({}, o, Flatten(SubSeq(log, 1, t)))

ReadIn(S, a) == IF \E s \in S : s.f <= a /\ a < s.t
                THEN (CHOOSE s \in S : s.f <= a /\ a < s.t).v ELSE None

\* STD-TIME-012: as-of resolution at (asof = a, time = t).
ReadAt(o, a, t) == ReadIn(TL(o, t), a)

\* Current timeline of o including operations staged in this commit.
Cur(o) == ApplyAll(index[o], o, pending)

(* ---- behaviour ---- *)

Init == /\ log = <<>>
        /\ pending = <<>>
        /\ nops = 0
        /\ index = [o \in Objs |-> {}]
        /\ answers = {}
        /\ derivs = {}
        /\ impacted = {}

Ops(o) ==
    {[o |-> o, kind |-> k, a |-> d, b |-> Inf, v |-> nops + 1] :
        k \in {"create", "supersede_from", "replace_future", "end"}, d \in Dates}
    \cup {[o |-> o, kind |-> "set_period", a |-> a, b |-> b, v |-> nops + 1] :
        a \in Dates, b \in Bounds \ {Inf}}
    \cup {[o |-> o, kind |-> "set_period", a |-> a, b |-> Inf, v |-> nops + 1] : a \in Dates}

\* Stage one operation; a rejected operation fails the flush and is not staged.
Stage == /\ nops < MaxOps
         /\ Len(log) < MaxCommits
         /\ \E o \in Objs : \E op \in Ops(o) :
               /\ Allowed(Cur(o), op) /\ (op.kind = "set_period" => op.a < op.b)
               /\ pending' = Append(pending, op)
               /\ nops' = nops + 1
         /\ UNCHANGED <<log, index, answers, derivs, impacted>>

\* Commit: append to the log, update the index incrementally, compute impact sets,
\* and record the answers visible at the new head.
Commit == /\ pending # <<>>
          /\ LET t == Len(log) + 1
                 newIndex == [o \in Objs |-> Cur(o)]
                 hit == {d \in derivs :
                           \E i \in 1..Len(pending) :
                              /\ pending[i].o = d.o
                              /\ d.a \in Range(ApplyAll(index[d.o], d.o, SubSeq(pending, 1, i - 1)), pending[i])}
             IN /\ log' = Append(log, pending)
                /\ index' = newIndex
                /\ impacted' = impacted \cup {d.id : d \in hit}
                /\ answers' = answers \cup
                      {[t |-> t, o |-> o, a |-> a, v |-> ReadIn(newIndex[o], a)] : o \in Objs, a \in Dates}
          /\ pending' = <<>>
          /\ UNCHANGED <<nops, derivs>>

\* A derived result reads o at asof a at the current head.
Derive == /\ Cardinality(derivs) < MaxDerivs
          /\ pending = <<>>
          /\ \E o \in Objs, a \in Dates :
               /\ ReadIn(index[o], a) # None
               /\ derivs' = derivs \cup {[id |-> Cardinality(derivs) + 1, o |-> o, a |-> a,
                                          v |-> ReadIn(index[o], a)]}
          /\ UNCHANGED <<log, pending, nops, index, answers, impacted>>

Next == Stage \/ Commit \/ Derive

Spec == Init /\ [][Next]_vars

(* ---- properties ---- *)

TypeOK == /\ nops \in 0..MaxOps
          /\ Len(log) \in 0..MaxCommits

\* No overlapping segments, and every segment is a non-empty period, at every t.
NoOverlap ==
    \A o \in Objs, t \in 0..Len(log) :
        LET S == TL(o, t) IN
          /\ \A s \in S : s.f < s.t
          /\ \A s1, s2 \in S : s1 # s2 => (s1.t <= s2.f \/ s2.t <= s1.f)

\* The materialised index equals the timeline replayed from the log (AR-016).
IndexMatchesLog == \A o \in Objs : index[o] = TL(o, Len(log))

\* Answers at past (cseq, asof) never change.
PastAnswersStable == \A r \in answers : ReadAt(r.o, r.a, r.t) = r.v

\* The impact set contains every derived result whose input now resolves differently.
ImpactComplete == \A d \in derivs : ReadAt(d.o, d.a, Len(log)) # d.v => d.id \in impacted
===============================================================================
