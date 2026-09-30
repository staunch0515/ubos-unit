//! `ubos-store-suite` — the storage suite SUITE-DVM-STORE on PostgreSQL (VER-CONF-8402,
//! EXIT-1-03, DEC-049).
//!
//! Each case exercises one property of the storage kernel through the backend-neutral
//! traits (`StoreTx`, `StoreRead`), the engine, or a direct database session, and reports
//! pass or fail with its evidence:
//!
//! - transactions: rollback leaves nothing; head compare-and-set; a failing process in a
//!   commit group aborts alone (DSN-DVM-001, -006, -208);
//! - snapshot reads by commit ordinal (DSN-DVM-004, -008);
//! - index maintenance and the index verifier, including an injected corruption
//!   (DSN-DVM-005, -092);
//! - sequences: gap-free under injected flush failures, concurrent appends, child branches
//!   (DSN-DVM-402, -404);
//! - chunk placement and the physical form (DSN-DVM-002, -003);
//! - tenancy: row-level security, partitions, immutable ledger entries (DSN-DVM-010);
//! - state roots and stored tree nodes, including an injected corruption (DSN-DVM-007);
//! - inspection aid: the storage traits name no backend type (VER-CONF-8402 step 2).
//!
//! Usage: `ubos-store-suite --database "host=127.0.0.1 user=postgres dbname=bpa"
//!        [--appends 10000] [--report report.json]`

use serde_json::{Value, json};
use sqlx::{Connection, Executor, PgConnection, Row};
use std::sync::Arc;
use std::time::Instant;
use ubos_crypto::LocalKeys;
use ubos_dvm::verify::verify_roots;
use ubos_dvm::{Adapter, BpaError, Engine};
use ubos_store::kernel::{
    BranchRow, ChunkRow, LedgerRow, Placement, Scope, SequenceRequest, StoreRead, StoreTx,
    VersionRow, hash_of,
};
use ubos_store::physical::{Chunks, INLINE_MAX, LocalBlobs, Physical, chunk_id};
use ubos_store::verify::{verify_indexes, verify_sequences};
use ubos_store::{PgBackend, PgStore, key_hash, pg_options};

/// Genesis of the workload: the classes of a prototype vector with objects and ledgers.
const GENESIS: &str = include_str!("../../../../standard/vectors/ledger/VER-CONF-1505.json");
/// Source of the storage traits, for the inspection aid.
const KERNEL_SOURCE: &str = include_str!("../../../ubos-store/src/kernel.rs");

const TENANT_B: &str = "01900000-0000-7000-8000-0000000000b1";

type Outcome = Result<(bool, Value), BpaError>;

fn err(e: impl std::fmt::Display) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", e.to_string())
}

fn sha(s: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(s.as_bytes()).into()
}

fn oid(n: u32) -> String {
    format!("01890a5d-ac96-7{:03x}-8000-{:012x}", n % 4096, n)
}

struct Suite {
    url: String,
    rt: tokio::runtime::Runtime,
    adapter: Adapter,
    scope: Scope,
    backend: PgBackend,
    classes: Value,
    appends: u32,
    /// Current version of each object on `main`, for update bases (STD-TXN-020).
    heads: std::collections::HashMap<String, Value>,
}

impl Suite {
    fn owner(&self) -> Result<PgConnection, BpaError> {
        let o = pg_options(&self.url)?;
        self.rt
            .block_on(PgConnection::connect_with(&o))
            .map_err(err)
    }

    fn exec(&mut self, op: &str, input: Value) -> Result<Value, BpaError> {
        self.adapter.execute(op, &input)
    }

    /// Commit changes; updates get the current version as base.
    fn commit(&mut self, branch: &str, mut changes: Vec<Value>) -> Result<Value, BpaError> {
        for ch in &mut changes {
            if ch["op"] == "update" && ch.get("base").is_none() {
                let id = ch["object_id"].as_str().unwrap_or_default().to_string();
                ch["base"] = self
                    .heads
                    .get(&(branch.to_string() + id.as_str()))
                    .cloned()
                    .unwrap_or(Value::Null);
            }
        }
        let r = self.exec("commit", json!({"branch": branch, "changes": changes}))?;
        if let Some(v) = r["versions"].as_object() {
            for (id, h) in v {
                self.heads
                    .insert(branch.to_string() + id.as_str(), h.clone());
            }
        }
        Ok(r)
    }

    fn engine(&mut self) -> &mut Engine {
        self.adapter.engine_mut()
    }

    fn branch_id(&self, name: &str) -> Result<i64, BpaError> {
        self.rt
            .block_on(self.backend.branch(name))?
            .map(|b| b.branch_id)
            .ok_or_else(|| err(format!("no branch {name}")))
    }

    /// A kernel branch for storage-level cases.
    fn kernel_branch(&self, name: &str) -> Result<i64, BpaError> {
        let b = self.backend.clone();
        let row = BranchRow {
            branch_id: 0,
            name: name.into(),
            head_no: 0,
            head_cseq: 0,
            parent: None,
            depth: 0,
        };
        self.rt.block_on(async move {
            let mut tx = b.begin().await?;
            let id = tx.create_branch(&row).await?;
            tx.commit().await?;
            id.ok_or_else(|| err(format!("branch {} exists", row.name)))
        })
    }

    /// Objects, updates, appends, a child branch and a merge.
    fn workload(&mut self) -> Result<Value, BpaError> {
        self.exec("set_now", json!({"now": "2026-03-15T10:00:00.000000Z"}))?;
        self.exec("load_genesis", json!({"classes": self.classes}))?;
        for i in 0..40u32 {
            self.commit("main", vec![
                json!({"op": "create", "class": "Customer", "object_id": oid(i), "slug": format!("c-{i}"),
                       "body": {"name": format!("Customer {i}")}, "valid": {"from": "2026-01-01"}}),
                json!({"op": "append", "class": "StockMove", "entry_id": oid(1000 + i),
                       "ledger_key": {"item": format!("item-{}", i % 3)},
                       "body": {"item": format!("item-{}", i % 3), "quantity": "1"}, "valid_at": "2026-03-15"}),
            ])?;
        }
        for i in 0..20u32 {
            self.commit(
                "main",
                vec![json!({"op": "update", "object_id": oid(i),
                "body": {"name": format!("Customer {i} v2")}, "valid": {"from": "2026-02-01"}})],
            )?;
        }
        self.exec(
            "create_branch",
            json!({"from": "main", "name": "feature/x"}),
        )?;
        for i in 0..5u32 {
            let base = self.heads.get(&format!("main{}", oid(30 + i))).cloned();
            self.commit("feature/x", vec![
                json!({"op": "append", "class": "StockMove", "entry_id": oid(2000 + i),
                       "ledger_key": {"item": "item-0"}, "body": {"item": "item-0", "quantity": "2"}, "valid_at": "2026-03-15"}),
                json!({"op": "update", "object_id": oid(30 + i), "base": base,
                       "body": {"name": "branch edit"}, "valid": {"from": "2026-03-01"}}),
            ])?;
        }
        let m = self.exec("merge", json!({"source": "feature/x", "target": "main"}))?;
        for i in 0..5u32 {
            let r = self.exec(
                "read",
                json!({"branch": "main", "object_id": oid(30 + i), "asof": "2026-06-01"}),
            )?;
            if let Some(h) = r.get("version").or_else(|| r.get("head")) {
                self.heads.insert(format!("main{}", oid(30 + i)), h.clone());
            }
        }
        Ok(m)
    }
}

// ---- cases ----------------------------------------------------------------------------

fn tx_rollback(s: &mut Suite) -> Outcome {
    let b = s.backend.clone();
    let id = sha("suite/rollback");
    let seen = s.rt.block_on(async move {
        let mut tx = b.begin().await?;
        tx.put_versions(&[VersionRow {
            version_id: id,
            object_id: "rollback".into(),
            class: "Customer".into(),
            envelope: vec![1, 2, 3],
        }])
        .await?;
        tx.put_meta("suite/rollback", b"true").await?;
        tx.rollback().await?;
        Ok::<_, BpaError>((
            b.version(&id).await?.is_some(),
            b.meta("suite/rollback").await?.is_some(),
        ))
    })?;
    Ok((
        seen == (false, false),
        json!({"version_visible": seen.0, "meta_visible": seen.1}),
    ))
}

fn tx_head_cas(s: &mut Suite) -> Outcome {
    let bid = s.kernel_branch("suite/cas")?;
    let b = s.backend.clone();
    let (first, second) = s.rt.block_on(async move {
        let mut tx = b.begin().await?;
        let first = tx.cas_branch_head(bid, 0, 10, 1).await?;
        tx.commit().await?;
        let mut tx = b.begin().await?;
        let second = tx.cas_branch_head(bid, 0, 11, 1).await?;
        tx.rollback().await?;
        Ok::<_, BpaError>((first, second))
    })?;
    Ok((first && !second, json!({"first": first, "stale": second})))
}

fn group_abort_alone(s: &mut Suite) -> Outcome {
    let inputs: Vec<Value> = (0..5u32)
        .map(|i| {
            let id = oid(i + 5);
            let base = if i == 2 {
                json!("sha256:0000000000000000000000000000000000000000000000000000000000000000")
            } else {
                s.heads
                    .get(&format!("main{id}"))
                    .cloned()
                    .unwrap_or(Value::Null)
            };
            json!({"branch": "main", "changes": [{"op": "update", "object_id": id, "base": base,
                   "body": {"name": format!("group {i}")}, "valid": {"from": "2026-03-01"}}]})
        })
        .collect();
    let out = s.engine().commit_group(&inputs)?;
    for r in out.iter().flatten() {
        if let Some(v) = r["versions"].as_object() {
            for (id, h) in v {
                s.heads.insert(format!("main{id}"), h.clone());
            }
        }
    }
    let codes: Vec<Value> = out
        .iter()
        .map(|r| match r {
            Ok(_) => json!("ok"),
            Err(e) => json!(e.code),
        })
        .collect();
    let ok = out.iter().enumerate().all(|(i, r)| (i == 2) == r.is_err());
    Ok((ok, json!({"outcomes": codes})))
}

fn snapshot_reads(s: &mut Suite) -> Outcome {
    let id = oid(3000);
    let r1 = s.commit(
        "main",
        vec![
            json!({"op": "create", "class": "Customer", "object_id": id, "slug": "snap",
        "body": {"name": "first"}, "valid": {"from": "2026-01-01"}}),
        ],
    )?;
    let t1 = r1["cseq"].clone();
    s.commit(
        "main",
        vec![
            json!({"op": "update", "object_id": id, "body": {"name": "second"},
        "valid": {"from": "2026-01-01"}}),
        ],
    )?;
    let old = s.exec(
        "read",
        json!({"branch": "main", "object_id": id, "time": t1, "asof": "2026-06-01"}),
    )?;
    let new = s.exec(
        "read",
        json!({"branch": "main", "object_id": id, "asof": "2026-06-01"}),
    )?;
    let ok = old.to_string().contains("first") && new.to_string().contains("second");
    Ok((ok, json!({"at_first_commit": old, "at_head": new})))
}

fn indexes_verified(s: &mut Suite) -> Outcome {
    let mut out = serde_json::Map::new();
    let mut ok = true;
    for name in ["main", "feature/x"] {
        let bid = s.branch_id(name)?;
        let r =
            s.rt.block_on(verify_indexes(&s.backend, bid, 0, u64::MAX >> 1))?;
        ok &= r.findings.is_empty() && r.checked > 0;
        out.insert(name.into(), r.to_json());
    }
    Ok((ok, Value::Object(out)))
}

fn index_corruption(s: &mut Suite) -> Outcome {
    let bid = s.branch_id("main")?;
    let target = oid(7);
    let mut owner = s.owner()?;
    let (t, v) = (s.scope.tenant.clone(), s.scope.vae.clone());
    let row = s
        .rt
        .block_on(
            sqlx::query("SELECT valid_from, cseq_from, version_id FROM temporal_index WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND object_id = $4 AND cseq_to = 9223372036854775807 LIMIT 1")
                .bind(&t)
                .bind(&v)
                .bind(bid)
                .bind(&target)
                .fetch_one(&mut owner),
        )
        .map_err(err)?;
    let (from, cseq, saved): (String, i64, Vec<u8>) = (row.get(0), row.get(1), row.get(2));
    let set = |c: &mut PgConnection, ver: Vec<u8>| {
        s.rt.block_on(
            sqlx::query("UPDATE temporal_index SET version_id = $6 WHERE tenant_id = $1::uuid AND vae_id = $2::uuid AND branch_id = $3 AND object_id = $4 AND valid_from = $5 AND cseq_from = $7")
                .bind(&t)
                .bind(&v)
                .bind(bid)
                .bind(&target)
                .bind(&from)
                .bind(ver)
                .bind(cseq)
                .execute(c),
        )
        .map_err(err)
    };
    set(&mut owner, vec![0u8; 32])?;
    let r =
        s.rt.block_on(verify_indexes(&s.backend, bid, 0, u64::MAX >> 1));
    set(&mut owner, saved)?;
    let r = r?;
    let found = r
        .findings
        .iter()
        .any(|f| f.key == target && f.check == "temporal_index");
    let clean =
        s.rt.block_on(verify_indexes(&s.backend, bid, 0, u64::MAX >> 1))?
            .findings
            .is_empty();
    Ok((
        found && clean,
        json!({"injected": target, "report": r.to_json(), "clean_after_restore": clean}),
    ))
}

fn sequences_gap_free(s: &mut Suite) -> Outcome {
    let bid = s.kernel_branch("suite/sequences")?;
    let b = s.backend.clone();
    let n = s.appends;
    let t0 = Instant::now();
    let (committed, failed) = s.rt.block_on(async move {
        let (mut committed, mut failed) = (0u32, 0u32);
        for i in 0..n {
            let eh = sha(&format!("entry {i}"));
            let mut tx = b.begin().await?;
            tx.lock_branch("suite/sequences").await?;
            let a = tx
                .alloc_sequences(
                    bid,
                    &[SequenceRequest {
                        class: "StockMove".into(),
                        ledger_key: key_hash("{\"item\":\"seq\"}"),
                        entry_hashes: vec![eh],
                    }],
                )
                .await?;
            tx.put_entries(
                bid,
                &[LedgerRow {
                    class: "StockMove".into(),
                    ledger_key: key_hash("{\"item\":\"seq\"}"),
                    seq: a[0].first_seq,
                    entry_id: format!("seq-{i}"),
                    cseq: u64::from(i) + 1,
                    idx: 0,
                    prev_hash: a[0].prev_hash,
                    entry_hash: eh,
                    body: b"{}".to_vec(),
                }],
            )
            .await?;
            // 5% injected flush failures: the transaction is rolled back after allocation.
            if eh[0] < 13 {
                tx.rollback().await?;
                failed += 1;
            } else {
                tx.commit().await?;
                committed += 1;
            }
        }
        Ok::<_, BpaError>((committed, failed))
    })?;
    let r = s.rt.block_on(verify_sequences(&s.backend, bid))?;
    let ok = r.findings.is_empty() && r.checked == u64::from(committed) && failed > 0;
    Ok((
        ok,
        json!({"appends": n, "committed": committed, "injected_failures": failed,
                   "seconds": t0.elapsed().as_secs_f64(), "report": r.to_json()}),
    ))
}

fn sequences_concurrent(s: &mut Suite) -> Outcome {
    let bid = s.kernel_branch("suite/concurrent")?;
    let b = s.backend.clone();
    let (ok_count, r) = s.rt.block_on(async move {
        let mut set = tokio::task::JoinSet::new();
        for i in 0..200u32 {
            let b = b.clone();
            set.spawn(async move {
                let eh = sha(&format!("cash {i}"));
                let key = key_hash("{\"account\":\"cash\"}");
                let mut tx = b.begin().await?;
                tx.lock_branch("suite/concurrent").await?;
                let a = tx
                    .alloc_sequences(
                        bid,
                        &[SequenceRequest {
                            class: "GlEntry".into(),
                            ledger_key: key,
                            entry_hashes: vec![eh],
                        }],
                    )
                    .await?;
                tx.put_entries(
                    bid,
                    &[LedgerRow {
                        class: "GlEntry".into(),
                        ledger_key: key,
                        seq: a[0].first_seq,
                        entry_id: format!("cash-{i}"),
                        cseq: 1,
                        idx: i,
                        prev_hash: a[0].prev_hash,
                        entry_hash: eh,
                        body: b"{}".to_vec(),
                    }],
                )
                .await?;
                tx.commit().await
            });
        }
        let mut ok = 0u32;
        while let Some(r) = set.join_next().await {
            if matches!(r, Ok(Ok(()))) {
                ok += 1;
            }
        }
        Ok::<_, BpaError>((ok, verify_sequences(&b, bid).await?))
    })?;
    Ok((
        ok_count == 200 && r.findings.is_empty(),
        json!({"succeeded": ok_count, "report": r.to_json()}),
    ))
}

fn sequences_branches(s: &mut Suite) -> Outcome {
    let mut out = serde_json::Map::new();
    let mut ok = true;
    for name in ["main", "feature/x"] {
        let bid = s.branch_id(name)?;
        let r = s.rt.block_on(verify_sequences(&s.backend, bid))?;
        ok &= r.findings.is_empty() && r.checked > 0;
        out.insert(name.into(), r.to_json());
    }
    Ok((ok, Value::Object(out)))
}

fn physical_parts(tenant: &str) -> Result<Chunks, BpaError> {
    let keys = Arc::new(LocalKeys::from_env()?);
    let dir = std::env::temp_dir().join(format!("ubos-suite-blobs-{}", std::process::id()));
    Ok(Chunks::new(
        Physical::new(keys, tenant),
        Arc::new(LocalBlobs::new(dir)?),
    ))
}

fn noise(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

fn chunk_placement(s: &mut Suite) -> Outcome {
    let chunks = physical_parts(&s.scope.tenant)?;
    let small = b"a small chunk".to_vec();
    let large = noise(3 * 1024 * 1024, 7);
    let rows = vec![chunks.prepare(&small)?, chunks.prepare(&large)?];
    let b = s.backend.clone();
    let stored = rows.clone();
    let back = s.rt.block_on(async move {
        let mut tx = b.begin().await?;
        tx.put_chunks(&stored).await?;
        tx.commit().await?;
        let mut out = Vec::new();
        for r in &stored {
            out.push(
                b.chunk(&r.content_id)
                    .await?
                    .ok_or_else(|| err("chunk missing"))?,
            );
        }
        Ok::<_, BpaError>(out)
    })?;
    let small_ok = back[0].placement == Placement::Inline && chunks.read(&back[0])? == small;
    let large_ok = back[1].placement == Placement::ObjectStore
        && back[1].body.is_none()
        && chunks.read(&back[1])? == large;
    Ok((
        small_ok && large_ok,
        json!({"inline_max": INLINE_MAX, "small": format!("{:?}", back[0].placement),
        "large_bytes": large.len(), "large": format!("{:?}", back[1].placement), "large_body_in_database": back[1].body.is_some()}),
    ))
}

fn chunk_corruption(s: &mut Suite) -> Outcome {
    let chunks = physical_parts(&s.scope.tenant)?;
    let row = chunks.prepare(b"integrity probe")?;
    let mut bad = row.clone();
    if let Some(b) = bad.body.as_mut() {
        let last = b.len() - 1;
        b[last] ^= 0x01;
    }
    let e = chunks.read(&bad).err();
    let flagged = e.as_ref().is_some_and(|e| {
        e.code == "NOD.BLOB_UNAVAILABLE" && e.details == Some(json!({"integrity": true}))
    });
    let wrong_id = ChunkRow {
        content_id: chunk_id(b"other")?,
        ..row.clone()
    };
    let e2 = chunks.read(&wrong_id).err();
    let flagged2 = e2
        .as_ref()
        .is_some_and(|e| e.code == "NOD.BLOB_UNAVAILABLE");
    Ok((
        flagged && flagged2,
        json!({"modified_byte": e.map(|e| json!({"code": e.code, "details": e.details})),
        "moved_to_other_id": e2.map(|e| json!({"code": e.code, "details": e.details}))}),
    ))
}

fn two_tenants(s: &mut Suite) -> Outcome {
    let a = physical_parts(&s.scope.tenant)?;
    let b = physical_parts(TENANT_B)?;
    let content = b"identical logical content";
    let (ra, rb) = (a.prepare(content)?, b.prepare(content)?);
    let same_id = ra.content_id == rb.content_id;
    let different_bytes = ra.body != rb.body;
    let cross = a.read(&rb).is_err();
    Ok((
        same_id && different_bytes && cross,
        json!({"same_content_id": same_id, "stored_bytes_differ": different_bytes,
        "tenant_a_cannot_open_tenant_b": cross}),
    ))
}

fn version_sealed(s: &mut Suite) -> Outcome {
    let mut owner = s.owner()?;
    let target = oid(1);
    let t = s.scope.tenant.clone();
    let rows =
        s.rt.block_on(
            sqlx::query(
                "SELECT envelope FROM object_version WHERE tenant_id = $1::uuid AND object_id = $2",
            )
            .bind(&t)
            .bind(&target)
            .fetch_all(&mut owner),
        )
        .map_err(err)?;
    let plain = rows.iter().any(|r| {
        let b: Vec<u8> = r.get(0);
        b.windows(8).any(|w| w == b"Customer")
    });
    let read = s.exec(
        "read",
        json!({"branch": "main", "object_id": target, "asof": "2026-06-01"}),
    )?;
    Ok((
        !rows.is_empty() && !plain && read.to_string().contains("Customer 1"),
        json!({"stored_versions": rows.len(), "plaintext_visible": plain, "read_back": read}),
    ))
}

const TABLES: [&str; 15] = [
    "kernel_meta",
    "branch_ref",
    "commit",
    "commit_parent",
    "chunk",
    "object_version",
    "ledger_entry",
    "ledger_sequence",
    "merkle_node",
    "object_state",
    "head_index",
    "temporal_index",
    "derivation_edge",
    "idempotency_record",
    "outbox",
];

fn rls_no_tenant(s: &mut Suite) -> Outcome {
    let mut c = s.owner()?;
    let mut counts = serde_json::Map::new();
    let mut total_owner = 0i64;
    let mut visible = 0i64;
    s.rt.block_on(async {
        for t in TABLES {
            let n: i64 = sqlx::query(&format!("SELECT count(*) FROM {t}"))
                .fetch_one(&mut c)
                .await
                .map_err(err)?
                .get(0);
            total_owner += n;
        }
        c.execute("SET ROLE ubos_app").await.map_err(err)?;
        for t in TABLES {
            let n: i64 = sqlx::query(&format!("SELECT count(*) FROM {t}"))
                .fetch_one(&mut c)
                .await
                .map_err(err)?
                .get(0);
            visible += n;
            counts.insert(t.into(), json!(n));
        }
        Ok::<_, BpaError>(())
    })?;
    Ok((
        visible == 0 && total_owner > 0,
        json!({"rows_in_database": total_owner, "visible_without_tenant": counts}),
    ))
}

fn rls_cross_tenant(s: &mut Suite) -> Outcome {
    let mut c = s.owner()?;
    let (a, v) = (s.scope.tenant.clone(), s.scope.vae.clone());
    let (seen_by_b, rejected) = s.rt.block_on(async {
        c.execute(format!("SET ROLE ubos_app; SET ubos.tenant = '{TENANT_B}'").as_str()).await.map_err(err)?;
        let n: i64 = sqlx::query("SELECT count(*) FROM object_version WHERE tenant_id = $1::uuid")
            .bind(&a)
            .fetch_one(&mut c)
            .await
            .map_err(err)?
            .get(0);
        let r = sqlx::query("INSERT INTO kernel_meta (tenant_id, vae_id, key, value) VALUES ($1::uuid, $2::uuid, 'suite/cross', 'true')")
            .bind(&a)
            .bind(&v)
            .execute(&mut c)
            .await;
        Ok::<_, BpaError>((n, r.err().map(|e| e.to_string())))
    })?;
    Ok((
        seen_by_b == 0 && rejected.is_some(),
        json!({"tenant_a_rows_seen_by_b": seen_by_b, "write_for_other_tenant": rejected}),
    ))
}

fn ledger_immutable(s: &mut Suite) -> Outcome {
    let mut c = s.owner()?;
    let t = s.scope.tenant.clone();
    let (as_owner, as_app) = s.rt.block_on(async {
        let r1 = sqlx::query("DELETE FROM ledger_entry WHERE ctid IN (SELECT ctid FROM ledger_entry WHERE tenant_id = $1::uuid LIMIT 1)")
            .bind(&t)
            .execute(&mut c)
            .await;
        c.execute(format!("SET ROLE ubos_app; SET ubos.tenant = '{t}'").as_str()).await.map_err(err)?;
        let r2 = sqlx::query("UPDATE ledger_entry SET idx = idx WHERE tenant_id = $1::uuid").bind(&t).execute(&mut c).await;
        Ok::<_, BpaError>((r1.err().map(|e| e.to_string()), r2.err().map(|e| e.to_string())))
    })?;
    let ok = as_owner
        .as_deref()
        .is_some_and(|e| e.contains("LEDG.IMMUTABLE_ENTRY"))
        && as_app.is_some();
    Ok((
        ok,
        json!({"delete_as_owner": as_owner, "update_as_application": as_app}),
    ))
}

fn partitions(s: &mut Suite) -> Outcome {
    let mut c = s.owner()?;
    let mut out = serde_json::Map::new();
    let mut ok = true;
    s.rt.block_on(async {
        for t in [
            "chunk",
            "object_version",
            "ledger_entry",
            "temporal_index",
            "merkle_node",
        ] {
            let n: i64 =
                sqlx::query("SELECT count(*) FROM pg_inherits WHERE inhparent = $1::regclass")
                    .bind(t)
                    .fetch_one(&mut c)
                    .await
                    .map_err(err)?
                    .get(0);
            ok &= n == 64;
            out.insert(t.into(), json!(n));
        }
        Ok::<_, BpaError>(())
    })?;
    Ok((ok, Value::Object(out)))
}

fn fresh_engine(s: &Suite) -> Result<Engine, BpaError> {
    Engine::attach(
        Box::new(PgStore::connect_scope(&s.url, s.scope.clone())?),
        &s.classes,
    )
}

fn roots_verified(s: &mut Suite) -> Outcome {
    let mut e = fresh_engine(s)?;
    let mut out = serde_json::Map::new();
    let mut ok = true;
    for name in ["main", "feature/x"] {
        let r = verify_roots(&mut e, name, 0, u64::MAX >> 1)?;
        ok &= r.findings.is_empty() && r.checked > 0;
        out.insert(name.into(), r.to_json());
    }
    Ok((ok, Value::Object(out)))
}

fn root_corruption(s: &mut Suite) -> Outcome {
    let mut e = fresh_engine(s)?;
    let root = ubos_version::Repo::new(e.store.as_mut(), &e.registry).root_at("main", None)?;
    let mut c = s.owner()?;
    let t = s.scope.tenant.clone();
    let id = hash_of(&root)?;
    let saved: Vec<u8> =
        s.rt.block_on(
            sqlx::query("SELECT body FROM merkle_node WHERE tenant_id = $1::uuid AND node_id = $2")
                .bind(&t)
                .bind(&id[..])
                .fetch_one(&mut c),
        )
        .map_err(err)?
        .get(0);
    let mut node: Value = serde_json::from_slice(&saved).map_err(err)?;
    if let Some(first) = node["entries"].get_mut(0).and_then(|x| x.get_mut(0)) {
        *first = json!(format!("{}~", first.as_str().unwrap_or_default()));
    }
    let tampered = ubos_types::canonical(&node)?.into_bytes();
    let update = |c: &mut PgConnection, body: Vec<u8>| {
        s.rt.block_on(
            sqlx::query(
                "UPDATE merkle_node SET body = $3 WHERE tenant_id = $1::uuid AND node_id = $2",
            )
            .bind(&t)
            .bind(&id[..])
            .bind(body)
            .execute(c),
        )
        .map_err(err)
    };
    update(&mut c, tampered)?;
    let r = verify_roots(&mut fresh_engine(s)?, "main", 0, u64::MAX >> 1);
    update(&mut c, saved)?;
    let r = r?;
    let found = r
        .findings
        .iter()
        .any(|f| f.check == "node" && f.key == root);
    Ok((found, json!({"tampered_node": root, "report": r.to_json()})))
}

fn trait_inspection(_s: &mut Suite) -> Outcome {
    let forbidden = [
        "sqlx", "Pg", "postgres", "Postgres", "tokio", "Sqlite", "rusqlite",
    ];
    let mut hits = Vec::new();
    for (n, line) in KERNEL_SOURCE.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        for f in forbidden {
            if code.contains(f) {
                hits.push(json!({"line": n + 1, "token": f, "text": line.trim()}));
            }
        }
    }
    Ok((
        hits.is_empty(),
        json!({"file": "crates/ubos-store/src/kernel.rs", "forbidden_tokens": forbidden, "hits": hits}),
    ))
}

type Case = (&'static str, &'static str, fn(&mut Suite) -> Outcome);

const CASES: &[Case] = &[
    ("transactions.rollback", "DSN-DVM-001", tx_rollback),
    ("transactions.head_cas", "DSN-DVM-208", tx_head_cas),
    (
        "transactions.group_abort_alone",
        "DSN-DVM-006",
        group_abort_alone,
    ),
    ("snapshot.reads", "DSN-DVM-004, DSN-DVM-008", snapshot_reads),
    (
        "indexes.verified",
        "DSN-DVM-005, DSN-DVM-092",
        indexes_verified,
    ),
    (
        "indexes.corruption_detected",
        "DSN-DVM-092",
        index_corruption,
    ),
    (
        "sequences.gap_free_with_failures",
        "DSN-DVM-402",
        sequences_gap_free,
    ),
    (
        "sequences.concurrent_appends",
        "DSN-DVM-404",
        sequences_concurrent,
    ),
    ("sequences.branches", "DSN-DVM-402", sequences_branches),
    ("chunks.placement", "DSN-DVM-003", chunk_placement),
    (
        "chunks.corruption_detected",
        "DSN-DVM-002",
        chunk_corruption,
    ),
    ("physical.two_tenants", "DSN-DVM-002", two_tenants),
    ("physical.version_sealed", "DSN-DVM-002", version_sealed),
    ("tenancy.no_tenant", "DSN-DVM-010", rls_no_tenant),
    ("tenancy.cross_tenant", "DSN-DVM-010", rls_cross_tenant),
    (
        "tenancy.ledger_immutable",
        "DSN-DVM-010, STD-KIND-020",
        ledger_immutable,
    ),
    ("tenancy.partitions", "DSN-DVM-010", partitions),
    ("roots.verified", "DSN-DVM-007, DSN-DVM-092", roots_verified),
    ("roots.corruption_detected", "DSN-DVM-092", root_corruption),
    (
        "inspection.backend_neutral_traits",
        "DSN-DVM-001, VER-CONF-8402",
        trait_inspection,
    ),
];

fn run() -> Result<bool, BpaError> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let url = arg("--database").unwrap_or_else(|| "host=127.0.0.1 user=postgres dbname=bpa".into());
    let appends: u32 = arg("--appends")
        .and_then(|a| a.parse().ok())
        .unwrap_or(10_000);
    let genesis: Value = serde_json::from_str(GENESIS).map_err(err)?;
    let classes = genesis["setup"]
        .as_array()
        .and_then(|s| s.iter().find(|x| x["op"] == "load_genesis"))
        .map(|x| x["input"]["classes"].clone())
        .ok_or_else(|| err("genesis vector has no load_genesis"))?;
    let mut adapter = Adapter::postgres(&url)?;
    let scope = adapter
        .engine_mut()
        .store
        .scope()
        .ok_or_else(|| err("the adapter's store has no scope"))?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(err)?;
    let backend = rt.block_on(PgBackend::connect(&url, scope.clone()))?;
    let mut s = Suite {
        url,
        rt,
        adapter,
        scope,
        backend,
        classes,
        appends,
        heads: Default::default(),
    };
    let t0 = Instant::now();
    let workload = s.workload()?;
    let mut results = Vec::new();
    let mut failed = 0;
    for (id, verifies, f) in CASES {
        let t = Instant::now();
        let (pass, evidence) = match f(&mut s) {
            Ok(r) => r,
            Err(e) => (false, json!({"error": e.to_string()})),
        };
        if !pass {
            failed += 1;
        }
        eprintln!("{} {id}", if pass { "pass" } else { "FAIL" });
        results.push(json!({"case": id, "verifies": verifies, "result": if pass { "pass" } else { "fail" },
                            "seconds": (t.elapsed().as_secs_f64() * 1000.0).round() / 1000.0, "evidence": evidence}));
    }
    let server: String =
        s.rt.block_on(
            sqlx::query("SELECT current_setting('server_version')").fetch_one(s.backend.pool()),
        )
        .map_err(err)?
        .get(0);
    let report = json!({
        "suite": "SUITE-DVM-STORE", "item": "VER-CONF-8402", "criterion": "EXIT-1-03",
        "backend": "postgresql", "server_version": server, "scope": {"tenant": s.scope.tenant, "vae": s.scope.vae},
        "workload": {"merge": workload},
        "summary": {"cases": CASES.len(), "passed": CASES.len() - failed, "failed": failed,
                    "seconds": (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0},
        "cases": results,
    });
    let text = serde_json::to_string_pretty(&report).map_err(err)?;
    match arg("--report") {
        Some(p) => std::fs::write(p, text + "\n").map_err(err)?,
        None => println!("{text}"),
    }
    eprintln!(
        "storage suite: {} of {} cases passed",
        CASES.len() - failed,
        CASES.len()
    );
    Ok(failed == 0)
}

fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}
