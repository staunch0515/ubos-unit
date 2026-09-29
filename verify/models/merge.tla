-------------------------------- MODULE merge --------------------------------
(***************************************************************************)
(* Three-way merge with ledger append (VER-FORM-0002).                    *)
(*                                                                         *)
(* Verifies: STD-VER-040, STD-VER-050, STD-VER-052, STD-VER-054,           *)
(*           STD-VER-056, STD-KIND-022, DSN-DVM-240, DSN-DVM-243,          *)
(*           DSN-DVM-244, DSN-DVM-249.                                      *)
(*                                                                         *)
(* Branches grow by change commits (update one field of a Definition      *)
(* object, end an object, append a ledger entry) and by merge commits.    *)
(* A commit is identified by <<branch, position>>; <<0, 0>> is the root.   *)
(* A written value is the identity of the commit that wrote it, so values  *)
(* are unique. The merge base follows STD-VER-040, including the virtual   *)
(* base for criss-cross histories. The merge engine marks conflicting      *)
(* fields; the properties compare it with the declarative rules.          *)
(*                                                                         *)
(* Scope limits (see report): no object creation or natural-key conflicts, *)
(* no field strategies other than three_way, no corrections (`corrects`),  *)
(* no period locks, no validation of the merged result (STD-VER-055).      *)
(***************************************************************************)
EXTENDS Naturals, Sequences, FiniteSets, TLC

CONSTANTS Branches,      \* branch numbers 1..n
          Objs,          \* Definition objects
          Fields,        \* fields of every object
          Keys,          \* ledger keys
          MaxPerBranch   \* bound on commits per branch

Root == <<0, 0>>
\* Markers are encoded like values (TLC compares only values of the same shape):
\* a field value is a commit identity; an object value maps every field to a value;
\* branch number 99 never occurs, so markers never equal a written value.
CONFLICT == <<99, 1>>                         \* field-level conflict marker
ENDED == [f \in Fields |-> <<99, 2>>]        \* the object is ended
OCONFLICT == [f \in Fields |-> <<99, 3>>]    \* object-level conflict marker

VARIABLES hist,     \* [Branches -> sequence of commits]
          failedOK  \* history: every failed merge so far passed its checks

vars == <<hist, failedOK>>

NoCheck == [complete |-> TRUE, symmetric |-> TRUE]   \* change commits carry no merge check

Symm == Permutations(Objs) \cup Permutations(Fields) \cup Permutations(Keys)

(* ---- commits and states ---- *)

RootState == [defs |-> [o \in Objs |-> [f \in Fields |-> Root]],
              led  |-> [k \in Keys |-> <<>>]]

C(id) == IF id = Root THEN [parents |-> <<>>, state |-> RootState, kind |-> "root", check |-> NoCheck]
         ELSE hist[id[1]][id[2]]

State(id) == C(id).state

HeadOf(b) == IF hist[b] = <<>> THEN Root ELSE <<b, Len(hist[b])>>

Ids == {Root} \cup UNION {{<<b, i>> : i \in 1..Len(hist[b])} : b \in Branches}

RECURSIVE Anc(_)
Anc(id) == {id} \cup UNION {Anc(p) : p \in {C(id).parents[i] : i \in DOMAIN C(id).parents}}

AncSet(X) == UNION {Anc(x) : x \in X}

\* Fixed total order used for the virtual base (stands for ascending commit_id).
Less(a, b) == a[1] < b[1] \/ (a[1] = b[1] /\ a[2] < b[2])

RECURSIVE SortIds(_)
SortIds(X) == IF X = {} THEN <<>>
              ELSE LET m == CHOOSE x \in X : \A y \in X \ {x} : Less(x, y)
                   IN <<m>> \o SortIds(X \ {m})

(* ---- three-way merge of Definition objects (STD-VER-050) ---- *)

FieldMerge(b, s, t) ==
    IF s = b THEN t ELSE IF t = b THEN s ELSE IF s = t THEN s ELSE CONFLICT

ObjMerge(B, S, T) ==
    IF S = ENDED /\ T = ENDED THEN ENDED
    ELSE IF S = ENDED THEN (IF T = B THEN ENDED ELSE OCONFLICT)
    ELSE IF T = ENDED THEN (IF S = B THEN ENDED ELSE OCONFLICT)
    ELSE IF B = ENDED THEN OCONFLICT   \* not reachable: an ended object is never changed
    ELSE [f \in Fields |-> FieldMerge(B[f], S[f], T[f])]

\* The engine: merged Definition part with conflict markers.
DefsEngine(B, S, T) == [o \in Objs |-> ObjMerge(B.defs[o], S.defs[o], T.defs[o])]

\* Reported conflicts: every marker, object-level or field-level (DSN-DVM-243).
Reported(D) == {<<o, "*">> : o \in {x \in Objs : D[x] = OCONFLICT}}
               \cup UNION {{<<o, f>> : f \in {g \in Fields : D[o][g] = CONFLICT}} :
                            o \in {x \in Objs : D[x] \notin {OCONFLICT, ENDED}}}

\* The declarative conflict set of STD-VER-050.
Expected(B, S, T) ==
    {<<o, "*">> : o \in {x \in Objs :
        \/ (S.defs[x] = ENDED /\ T.defs[x] # ENDED /\ T.defs[x] # B.defs[x])
        \/ (T.defs[x] = ENDED /\ S.defs[x] # ENDED /\ S.defs[x] # B.defs[x])}}
    \cup UNION {{<<o, f>> : f \in {g \in Fields : S.defs[o][g] # B.defs[o][g] /\ T.defs[o][g] # B.defs[o][g]
                                              /\ S.defs[o][g] # T.defs[o][g]}} :
                 o \in {x \in Objs : S.defs[x] # ENDED /\ T.defs[x] # ENDED}}

(* ---- ledger append on merge (STD-VER-054) ---- *)

EntryIds(q) == {q[i].id : i \in DOMAIN q}

LedgerMerge(S, T) ==
    [k \in Keys |->
        LET tq == T.led[k]
            new == SelectSeq(S.led[k], LAMBDA e : e.id \notin EntryIds(tq))
        IN tq \o [i \in 1..Len(new) |-> [id |-> new[i].id, seq |-> Len(tq) + i, src |-> new[i].seq]]]

(* ---- merge base (STD-VER-040) ---- *)

Best(X, Y) == LET CA == AncSet(X) \cap AncSet(Y)
              IN {c \in CA : ~\E d \in CA \ {c} : c \in Anc(d)}

\* A virtual merge keeps the base value where its own merge conflicts (see report, OQ-005).
Resolve(B, D) == [o \in Objs |->
    IF D[o] = OCONFLICT THEN B.defs[o]
    ELSE IF D[o] = ENDED THEN ENDED
    ELSE [f \in Fields |-> IF D[o][f] = CONFLICT THEN B.defs[o][f] ELSE D[o][f]]]

RECURSIVE BaseState(_, _), Fold(_, _, _, _)
Fold(tips, acc, rest, dummy) ==
    IF rest = <<>> THEN acc
    ELSE LET c == Head(rest)
             b == BaseState(tips, {c})
             m == [defs |-> Resolve(b, DefsEngine(b, acc, State(c))),
                   led  |-> LedgerMerge(State(c), acc)]
         IN Fold(tips \cup {c}, m, Tail(rest), dummy)
BaseState(X, Y) ==
    LET best == SortIds(Best(X, Y))
    IN IF Len(best) = 1 THEN State(best[1])
       ELSE Fold({best[1]}, State(best[1]), Tail(best), 0)

(* ---- behaviour ---- *)

Init == /\ hist = [b \in Branches |-> <<>>]
        /\ failedOK = NoCheck

AddCommit(b, c) == hist' = [hist EXCEPT ![b] = Append(@, c)]

NewId(b) == <<b, Len(hist[b]) + 1>>

Update(b, o, f) ==
    LET S == State(HeadOf(b)) IN
    /\ S.defs[o] # ENDED
    /\ AddCommit(b, [parents |-> <<HeadOf(b)>>, kind |-> "change", check |-> NoCheck,
                     state |-> [S EXCEPT !.defs[o][f] = NewId(b)]])

End(b, o) ==
    LET S == State(HeadOf(b)) IN
    /\ S.defs[o] # ENDED
    /\ AddCommit(b, [parents |-> <<HeadOf(b)>>, kind |-> "change", check |-> NoCheck,
                     state |-> [S EXCEPT !.defs[o] = ENDED]])

Append1(b, k) ==
    LET S == State(HeadOf(b)) IN
    AddCommit(b, [parents |-> <<HeadOf(b)>>, kind |-> "change", check |-> NoCheck,
                  state |-> [S EXCEPT !.led[k] = Append(@, [id |-> NewId(b), seq |-> Len(@) + 1, src |-> 0])]])

Change(b) ==
    /\ Len(hist[b]) < MaxPerBranch
    /\ \/ \E o \in Objs, f \in Fields : Update(b, o, f)
       \/ \E o \in Objs : End(b, o)
       \/ \E k \in Keys : Append1(b, k)
    /\ UNCHANGED failedOK

\* Merge source branch s into target branch t (DSN-DVM-240). The checks that concern a
\* single merge are evaluated once, when it happens, and kept with it (history variables):
\*   complete - the reported conflicts equal the declarative set of STD-VER-050;
\*   symmetric - base and result do not depend on the order of source and target.
Merge(s, t) ==
    LET sid == HeadOf(s)
        tid == HeadOf(t)
        B == BaseState({tid}, {sid})
        D == DefsEngine(B, State(sid), State(tid))
        got == Reported(D)
        complete == got = Expected(B, State(sid), State(tid))
        symmetric == /\ B = BaseState({sid}, {tid})
                     /\ Reported(DefsEngine(B, State(tid), State(sid))) = got
                     /\ (got = {} => DefsEngine(B, State(tid), State(sid)) = D)
    IN /\ s # t
       /\ Len(hist[t]) < MaxPerBranch
       /\ sid \notin Anc(tid)                     \* something to merge
       /\ IF got = {}
          THEN /\ AddCommit(t, [parents |-> <<tid, sid>>, kind |-> "merge",
                                check |-> [complete |-> complete, symmetric |-> symmetric],
                                state |-> [defs |-> D, led |-> LedgerMerge(State(sid), State(tid))]])
               /\ UNCHANGED failedOK
          ELSE /\ failedOK' = [complete |-> failedOK.complete /\ complete,
                                  symmetric |-> failedOK.symmetric /\ symmetric]
               /\ UNCHANGED hist

Next == \/ \E b \in Branches : Change(b)
        \/ \E s, t \in Branches : Merge(s, t)

Spec == Init /\ [][Next]_vars

(* ---- properties ---- *)

MergeCommits == UNION {{<<b, i>> : i \in {j \in 1..Len(hist[b]) : hist[b][j].kind = "merge"}} : b \in Branches}

TypeOK == \A b \in Branches : Len(hist[b]) <= MaxPerBranch

\* First parent of every commit is the previous commit of its branch; a merge commit
\* has exactly the parents [target, source] (STD-VER-003, STD-VER-056).
MergeParents ==
    \A b \in Branches : \A i \in 1..Len(hist[b]) :
        /\ hist[b][i].parents[1] = (IF i = 1 THEN Root ELSE <<b, i - 1>>)
        /\ Len(hist[b][i].parents) = (IF hist[b][i].kind = "merge" THEN 2 ELSE 1)

\* Gap-free sequences and no duplicated entries in every committed state (STD-KIND-022).
LedgerWellFormed ==
    \A id \in Ids : \A k \in Keys :
        LET q == State(id).led[k] IN
          /\ \A i \in DOMAIN q : q[i].seq = i
          /\ Cardinality(EntryIds(q)) = Len(q)

\* No lost entries: a merge keeps the target's entries in place and adds exactly the
\* source's missing entries, in source order (STD-VER-054).
NoLostEntries ==
    \A m \in MergeCommits :
        LET T == State(C(m).parents[1]) S == State(C(m).parents[2]) R == State(m) IN
        \A k \in Keys :
          /\ SubSeq(R.led[k], 1, Len(T.led[k])) = T.led[k]
          /\ EntryIds(R.led[k]) = EntryIds(T.led[k]) \cup EntryIds(S.led[k])
          /\ \A i, j \in (Len(T.led[k]) + 1)..Len(R.led[k]) : i < j => R.led[k][i].src < R.led[k][j].src

\* Failed merges report every conflict, and committed merges had none (DSN-DVM-243).
ConflictsComplete == /\ \A m \in MergeCommits : C(m).check.complete
                     /\ failedOK.complete

\* Determinism: the merge base does not depend on argument order, and the result and the
\* conflicts are symmetric in source and target (STD-VER-040, STD-VER-050).
Determinism == /\ \A m \in MergeCommits : C(m).check.symmetric
               /\ failedOK.symmetric
===============================================================================
