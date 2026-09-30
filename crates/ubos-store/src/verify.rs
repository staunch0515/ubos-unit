//! Verifiers of the kernel indexes and ledger sequences (DSN-DVM-092, DSN-DVM-402).
//!
//! `verify_indexes` rebuilds `head_index` and `temporal_index` rows from the committed
//! object rows (the transition table `object_state`, DEC-047), compares order-independent
//! checksums per bucket of object identifiers, and reports the keys of every mismatching
//! row. `verify_sequences` checks that each ledger key's own entries on a branch carry dense
//! sequences after the inherited ones, that the hash chain links them, and that the counter
//! row agrees. Both use only [`StoreRead`], so they run unchanged on every backend.

use crate::kernel::{HeadRow, OPEN, StoreRead, TemporalRow, hash_text};
use crate::{ObjectRow, index_row};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use ubos_types::BpaError;

/// Buckets of object identifiers for the checksums.
pub const BUCKETS: usize = 16;

/// One mismatch found by a verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The verifier's check (`head_index`, `temporal_index`, `sequence`, `chain`, `counter`).
    pub check: &'static str,
    /// The affected key.
    pub key: String,
    /// What differs.
    pub detail: String,
}

/// The outcome of a verifier run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Rows or entries examined.
    pub checked: u64,
    /// Buckets whose checksums differed.
    pub buckets_differing: u64,
    /// Mismatches found.
    pub findings: Vec<Finding>,
}

impl Report {
    /// The report as JSON.
    pub fn to_json(&self) -> Value {
        json!({
            "checked": self.checked,
            "buckets_differing": self.buckets_differing,
            "findings": self.findings.iter().map(|f| json!({"check": f.check, "key": f.key, "detail": f.detail})).collect::<Vec<_>>(),
        })
    }
}

fn bucket(object_id: &str) -> usize {
    let h = Sha256::digest(object_id.as_bytes());
    h[0] as usize % BUCKETS
}

fn row_digest(row: &impl std::fmt::Debug) -> [u8; 32] {
    Sha256::digest(format!("{row:?}").as_bytes()).into()
}

/// Compare expected and stored rows by bucket checksums (XOR of row digests), then list
/// the differing rows of the buckets that disagree.
fn compare<T: Ord + Clone + std::fmt::Debug>(
    check: &'static str,
    expected: &[T],
    stored: &[T],
    id: impl Fn(&T) -> &str,
    report: &mut Report,
) {
    let mut sums = [[[0u8; 32]; 2]; BUCKETS];
    for (side, rows) in [expected, stored].iter().enumerate() {
        for r in rows.iter() {
            let d = row_digest(r);
            for (a, b) in sums[bucket(id(r))][side].iter_mut().zip(d) {
                *a ^= b;
            }
        }
    }
    report.checked += stored.len() as u64;
    for (b, [e, s]) in sums.iter().enumerate() {
        if e == s {
            continue;
        }
        report.buckets_differing += 1;
        let want: BTreeSet<&T> = expected.iter().filter(|r| bucket(id(r)) == b).collect();
        let have: BTreeSet<&T> = stored.iter().filter(|r| bucket(id(r)) == b).collect();
        for r in want.difference(&have) {
            report.findings.push(Finding {
                check,
                key: id(r).to_string(),
                detail: format!("missing or different: expected {r:?}"),
            });
        }
        for r in have.difference(&want) {
            report.findings.push(Finding {
                check,
                key: id(r).to_string(),
                detail: format!("unexpected row {r:?}"),
            });
        }
    }
}

/// Rebuild the kernel indexes of a branch for the object rows written with `cseq` in
/// `[from, to]` and compare them with the stored ones (DSN-DVM-092). `head_index` is
/// compared as a whole.
pub async fn verify_indexes<R: StoreRead>(
    store: &R,
    branch_id: i64,
    from: u64,
    to: u64,
) -> Result<Report, BpaError> {
    let mut report = Report::default();
    let bad = |e: serde_json::Error| BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string());
    let derive = |r: &crate::kernel::StateRow| -> Result<crate::kernel::IndexRow, BpaError> {
        let state = r
            .state
            .as_deref()
            .map(serde_json::from_slice::<Value>)
            .transpose()
            .map_err(bad)?;
        index_row(&ObjectRow {
            object_id: r.object_id.clone(),
            class: r.class.clone(),
            state,
        })
    };
    // temporal_index
    let mut expected = Vec::new();
    for r in store.state_rows(branch_id, from, to).await? {
        let ix = derive(&r)?;
        for (f, t, v) in ix.segments {
            expected.push(TemporalRow {
                object_id: r.object_id.clone(),
                valid_from: f,
                valid_to: t,
                cseq_from: r.cseq_from,
                cseq_to: r.cseq_to,
                version: v,
                class: r.class.clone(),
            });
        }
    }
    let stored = store.temporal_rows(branch_id, from, to).await?;
    compare(
        "temporal_index",
        &expected,
        &stored,
        |r| &r.object_id,
        &mut report,
    );
    // head_index
    let mut expected = Vec::new();
    for r in store.state_rows(branch_id, 0, OPEN).await? {
        if r.cseq_to != OPEN {
            continue;
        }
        let ix = derive(&r)?;
        expected.push(HeadRow {
            object_id: r.object_id.clone(),
            version: ix.head,
            class: r.class.clone(),
            hidden: r.state.is_none(),
        });
    }
    let stored = store.head_rows(branch_id).await?;
    compare(
        "head_index",
        &expected,
        &stored,
        |r| &r.object_id,
        &mut report,
    );
    Ok(report)
}

/// Check the ledger sequences of a branch (DSN-DVM-402): dense after the inherited
/// entries, linked by the hash chain, and matching the counter rows.
pub async fn verify_sequences<R: StoreRead>(store: &R, branch_id: i64) -> Result<Report, BpaError> {
    let mut report = Report::default();
    let mut keys: BTreeMap<(String, [u8; 32]), Vec<crate::kernel::EntryMeta>> = BTreeMap::new();
    for e in store.entry_meta(branch_id).await? {
        keys.entry((e.class.clone(), e.ledger_key))
            .or_default()
            .push(e);
    }
    let counters: BTreeMap<(String, [u8; 32]), crate::kernel::CounterRow> = store
        .counters(branch_id)
        .await?
        .into_iter()
        .map(|c| ((c.class.clone(), c.ledger_key), c))
        .collect();
    for ((class, key), entries) in &keys {
        let name = format!("{class}/{}", &hash_text(key)[7..19]);
        let (inherited, mut prev) = store.inherited(branch_id, class, key).await?;
        let mut next = inherited + 1;
        for e in entries {
            report.checked += 1;
            if e.seq != next {
                report.findings.push(Finding {
                    check: "sequence",
                    key: format!("{name}#{}", e.seq),
                    detail: format!("expected sequence {next}"),
                });
            }
            if e.prev_hash != prev {
                report.findings.push(Finding {
                    check: "chain",
                    key: format!("{name}#{}", e.seq),
                    detail: "previous hash does not link to the preceding entry".into(),
                });
            }
            next = e.seq + 1;
            prev = e.entry_hash;
        }
        match counters.get(&(class.clone(), *key)) {
            Some(c) if c.next_seq == next && c.last_hash == prev => {}
            Some(c) => report.findings.push(Finding {
                check: "counter",
                key: name.clone(),
                detail: format!(
                    "counter at {} (expected {next}) or last hash differs",
                    c.next_seq
                ),
            }),
            None => report.findings.push(Finding {
                check: "counter",
                key: name.clone(),
                detail: "no counter row".into(),
            }),
        }
    }
    for (class, key) in counters.keys() {
        if !keys.contains_key(&(class.clone(), *key)) {
            report.findings.push(Finding {
                check: "counter",
                key: format!("{class}/{}", &hash_text(key)[7..19]),
                detail: "counter row without entries".into(),
            });
        }
    }
    Ok(report)
}
