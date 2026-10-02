//! Verifiers of the kernel indexes and ledger sequences (DSN-DVM-092, DSN-DVM-402).
//!
//! `verify_indexes` rebuilds `head_index` and `temporal_index` rows from the write sets the
//! commit records carry (`objects`, DEC-053), compares order-independent checksums per
//! bucket of object identifiers, and reports the keys of every mismatching row. `verify_sequences` checks that each ledger key's own entries on a branch carry dense
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

/// Rebuild the kernel indexes of a branch from the write sets of its commit records
/// (`objects`, DEC-053) for the rows whose `cseq_from` lies in `[from, to]`, and compare
/// them with the stored `temporal_index` and `head_index` rows (DSN-DVM-092).
pub async fn verify_indexes<R: StoreRead>(
    store: &R,
    branch_id: i64,
    from: u64,
    to: u64,
) -> Result<Report, BpaError> {
    let mut report = Report::default();
    let bad = |e: serde_json::Error| BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string());
    // Replay the branch's commits in order: each write of an object closes its open rows.
    let mut heads: Vec<HeadRow> = Vec::new();
    let mut temporal: Vec<TemporalRow> = Vec::new();
    let mut open: BTreeMap<String, (usize, Vec<usize>)> = BTreeMap::new();
    for (cseq, body) in store.commit_bodies(branch_id).await? {
        let rec: Value = serde_json::from_slice(&body).map_err(bad)?;
        let rows: Vec<ObjectRow> = match rec.get("objects") {
            Some(v) => serde_json::from_value(v.clone()).map_err(bad)?,
            None => Vec::new(),
        };
        for o in &rows {
            if let Some((h, ts)) = open.remove(&o.object_id) {
                heads[h].cseq_to = cseq;
                for t in ts {
                    temporal[t].cseq_to = cseq;
                }
            }
            let ix = index_row(o)?;
            heads.push(HeadRow {
                object_id: o.object_id.clone(),
                cseq_from: cseq,
                cseq_to: OPEN,
                version: ix.head,
                class: o.class.clone(),
                kind: ix.kind,
                slug: ix.slug,
                instant: ix.instant,
                hidden: !ix.present,
            });
            let mut ts = Vec::new();
            for (f, t, v) in ix.segments {
                ts.push(temporal.len());
                temporal.push(TemporalRow {
                    object_id: o.object_id.clone(),
                    valid_from: f,
                    valid_to: t,
                    cseq_from: cseq,
                    cseq_to: OPEN,
                    version: v,
                    class: o.class.clone(),
                });
            }
            open.insert(o.object_id.clone(), (heads.len() - 1, ts));
        }
    }
    let in_range = |c: u64| c >= from && c <= to;
    let expected: Vec<TemporalRow> = temporal
        .into_iter()
        .filter(|r| in_range(r.cseq_from))
        .collect();
    let stored = store.temporal_rows(branch_id, from, to).await?;
    compare(
        "temporal_index",
        &expected,
        &stored,
        |r| &r.object_id,
        &mut report,
    );
    let expected: Vec<HeadRow> = heads
        .into_iter()
        .filter(|r| in_range(r.cseq_from))
        .collect();
    let stored = store.head_rows(branch_id, from, to).await?;
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
