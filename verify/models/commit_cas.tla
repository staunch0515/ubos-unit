------------------------------ MODULE commit_cas ------------------------------
(***************************************************************************)
(* Commit and compare-and-set on one branch (VER-FORM-0001).              *)
(*                                                                         *)
(* Verifies: STD-TXN-020, STD-TXN-021, STD-TXN-022, STD-TXN-023,           *)
(*           STD-VER-003, DSN-DVM-208, DSN-DVM-515.                         *)
(*                                                                         *)
(* Processes take a snapshot of the branch head, read and write objects,   *)
(* and flush through any node. A node's sequencer validates each process's *)
(* bases (and, for serializable processes, its read set) against its view: *)
(* the head it read from storage plus the commits already in its group.    *)
(* A group is installed by one storage transaction whose head              *)
(* compare-and-set must see the head the node expected (DSN-DVM-208); on a *)
(* mismatch every process of the group returns to validation. A process    *)
(* whose validation fails is retried from a fresh snapshot (STD-TXN-023).  *)
(*                                                                         *)
(* An object version is identified by the cseq of the commit that wrote    *)
(* it; 0 is the initial version.                                           *)
(***************************************************************************)
EXTENDS Naturals, Sequences, FiniteSets, TLC

CONSTANTS Procs,         \* processes
          Nodes,         \* nodes running a sequencer
          Objs,          \* objects
          Serializable,  \* processes declared serializable (subset of Procs)
          MaxCommits,    \* bound on commits
          MaxAttempts    \* attempts per transaction (1 + retries)

ASSUME Serializable \subseteq Procs

VARIABLES hist,     \* storage: commit sequence; hist[i] is the commit with cseq i
          head,     \* storage: branch head (cseq of the last commit)
          pc,       \* process state: "idle", "working", "queued", "retry"
          want,     \* process: objects it writes and reads in this transaction
          snap,     \* process: head at its snapshot
          ws,       \* process: set of <<object, base version>>
          rs,       \* process: set of <<object, version read>>
          attempts, \* process: attempts of this transaction
          group,    \* node: [exp |-> expected head, procs |-> sequence of processes]
          aborts    \* history: [p, justified] for every failed validation

vars == <<hist, head, pc, want, snap, ws, rs, attempts, group, aborts>>

Symm == Permutations(Procs \ Serializable) \cup Permutations(Nodes) \cup Permutations(Objs)

Max(X) == CHOOSE x \in X : \A y \in X : y <= x

\* Version of o at cseq k in storage.
VerAt(o, k) == Max({0} \cup {i \in 1..k : \E w \in hist[i].writes : w[1] = o})

Writes(p) == {w[1] : w \in ws[p]}

InFlight == Cardinality({p \in Procs : pc[p] \in {"working", "queued", "retry"}})

Init == /\ hist = <<>>
        /\ head = 0
        /\ pc = [p \in Procs |-> "idle"]
        /\ want = [p \in Procs |-> [w |-> {}, r |-> {}]]
        /\ snap = [p \in Procs |-> 0]
        /\ ws = [p \in Procs |-> {}]
        /\ rs = [p \in Procs |-> {}]
        /\ attempts = [p \in Procs |-> 0]
        /\ group = [n \in Nodes |-> [exp |-> 0, procs |-> <<>>]]
        /\ aborts = {}

TakeSnapshot(p, w) ==
    /\ snap' = [snap EXCEPT ![p] = head]
    /\ ws' = [ws EXCEPT ![p] = {<<o, VerAt(o, head)>> : o \in w.w}]
    /\ rs' = [rs EXCEPT ![p] = {<<o, VerAt(o, head)>> : o \in w.r}]
    /\ pc' = [pc EXCEPT ![p] = "working"]

\* A new transaction: choose what it writes and reads, and take a snapshot.
Begin(p) ==
    /\ pc[p] = "idle"
    /\ Len(hist) + InFlight < MaxCommits
    /\ \E W \in SUBSET Objs \ {{}}, R \in SUBSET Objs :
         /\ (p \notin Serializable => R = {})
         /\ want' = [want EXCEPT ![p] = [w |-> W, r |-> R]]
         /\ TakeSnapshot(p, [w |-> W, r |-> R])
    /\ attempts' = [attempts EXCEPT ![p] = 1]
    /\ UNCHANGED <<hist, head, group, aborts>>

\* Automatic retry from a fresh snapshot (STD-TXN-023).
Retry(p) ==
    /\ pc[p] = "retry"
    /\ attempts[p] < MaxAttempts
    /\ TakeSnapshot(p, want[p])
    /\ attempts' = [attempts EXCEPT ![p] = @ + 1]
    /\ UNCHANGED <<hist, head, want, group, aborts>>

\* A retry budget exhausted: the transaction fails and the process is free again.
GiveUp(p) ==
    /\ pc[p] = "retry"
    /\ attempts[p] = MaxAttempts
    /\ pc' = [pc EXCEPT ![p] = "idle"]
    /\ UNCHANGED <<hist, head, want, snap, ws, rs, attempts, group, aborts>>

\* Flush through node n: validate against the node's view (STD-TXN-021, STD-TXN-022).
\* A node admits a process into its open group only if the group's expected head is not
\* older than the process's snapshot; otherwise the process waits for the next group.
\* Without this rule a stale group fails a flush whose objects did not change
\* (found by this model, see reports/commit_cas.md).
Flush(p, n) ==
    /\ pc[p] = "working"
    /\ LET g0 == IF group[n].procs = <<>> THEN [exp |-> head, procs |-> <<>>] ELSE group[n]
       IN snap[p] <= g0.exp     \* the group's view is not older than the snapshot (DSN-DVM-208)
    /\ LET g0 == IF group[n].procs = <<>> THEN [exp |-> head, procs |-> <<>>] ELSE group[n]
           view(o) == LET mine == {i \in 1..Len(g0.procs) : o \in Writes(g0.procs[i])}
                      IN IF mine = {} THEN VerAt(o, g0.exp) ELSE g0.exp + Max(mine)
           stale == {w[1] : w \in {x \in ws[p] : view(x[1]) # x[2]}}
           changed == IF p \in Serializable THEN {r[1] : r \in {x \in rs[p] : view(x[1]) # x[2]}} ELSE {}
           \* a failure is justified only by a concurrent writer of a named object
           writers(o) == (\E i \in (snap[p] + 1)..head : \E w \in hist[i].writes : w[1] = o)
                         \/ (\E i \in 1..Len(g0.procs) : o \in Writes(g0.procs[i]))
       IN IF stale = {} /\ changed = {}
          THEN /\ group' = [group EXCEPT ![n] = [exp |-> g0.exp, procs |-> Append(g0.procs, p)]]
               /\ pc' = [pc EXCEPT ![p] = "queued"]
               /\ UNCHANGED aborts
          ELSE /\ group' = [group EXCEPT ![n] = g0]
               /\ pc' = [pc EXCEPT ![p] = "retry"]
               /\ aborts' = aborts \cup {[p |-> p, justified |-> \A o \in stale \cup changed : writers(o)]}
    /\ UNCHANGED <<hist, head, want, snap, ws, rs, attempts>>

\* Install node n's group in one storage transaction with head compare-and-set.
Install(n) ==
    /\ group[n].procs # <<>>
    /\ LET g == group[n] IN
         IF head = g.exp
         THEN /\ hist' = hist \o [i \in 1..Len(g.procs) |->
                          [cseq |-> g.exp + i,
                           parent |-> g.exp + i - 1,
                           proc |-> g.procs[i],
                           node |-> n,
                           ser |-> g.procs[i] \in Serializable,
                           writes |-> ws[g.procs[i]],
                           reads |-> rs[g.procs[i]]]]
              /\ head' = g.exp + Len(g.procs)
              /\ pc' = [p \in Procs |-> IF \E i \in 1..Len(g.procs) : g.procs[i] = p THEN "idle" ELSE pc[p]]
         ELSE \* TXN.HEAD_MOVED: the sequencer re-validates against the new head
              /\ pc' = [p \in Procs |-> IF \E i \in 1..Len(g.procs) : g.procs[i] = p THEN "working" ELSE pc[p]]
              /\ UNCHANGED <<hist, head>>
    /\ group' = [group EXCEPT ![n] = [exp |-> 0, procs |-> <<>>]]
    /\ UNCHANGED <<want, snap, ws, rs, attempts, aborts>>

Next == \/ \E p \in Procs : Begin(p) \/ Retry(p) \/ GiveUp(p)
        \/ \E p \in Procs, n \in Nodes : Flush(p, n)
        \/ \E n \in Nodes : Install(n)

Spec == Init /\ [][Next]_vars

(* ---- properties ---- *)

TypeOK == /\ head \in 0..MaxCommits
          /\ Len(hist) <= MaxCommits
          /\ pc \in [Procs -> {"idle", "working", "queued", "retry"}]

\* Linear first-parent history: each commit's parent is the previous commit (STD-VER-003).
LinearHistory == /\ head = Len(hist)
                 /\ \A i \in 1..Len(hist) : hist[i].parent = i - 1

\* Dense cseq: the commits carry 1..n without gaps or repeats.
DenseCseq == \A i \in 1..Len(hist) : hist[i].cseq = i

\* No lost update: every write was based on the version current just before its commit.
NoLostUpdate == \A i \in 1..Len(hist) : \A w \in hist[i].writes : VerAt(w[1], i - 1) = w[2]

\* Serializable processes: every read is still current at the commit point, so the
\* transaction is equivalent to running alone at its position in the history.
SerializableReads == \A i \in 1..Len(hist) :
                        hist[i].ser => \A r \in hist[i].reads : VerAt(r[1], i - 1) = r[2]

\* The head moving for unrelated objects never fails a flush (STD-TXN-021).
FailuresJustified == \A a \in aborts : a.justified
===============================================================================
