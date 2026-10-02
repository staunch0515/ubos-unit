//! Backend-neutral storage traits of the kernel (DSN-DVM-001, DEC-047, DEC-053).
//!
//! [`StoreTx`] is one storage transaction with the write operations the DVM needs;
//! [`StoreRead`] holds the snapshot reads by commit ordinal. Both use only the row types of
//! this module (plain integers, strings, byte arrays), so that the SQLite adapter of PH-4
//! (DEC-024) implements them without a trait change. The operations are asynchronous; the
//! PostgreSQL adapter runs them on Tokio through sqlx (DEC-007).

use ubos_types::BpaError;

/// A 32-byte content ID (the `sha256:` prefix is implied, UBS-SYS-DVM-04 §1).
pub type Hash = [u8; 32];

/// Open end of a `cseq` interval (UBS-SYS-DVM-04 §1).
pub const OPEN: u64 = i64::MAX as u64;

/// Tenant and VAE a store works in (AR-017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// Tenant identifier (UUID text).
    pub tenant: String,
    /// VAE identifier (UUID text).
    pub vae: String,
}

/// A branch reference row (`branch_ref`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRow {
    /// Surrogate identifier within the VAE.
    pub branch_id: i64,
    /// Branch name.
    pub name: String,
    /// Head commit (the engine's internal handle, DEC-053).
    pub head_no: u64,
    /// Content ID of the head commit; `None` before the first commit.
    pub head_commit: Option<Hash>,
    /// Ordinal of the head commit.
    pub head_cseq: u64,
    /// Read-through parent and the ordinal it was forked at.
    pub parent: Option<(i64, u64)>,
    /// Nesting depth (0 for `main`).
    pub depth: u16,
}

/// A commit row (`commit`, `commit_parent`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRow {
    /// The engine's internal handle (a surrogate, not an ordinal; DEC-053).
    pub commit_no: u64,
    /// Commit identifier: content ID of the canonical commit record (STD-VER-001).
    pub commit_id: Hash,
    /// Ordinal on the branch.
    pub cseq: u64,
    /// Transaction time (canonical instant text); `None` for a genesis commit.
    pub tx_time: Option<String>,
    /// State root after the commit (STD-PROOF-001).
    pub state_root: Hash,
    /// Explicit process of the commit (UUID text), if any (DEC-051).
    pub process_id: Option<String>,
    /// Initiator of the commit (`did:ubos`), if recorded.
    pub author: Option<String>,
    /// Parent commits in order by handle; the first is the previous head. The store
    /// records them by content ID.
    pub parents: Vec<u64>,
    /// The engine's commit record (a superset of the canonical record).
    pub body: Vec<u8>,
}

/// Where a chunk's bytes are kept (DSN-DVM-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// In the `chunk` row.
    Inline,
    /// In the object store under `t/<tenant-hash>/c/<content-id>`.
    ObjectStore,
}

/// A chunk row (`chunk`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkRow {
    /// Content ID of the logical bytes.
    pub content_id: Hash,
    /// Logical size in bytes.
    pub size_logical: u32,
    /// Placement.
    pub placement: Placement,
    /// Stored bytes when inline.
    pub body: Option<Vec<u8>>,
}

/// A version row (`object_version`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRow {
    /// Content ID of the version.
    pub version_id: Hash,
    /// Object identifier (UUID text).
    pub object_id: String,
    /// Class name (classes are identified by name within a VAE, DEC-053).
    pub class: String,
    /// Content ID of the class declaration the version was written under.
    pub class_version: Hash,
    /// Kind code ([`kind_code`]).
    pub kind: u16,
    /// Valid-time start (canonical date or instant text).
    pub valid_from: String,
    /// Valid-time end, `None` when open.
    pub valid_to: Option<String>,
    /// Canonical stored record (physical form).
    pub envelope: Vec<u8>,
}

/// Kind code of a class kind (`object_version.kind`, `head_index.kind`).
pub fn kind_code(kind: &str) -> u16 {
    match kind {
        "definition" => 1,
        "ledger" => 2,
        "system" => 3,
        "raw_document" => 4,
        "inherit" => 5,
        _ => 0,
    }
}

/// Class kind of a kind code.
pub fn kind_name(code: u16) -> &'static str {
    match code {
        1 => "definition",
        2 => "ledger",
        3 => "system",
        4 => "raw_document",
        5 => "inherit",
        _ => "",
    }
}

/// A ledger entry to append (`ledger_entry`); its sequence comes from
/// [`StoreTx::alloc_sequences`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerRow {
    /// Ledger class.
    pub class: String,
    /// Hash of the canonical ledger key.
    pub ledger_key: Hash,
    /// Sequence within the key on the branch.
    pub seq: u64,
    /// Entry identifier (UUID text).
    pub entry_id: String,
    /// Business effect date or instant (canonical text).
    pub valid_at: String,
    /// Commit ordinal that made the entry visible.
    pub cseq: u64,
    /// Position in the commit (append order).
    pub idx: u32,
    /// Hash of the previous entry of the key (hash chain), zero for the first.
    pub prev_hash: Hash,
    /// Entry hash.
    pub entry_hash: Hash,
    /// Canonical entry record.
    pub body: Vec<u8>,
}

/// Appends of one ledger key within one flush, in order, for sequence allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceRequest {
    /// Ledger class.
    pub class: String,
    /// Hash of the canonical ledger key.
    pub ledger_key: Hash,
    /// Entry hashes of the appends in order.
    pub entry_hashes: Vec<Hash>,
}

/// Sequences allocated for one [`SequenceRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocated {
    /// Sequence of the first append; the others follow densely.
    pub first_seq: u64,
    /// Hash of the entry before the first append (zero when the key is new).
    pub prev_hash: Hash,
}

/// The kernel index rows of one object after a commit: its head row (`head_index`) and
/// its valid-time segments (`temporal_index`), from which the engine's object state is
/// rebuilt (DEC-053).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRow {
    /// Object identifier (UUID text).
    pub object_id: String,
    /// Class name.
    pub class: String,
    /// False when the object no longer exists on the branch (`hidden`).
    pub present: bool,
    /// Head version (`head_index`).
    pub head: Option<Hash>,
    /// Class kind.
    pub kind: String,
    /// Current slug.
    pub slug: String,
    /// Valid-time granularity of the class: true for `instant`, false for `date`.
    pub instant: bool,
    /// Valid-time segments `(from, to, version)` (`temporal_index`); `to` `None` is open.
    pub segments: Vec<(String, Option<String>, Hash)>,
}

/// The facts of one object at a commit ordinal, as read from the kernel indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectFacts {
    /// Class name.
    pub class: String,
    /// Class kind.
    pub kind: String,
    /// Current slug.
    pub slug: String,
    /// Head version, `None` when the branch holds no version.
    pub head: Option<Hash>,
    /// Valid-time segments `(from, to, version)` in valid-time order.
    pub segments: Vec<(String, Option<String>, Hash)>,
}

/// A tree node row (`merkle_node`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRow {
    /// Node hash.
    pub node_id: Hash,
    /// Level in the tree.
    pub level: u16,
    /// Stored node.
    pub body: Vec<u8>,
}

/// An outbox row (`outbox`, STD-TXN-070).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxRow {
    /// Commit ordinal.
    pub cseq: u64,
    /// Position among the commit's effects.
    pub idx: u32,
    /// Effect kind.
    pub kind: u16,
    /// Ordering key of the effect, if any.
    pub aggregate_key: Option<Vec<u8>>,
    /// Canonical payload.
    pub payload: Vec<u8>,
}

/// An idempotency record (`idempotency_record`, STD-TXN-060).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyRow {
    /// Scope of the key.
    pub scope: String,
    /// Idempotency key.
    pub key: String,
    /// Hash of the request.
    pub request_hash: Hash,
    /// Stored outcome.
    pub outcome: Option<Vec<u8>>,
    /// Commit of the outcome.
    pub commit_id: Option<Hash>,
}

/// A `head_index` row.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeadRow {
    /// Object identifier.
    pub object_id: String,
    /// First ordinal of the row.
    pub cseq_from: u64,
    /// End ordinal (exclusive), [`OPEN`] for the current row.
    pub cseq_to: u64,
    /// Head version.
    pub version: Option<Hash>,
    /// Class name.
    pub class: String,
    /// Class kind.
    pub kind: String,
    /// Slug.
    pub slug: String,
    /// Valid-time granularity: true for `instant`.
    pub instant: bool,
    /// Removed on the branch.
    pub hidden: bool,
}

/// A `temporal_index` row.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TemporalRow {
    /// Object identifier.
    pub object_id: String,
    /// Valid-time start.
    pub valid_from: String,
    /// Valid-time end, `None` when open.
    pub valid_to: Option<String>,
    /// First ordinal.
    pub cseq_from: u64,
    /// End ordinal (exclusive).
    pub cseq_to: u64,
    /// Version valid in the segment.
    pub version: Hash,
    /// Class name.
    pub class: String,
}

/// The sequence facts of one ledger entry, as read by the verifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryMeta {
    /// Ledger class.
    pub class: String,
    /// Hash of the canonical ledger key.
    pub ledger_key: Hash,
    /// Sequence.
    pub seq: u64,
    /// Entry identifier.
    pub entry_id: String,
    /// Previous hash of the chain.
    pub prev_hash: Hash,
    /// Entry hash.
    pub entry_hash: Hash,
}

/// A `ledger_sequence` counter row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterRow {
    /// Ledger class.
    pub class: String,
    /// Hash of the canonical ledger key.
    pub ledger_key: Hash,
    /// Next sequence to allocate.
    pub next_seq: u64,
    /// Hash of the last entry.
    pub last_hash: Hash,
}

/// Filter of a ledger read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryFilter {
    /// All entries of the branch.
    All,
    /// The entries of one ledger key.
    Key(String, Hash),
    /// The entries with one identifier.
    Id(String),
}

/// One storage transaction (DSN-DVM-001). Nothing is visible to other transactions before
/// [`StoreTx::commit`]; dropping or rolling back leaves storage unchanged.
#[allow(async_fn_in_trait)]
pub trait StoreTx {
    /// Lock a branch for the sequencer until the end of the transaction (DSN-DVM-006).
    async fn lock_branch(&mut self, name: &str) -> Result<Option<BranchRow>, BpaError>;
    /// Create a branch; `None` when the name exists. `branch_id` is assigned by the store.
    async fn create_branch(&mut self, row: &BranchRow) -> Result<Option<i64>, BpaError>;
    /// Store chunks (idempotent by content ID).
    async fn put_chunks(&mut self, rows: &[ChunkRow]) -> Result<(), BpaError>;
    /// Store versions (idempotent by content ID).
    async fn put_versions(&mut self, rows: &[VersionRow]) -> Result<(), BpaError>;
    /// Append ledger entries on a branch.
    async fn put_entries(&mut self, branch_id: i64, rows: &[LedgerRow]) -> Result<(), BpaError>;
    /// Store a commit with its parents.
    async fn put_commit(&mut self, branch_id: i64, row: &CommitRow) -> Result<(), BpaError>;
    /// Move a branch head; false when the head is not `expected` (DSN-DVM-208).
    async fn cas_branch_head(
        &mut self,
        branch_id: i64,
        expected: u64,
        head_no: u64,
        head_commit: &Hash,
        head_cseq: u64,
    ) -> Result<bool, BpaError>;
    /// Close the open rows of the objects and write their new rows at `cseq`
    /// (`head_index`, `temporal_index`; DSN-DVM-005).
    async fn upsert_index_rows(
        &mut self,
        branch_id: i64,
        cseq: u64,
        rows: &[IndexRow],
    ) -> Result<(), BpaError>;
    /// Store tree nodes (idempotent by hash).
    async fn put_tree_nodes(&mut self, rows: &[NodeRow]) -> Result<(), BpaError>;
    /// Allocate dense sequences per ledger key from the counter rows (DSN-DVM-402); the
    /// counters advance only if the transaction commits.
    async fn alloc_sequences(
        &mut self,
        branch_id: i64,
        requests: &[SequenceRequest],
    ) -> Result<Vec<Allocated>, BpaError>;
    /// Append outbox rows of a commit (STD-TXN-050).
    async fn outbox_append(&mut self, branch_id: i64, rows: &[OutboxRow]) -> Result<(), BpaError>;
    /// Store an idempotency record; false when the key exists.
    async fn idempotency_put(&mut self, row: &IdempotencyRow) -> Result<bool, BpaError>;
    /// Store a setting of the adapter.
    async fn put_meta(&mut self, key: &str, value: &[u8]) -> Result<(), BpaError>;
    /// Make the writes durable and visible.
    async fn commit(self) -> Result<(), BpaError>;
    /// Discard the writes.
    async fn rollback(self) -> Result<(), BpaError>;
}

/// Snapshot reads by commit ordinal (DSN-DVM-008). Reads of a branch see only its own rows
/// with `cseq_from <= cseq < cseq_to`; the caller resolves the read-through parent.
#[allow(async_fn_in_trait)]
pub trait StoreRead {
    /// A branch by name.
    async fn branch(&self, name: &str) -> Result<Option<BranchRow>, BpaError>;
    /// A commit by engine identifier: `(branch_id, cseq, body)`.
    async fn commit(&self, commit_no: u64) -> Result<Option<(i64, u64, Vec<u8>)>, BpaError>;
    /// The commit of a branch with the given ordinal.
    async fn commit_at(&self, branch_id: i64, cseq: u64) -> Result<Option<u64>, BpaError>;
    /// Fresh commit identifiers (not dense).
    async fn commit_numbers(&self, n: u32) -> Result<Vec<u64>, BpaError>;
    /// A version envelope (stored form).
    async fn version(&self, id: &Hash) -> Result<Option<Vec<u8>>, BpaError>;
    /// A chunk row (stored form, or its descriptor when placed in the object store).
    async fn chunk(&self, id: &Hash) -> Result<Option<ChunkRow>, BpaError>;
    /// A tree node.
    async fn node(&self, id: &Hash) -> Result<Option<Vec<u8>>, BpaError>;
    /// The object of the branch visible at `cseq`: `Some(facts, or None when removed)`
    /// when the branch has a row for it.
    async fn object(
        &self,
        branch_id: i64,
        cseq: u64,
        object_id: &str,
    ) -> Result<Option<Option<ObjectFacts>>, BpaError>;
    /// All objects of the branch visible at `cseq`, optionally of one class.
    async fn objects(
        &self,
        branch_id: i64,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<Vec<(String, Option<ObjectFacts>)>, BpaError>;
    /// Ledger entries of the branch up to `cseq` in append order.
    async fn entries(
        &self,
        branch_id: i64,
        cseq: u64,
        filter: &EntryFilter,
    ) -> Result<Vec<Vec<u8>>, BpaError>;
    /// A setting of the adapter.
    async fn meta(&self, key: &str) -> Result<Option<Vec<u8>>, BpaError>;
    /// The commits of a branch as `(commit_no, cseq)`, by ordinal.
    async fn branch_commits(&self, branch_id: i64) -> Result<Vec<(u64, u64)>, BpaError>;
    /// The commit records of a branch as `(cseq, body)`, by ordinal (verifiers).
    async fn commit_bodies(&self, branch_id: i64) -> Result<Vec<(u64, Vec<u8>)>, BpaError>;
    /// The `head_index` rows of a branch whose `cseq_from` lies in `[from, to]`.
    async fn head_rows(&self, branch_id: i64, from: u64, to: u64)
    -> Result<Vec<HeadRow>, BpaError>;
    /// The `temporal_index` rows of a branch whose `cseq_from` lies in `[from, to]`.
    async fn temporal_rows(
        &self,
        branch_id: i64,
        from: u64,
        to: u64,
    ) -> Result<Vec<TemporalRow>, BpaError>;
    /// The sequence facts of the branch's own entries, by class, key and sequence.
    async fn entry_meta(&self, branch_id: i64) -> Result<Vec<EntryMeta>, BpaError>;
    /// The counter rows of a branch.
    async fn counters(&self, branch_id: i64) -> Result<Vec<CounterRow>, BpaError>;
    /// Entries a branch inherits for one key through its read-through parents: their
    /// number and the hash of the last one (zero when none).
    async fn inherited(
        &self,
        branch_id: i64,
        class: &str,
        key: &Hash,
    ) -> Result<(u64, Hash), BpaError>;
}

/// Parse a `sha256:<hex>` content ID.
pub fn hash_of(id: &str) -> Result<Hash, BpaError> {
    let hex = id.strip_prefix("sha256:").unwrap_or(id);
    let bad = || BpaError::new("NOD.STORAGE_UNAVAILABLE", format!("malformed hash {id}"));
    if hex.len() != 64 {
        return Err(bad());
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| bad())?;
    }
    Ok(out)
}

/// Format a content ID as `sha256:<hex>`.
pub fn hash_text(h: &Hash) -> String {
    let mut s = String::with_capacity(71);
    s.push_str("sha256:");
    for b in h {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
