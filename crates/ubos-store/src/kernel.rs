//! Backend-neutral storage traits of the kernel (DSN-DVM-001, DEC-047).
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
    /// Head commit (engine identifier, transition column of DEC-047).
    pub head_no: u64,
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
    /// Engine identifier (transition key, DEC-047).
    pub commit_no: u64,
    /// Content ID of `body`.
    pub commit_id: Hash,
    /// Ordinal on the branch.
    pub cseq: u64,
    /// State root after the commit (STD-PROOF-001).
    pub state_root: Hash,
    /// Parent commits in order; the first is the previous head.
    pub parents: Vec<u64>,
    /// Canonical commit record.
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
    /// Object identifier.
    pub object_id: String,
    /// Class name (transition column, DEC-047).
    pub class: String,
    /// Canonical stored record.
    pub envelope: Vec<u8>,
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
    /// Entry identifier.
    pub entry_id: String,
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

/// The engine's state of one object after a commit (transition table `object_state`,
/// DEC-047) and the kernel index rows derived from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRow {
    /// Object identifier.
    pub object_id: String,
    /// Class name.
    pub class: String,
    /// Canonical state, or `None` when the object no longer exists on the branch.
    pub state: Option<Vec<u8>>,
    /// Head version (`head_index`).
    pub head: Option<Hash>,
    /// Valid-time segments `(from, to, version)` (`temporal_index`); `to` `None` is open.
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
    pub commit_no: Option<u64>,
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
        head_cseq: u64,
    ) -> Result<bool, BpaError>;
    /// Close the open rows of the objects and write their new rows at `cseq`
    /// (`object_state`, `head_index`, `temporal_index`; DSN-DVM-005).
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
    /// Append outbox rows of a commit (STD-TXN-070).
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
    /// The object row of the branch visible at `cseq`: `Some(state or None)` when present.
    async fn object(
        &self,
        branch_id: i64,
        cseq: u64,
        object_id: &str,
    ) -> Result<Option<Option<Vec<u8>>>, BpaError>;
    /// All object rows of the branch visible at `cseq`, optionally of one class.
    async fn objects(
        &self,
        branch_id: i64,
        cseq: u64,
        class: Option<&str>,
    ) -> Result<Vec<(String, Option<Vec<u8>>)>, BpaError>;
    /// Ledger entries of the branch up to `cseq` in append order.
    async fn entries(
        &self,
        branch_id: i64,
        cseq: u64,
        filter: &EntryFilter,
    ) -> Result<Vec<Vec<u8>>, BpaError>;
    /// A setting of the adapter.
    async fn meta(&self, key: &str) -> Result<Option<Vec<u8>>, BpaError>;
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
