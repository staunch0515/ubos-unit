----------------------------- MODULE group_commit -----------------------------
(***************************************************************************)
(* Group commit with ledger sequences on one branch (VER-FORM-0004).      *)
(*                                                                         *)
(* Verifies: DSN-DVM-006, DSN-DVM-402, DSN-DVM-404, DSN-DVM-424,           *)
(*           STD-TXN-021, STD-TXN-024, STD-KIND-022.                        *)
(*                                                                         *)
(* The flush has two phases (W-8). Preparation runs in parallel and        *)
(* without a lock: a process takes a snapshot, computes its object writes  *)
(* with their bases and its ledger appends (key and delta, no sequence).   *)
(* Sequencing runs serially under the branch lock: the node closes its     *)
(* batch, validates each process against its view (storage head plus the   *)
(* commits accepted before it in the batch), allocates ledger sequences    *)
(* from the counter row, evaluates the balance invariant (balance >= 0) on *)
(* the balance after the earlier deltas of the batch, and aborts only the  *)
(* offending process. The batch is then written by one storage transaction *)
(* (commits, counter rows, tree update), which may fail or be lost with a  *)
(* node crash; a failed transaction leaves storage unchanged and the batch *)
(* is sequenced again.                                                     *)
(*                                                                         *)
(* An object version is the cseq of the commit that wrote it; 0 is the     *)
(* initial version. The tree root is a function of the committed state, so *)
(* serial equivalence of the committed history covers the tree update.     *)
(*                                                                         *)
(* Mutation selects a deliberately wrong variant for the sensitivity       *)
(* checks of reports/README.md; "none" is the design.                      *)
(***************************************************************************)
EXTENDS Integers, Sequences, FiniteSets, TLC

CONSTANTS Procs,        \* processes
          Nodes,        \* nodes running a sequencer
          Objs,         \* objects (compare-and-set)
          Keys,         \* ledger keys (append-only)
          Amount,       \* an append deposits or withdraws this amount
          InitBal,      \* initial balance of every key
          BatchMax,     \* largest batch (DSN-DVM-006 default 64)
          MaxCommits,   \* bound on commits
          MaxTxns,      \* bound on transactions begun (committed or not)
          MaxAttempts,  \* attempts per transaction (1 + retries)
          MaxFailures,  \* bound on failed storage transactions
          Mutation      \* "none" or a mutation of reports/README.md

Deltas == {-Amount, Amount}

ASSUME Mutation \in {"none", "no_rollback", "seq_from_storage", "pre_batch_balance",
                     "no_batch_view", "appends_cas", "whole_batch_abort", "partial_install"}

VARIABLES hist,     \* storage: hist[i] is the commit with cseq i
          head,     \* storage: branch head
          ctr,      \* storage: ledger_sequence counter row per key
          lock,     \* storage: node holding the branch lock, or "none"
          fails,    \* number of failed storage transactions so far
          pc,       \* process: "idle", "prepared", "queued", "staged", "retry"
          want,     \* process: [w |-> objects, a |-> set of <<key, delta>>]
          tid,      \* process: number of its current transaction
          snap,     \* process: head at its snapshot
          ws,       \* process: set of <<object, base version>>
          kbase,    \* process: counter per key at its snapshot (used by "appends_cas" only)
          attempts, \* process: attempts of this transaction
          queue,    \* node: processes waiting for the next batch
          staged,   \* node: commits accepted in the batch being written
          okJust,   \* history: every abort so far was justified
          okAppend  \* history: no append-only process was aborted by a conflict

vars == <<hist, head, ctr, lock, fails, pc, want, tid, snap, ws, kbase, attempts, queue, staged, okJust, okAppend>>

Symm == Permutations(Procs) \cup Permutations(Nodes) \cup Permutations(Objs) \cup Permutations(Keys)

Max(X) == CHOOSE x \in X : \A y \in X : y <= x

Range(s) == {s[i] : i \in 1..Len(s)}

WObjs(c) == {w[1] : w \in c.writes}

\* Delta of commit c on key k (a transaction appends at most once per key).
DeltaOf(c, k) == IF \E a \in c.apps : a.k = k THEN (CHOOSE a \in c.apps : a.k = k).d ELSE 0

HasKey(c, k) == \E a \in c.apps : a.k = k

\* Committed state after the first j commits, computed independently of the sequencer.
VerAt(o, j) == Max({0} \cup {i \in 1..j : o \in WObjs(hist[i])})

RECURSIVE BalAt(_, _)
BalAt(k, j) == IF j = 0 THEN InitBal ELSE BalAt(k, j - 1) + DeltaOf(hist[j], k)

CntAt(k, j) == Cardinality({i \in 1..j : HasKey(hist[i], k)})

RECURSIVE TidSum(_)
TidSum(S) == IF S = {} THEN 0 ELSE LET p == CHOOSE q \in S : TRUE IN tid[p] + TidSum(S \ {p})

InFlight == Cardinality({p \in Procs : pc[p] \in {"prepared", "queued", "staged", "retry"}})

Init == /\ hist = <<>>
        /\ head = 0
        /\ ctr = [k \in Keys |-> 0]
        /\ lock = "none"
        /\ fails = 0
        /\ pc = [p \in Procs |-> "idle"]
        /\ want = [p \in Procs |-> [w |-> {}, a |-> {}]]
        /\ tid = [p \in Procs |-> 0]
        /\ snap = [p \in Procs |-> 0]
        /\ ws = [p \in Procs |-> {}]
        /\ kbase = [p \in Procs |-> [k \in Keys |-> 0]]
        /\ attempts = [p \in Procs |-> 0]
        /\ queue = [n \in Nodes |-> <<>>]
        /\ staged = [n \in Nodes |-> <<>>]
        /\ okJust = TRUE
        /\ okAppend = TRUE

\* Preparation (parallel, no lock): snapshot of the committed state.
Prepare(p, w) ==
    /\ snap' = [snap EXCEPT ![p] = head]
    /\ ws' = [ws EXCEPT ![p] = {<<o, VerAt(o, head)>> : o \in w.w}]
    /\ kbase' = [kbase EXCEPT ![p] = [k \in Keys |-> ctr[k]]]
    /\ pc' = [pc EXCEPT ![p] = "prepared"]

\* A new transaction: objects it writes and appends it makes.
Begin(p) ==
    /\ pc[p] = "idle"
    /\ Len(hist) + InFlight < MaxCommits
    /\ TidSum(Procs) < MaxTxns
    /\ \E W \in SUBSET Objs, K \in SUBSET Keys :
         /\ W \cup K # {}
         /\ \E f \in [K -> Deltas] :
              LET w == [w |-> W, a |-> {<<k, f[k]>> : k \in K}]
              IN /\ want' = [want EXCEPT ![p] = w]
                 /\ Prepare(p, w)
    /\ tid' = [tid EXCEPT ![p] = @ + 1]
    /\ attempts' = [attempts EXCEPT ![p] = 1]
    /\ UNCHANGED <<hist, head, ctr, lock, fails, queue, staged, okJust, okAppend>>

\* Automatic retry from a fresh snapshot after a conflict (STD-TXN-023).
Retry(p) ==
    /\ pc[p] = "retry"
    /\ attempts[p] < MaxAttempts
    /\ Prepare(p, want[p])
    /\ attempts' = [attempts EXCEPT ![p] = @ + 1]
    /\ UNCHANGED <<hist, head, ctr, lock, fails, want, tid, queue, staged, okJust, okAppend>>

GiveUp(p) ==
    /\ pc[p] = "retry"
    /\ attempts[p] = MaxAttempts
    /\ pc' = [pc EXCEPT ![p] = "idle"]
    /\ UNCHANGED <<hist, head, ctr, lock, fails, want, tid, snap, ws, kbase, attempts, queue, staged, okJust, okAppend>>

\* A prepared process joins the next batch of node n.
Enqueue(p, n) ==
    /\ pc[p] = "prepared"
    /\ Len(queue[n]) < BatchMax
    /\ queue' = [queue EXCEPT ![n] = Append(@, p)]
    /\ pc' = [pc EXCEPT ![p] = "queued"]
    /\ UNCHANGED <<hist, head, ctr, lock, fails, want, tid, snap, ws, kbase, attempts, staged, okJust, okAppend>>

(* ---- sequencing (serial, under the branch lock) ---- *)

\* View of the sequencer: storage plus the commits accepted so far in the batch (acc).
ViewVer(o, acc) ==
    LET mine == {i \in 1..Len(acc) : o \in WObjs(acc[i])}
    IN IF mine = {} \/ Mutation = "no_batch_view" THEN VerAt(o, head) ELSE head + Max(mine)

ViewCnt(k, acc) ==
    ctr[k] + (IF Mutation = "seq_from_storage" THEN 0
              ELSE Cardinality({i \in 1..Len(acc) : HasKey(acc[i], k)}))

RECURSIVE AccDelta(_, _)
AccDelta(k, acc) == IF acc = <<>> THEN 0 ELSE DeltaOf(Head(acc), k) + AccDelta(k, Tail(acc))

ViewBal(k, acc) == BalAt(k, head) + (IF Mutation = "pre_batch_balance" THEN 0 ELSE AccDelta(k, acc))

\* Validate process p against the view, then accept it or abort it alone.
SeqOne(p, st) ==
    LET acc == st.acc
        stale == {x[1] : x \in {y \in ws[p] : ViewVer(y[1], acc) # y[2]}}
                 \cup (IF Mutation = "appends_cas"
                       THEN {a[1] : a \in {b \in want[p].a : ViewCnt(b[1], acc) # kbase[p][b[1]]}}
                       ELSE {})
        short == {a[1] : a \in {b \in want[p].a : ViewBal(b[1], acc) + b[2] < 0}}
        \* a conflict is justified only by a concurrent writer of one of its objects
        writer(o) == (\E i \in (snap[p] + 1)..head : o \in WObjs(hist[i]))
                     \/ (\E i \in 1..Len(acc) : o \in WObjs(acc[i]))
        \* an invariant abort is justified only by the true balance after the earlier deltas
        truly_short(k) == \E a \in want[p].a : a[1] = k /\ BalAt(k, head) + AccDelta(k, acc) + a[2] < 0
        commit == [cseq |-> 0, proc |-> p, tid |-> tid[p], req |-> want[p], writes |-> ws[p],
                   apps |-> {[k |-> a[1], d |-> a[2], seq |-> ViewCnt(a[1], acc) + 1] : a \in want[p].a}]
    IN IF stale # {}
       THEN [acc |-> acc, retry |-> st.retry \cup {p}, failed |-> st.failed,
             ab |-> st.ab \cup {[p |-> p, reason |-> "conflict", appendOnly |-> want[p].w = {},
                                 justified |-> \A o \in stale : o \in Objs /\ writer(o)]}]
       ELSE IF short # {}
       THEN [acc |-> acc, retry |-> st.retry, failed |-> st.failed \cup {p},
             ab |-> st.ab \cup {[p |-> p, reason |-> "invariant", appendOnly |-> want[p].w = {},
                                 justified |-> \A k \in short : truly_short(k)]}]
       ELSE [acc |-> Append(acc, commit), retry |-> st.retry, failed |-> st.failed, ab |-> st.ab]

RECURSIVE SeqAll(_, _)
SeqAll(q, st) == IF q = <<>> THEN st ELSE SeqAll(Tail(q), SeqOne(Head(q), st))

\* Node n takes the branch lock, closes its batch and sequences it.
Sequence(n) ==
    /\ lock = "none"
    /\ queue[n] # <<>>
    /\ LET st0 == SeqAll(queue[n], [acc |-> <<>>, retry |-> {}, failed |-> {}, ab |-> {}])
           \* mutation: one failing process aborts the whole batch
           st == IF Mutation = "whole_batch_abort" /\ st0.ab # {} /\ st0.acc # <<>>
                 THEN [acc |-> <<>>, failed |-> st0.failed,
                       retry |-> st0.retry \cup {st0.acc[i].proc : i \in 1..Len(st0.acc)},
                       ab |-> st0.ab \cup {[p |-> st0.acc[i].proc, reason |-> "conflict", appendOnly |-> FALSE,
                                           justified |-> FALSE] : i \in 1..Len(st0.acc)}]
                 ELSE st0
           acc == st.acc
       IN /\ lock' = n
          /\ staged' = [staged EXCEPT ![n] = acc]
          /\ queue' = [queue EXCEPT ![n] = <<>>]
          /\ okJust' = (okJust /\ \A a \in st.ab : a.justified)
          /\ okAppend' = (okAppend /\ \A a \in st.ab : a.appendOnly => a.reason = "invariant")
          /\ pc' = [p \in Procs |-> IF p \in st.retry THEN "retry"
                                    ELSE IF p \in st.failed THEN "idle"
                                    ELSE IF \E i \in 1..Len(acc) : acc[i].proc = p THEN "staged"
                                    ELSE pc[p]]
    /\ UNCHANGED <<hist, head, ctr, fails, want, tid, snap, ws, kbase, attempts>>

Count(k, acc) == Cardinality({i \in 1..Len(acc) : HasKey(acc[i], k)})

\* The storage transaction of the batch commits: commits, counter rows and tree update.
Install(n) ==
    /\ lock = n
    /\ LET acc == staged[n] IN
         /\ hist' = hist \o [i \in 1..Len(acc) |->
                     IF Mutation = "partial_install" /\ i = Len(acc)
                     THEN [acc[i] EXCEPT !.cseq = head + i, !.writes = {}]
                     ELSE [acc[i] EXCEPT !.cseq = head + i]]
         /\ head' = head + Len(acc)
         /\ ctr' = [k \in Keys |-> ctr[k] + Count(k, acc)]
         /\ pc' = [p \in Procs |-> IF \E i \in 1..Len(acc) : acc[i].proc = p THEN "idle" ELSE pc[p]]
    /\ lock' = "none"
    /\ staged' = [staged EXCEPT ![n] = <<>>]
    /\ UNCHANGED <<fails, want, tid, snap, ws, kbase, attempts, queue, okJust, okAppend>>

\* The storage transaction fails or the node crashes: storage is unchanged, the lock is
\* released and the accepted processes are sequenced again in a later batch.
Fail(n) ==
    /\ lock = n
    /\ fails < MaxFailures
    /\ fails' = fails + 1
    /\ LET acc == staged[n] IN
         /\ ctr' = IF Mutation = "no_rollback" THEN [k \in Keys |-> ctr[k] + Count(k, acc)] ELSE ctr
         /\ pc' = [p \in Procs |-> IF \E i \in 1..Len(acc) : acc[i].proc = p THEN "prepared" ELSE pc[p]]
    /\ lock' = "none"
    /\ staged' = [staged EXCEPT ![n] = <<>>]
    /\ UNCHANGED <<hist, head, want, tid, snap, ws, kbase, attempts, queue, okJust, okAppend>>

Next == \/ \E p \in Procs : Begin(p) \/ Retry(p) \/ GiveUp(p)
        \/ \E p \in Procs, n \in Nodes : Enqueue(p, n)
        \/ \E n \in Nodes : Sequence(n) \/ Install(n) \/ Fail(n)

Spec == Init /\ [][Next]_vars

(* ---- properties ---- *)

TypeOK == /\ head \in 0..MaxCommits
          /\ Len(hist) <= MaxCommits
          /\ lock \in Nodes \cup {"none"}
          /\ pc \in [Procs -> {"idle", "prepared", "queued", "staged", "retry"}]

\* Dense cseq and a linear history (STD-VER-003).
DenseCseq == /\ head = Len(hist)
             /\ \A i \in 1..Len(hist) : hist[i].cseq = i

\* Gap-free sequences per key: the committed entries of k carry exactly 1..n, and the
\* counter row equals n, also after failed storage transactions (DSN-DVM-402).
GapFree == \A k \in Keys :
             LET es == {i \in 1..Len(hist) : HasKey(hist[i], k)}
                 seqs == {(CHOOSE a \in hist[i].apps : a.k = k).seq : i \in es}
             IN /\ seqs = 1..Cardinality(es)
                /\ ctr[k] = Cardinality(es)

\* Per-key total order: sequence order equals commit order (STD-KIND-022).
PerKeyOrder == \A k \in Keys, i, j \in 1..Len(hist) :
                 (i < j /\ HasKey(hist[i], k) /\ HasKey(hist[j], k))
                   => (CHOOSE a \in hist[i].apps : a.k = k).seq < (CHOOSE a \in hist[j].apps : a.k = k).seq

\* Batch atomicity per process: a transaction is committed at most once and then with
\* all of its writes and appends in one commit (DSN-DVM-006).
ProcessAtomic == /\ \A i \in 1..Len(hist) :
                      /\ WObjs(hist[i]) = hist[i].req.w
                      /\ {<<a.k, a.d>> : a \in hist[i].apps} = hist[i].req.a
                 /\ \A i, j \in 1..Len(hist) :
                      (i # j) => (hist[i].proc # hist[j].proc \/ hist[i].tid # hist[j].tid)

\* Serial equivalence: every commit is exactly what running its process alone at its
\* position in the history would produce: bases current, next sequence per key, balance
\* invariant satisfied. The parallel preparation therefore changes no result (W-8).
SerialEquivalent == \A i \in 1..Len(hist) :
                      /\ \A w \in hist[i].writes : VerAt(w[1], i - 1) = w[2]
                      /\ \A a \in hist[i].apps : /\ a.seq = CntAt(a.k, i - 1) + 1
                                                 /\ BalAt(a.k, i - 1) + a.d >= 0

\* Balance invariant on every committed state (DSN-DVM-424).
BalanceSafe == \A k \in Keys, j \in 0..Len(hist) : BalAt(k, j) >= 0

\* Appends take no part in compare-and-set: a process that only appends is aborted only by
\* the balance invariant (DSN-DVM-404, STD-TXN-024).
NoFalseConflict == okAppend

\* Only the offending process is aborted, and only for a real reason (STD-TXN-021, DSN-DVM-424).
FailuresJustified == okJust
===============================================================================
