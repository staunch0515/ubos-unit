//! Bulk load of a WL-GENERIC dataset into the PostgreSQL layout of PH-0 step 5b, and a
//! check that the bulk load equals the same history committed through the kernel path.
//!
//! The history is one commit with every creation (`cseq` 1), then one commit per month with
//! that month's changes (`supersede_from` at the first day of the month). Versions, object
//! rows, tree nodes and commit records are exactly those the commit path writes; `check`
//! proves it by comparing the state roots of every commit.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

use serde_json::{Map, Value, json};
use ubos_dvm::Engine;
use ubos_model::Registry;
use ubos_store::{BulkRows, MemStore, PgStore};
use ubos_tree::{MemNodes, Node};
use ubos_types::BpaError;
use ubos_version::{ObjectState, Repo, version_record};

use crate::workload::{self, Spec};

type Fields = HashMap<String, Vec<(String, String)>>;

/// Effective fields `(name, type)` of every generated class.
fn fields(reg: &Registry) -> Result<Fields, BpaError> {
    let mut out = HashMap::new();
    for c in 0..workload::CLASSES {
        let name = format!("Generic{c:03}");
        let f = reg
            .class(&name)?
            .fields
            .iter()
            .map(|(n, d)| (n.clone(), d["type"].as_str().unwrap_or("text").to_string()))
            .collect();
        out.insert(name, f);
    }
    Ok(out)
}

/// Everything one object contributes to the load.
struct ObjectLoad {
    versions: Vec<(String, Value)>,
    rows: Vec<(String, u64, Option<u64>, String, Value)>,
    initial: (String, String),
    changes: Vec<(u64, String, String)>,
}

/// The versions, index rows and tree changes of object `i`.
fn object(spec: &Spec, reg: &Registry, fields: &Fields, i: u64) -> Result<ObjectLoad, BpaError> {
    let (seed, oid, class, slug) = (
        spec.seed,
        workload::object_id(spec.seed, i),
        workload::class_of(spec.seed, i),
        workload::slug(i),
    );
    let f = &fields[&class];
    let key = format!("o/{oid}");
    let d0 = workload::month_date(0);
    let mut body = reg.check_body(
        &class,
        &Value::Object(workload::body(seed, i, 0, f, None)),
        false,
    )?;
    let (vh, rec) = version_record(reg, &oid, &class, &slug, &body, &d0, None, None, false)?;
    let timeline = ubos_time::apply(&[], "create", &vh, &d0, None)
        .map_err(|e| BpaError::new(e.0, "timeline"))?;
    let mut state = ObjectState {
        class: class.clone(),
        kind: "definition".into(),
        slug: slug.clone(),
        head: vh.clone(),
        timeline,
    };
    let mut out = ObjectLoad {
        versions: vec![(vh.clone(), rec)],
        rows: Vec::new(),
        initial: (key.clone(), vh),
        changes: Vec::new(),
    };
    let mut from = 1;
    for m in 1..=spec.months {
        if !workload::changes_in(seed, i, m) {
            continue;
        }
        let dm = workload::month_date(m);
        body = reg.check_body(
            &class,
            &Value::Object(workload::body(seed, i, m, f, Some(&body))),
            false,
        )?;
        let to = ubos_time::range(&state.timeline, "supersede_from", &dm, None).1;
        let (vh, rec) = version_record(
            reg,
            &oid,
            &class,
            &slug,
            &body,
            &dm,
            to.as_deref(),
            Some(&state.head),
            false,
        )?;
        let cseq = 1 + m;
        out.rows.push((
            oid.clone(),
            from,
            Some(cseq),
            class.clone(),
            serde_json::to_value(&state)
                .map_err(|e| BpaError::new("MODEL.INVALID_VALUE", e.to_string()))?,
        ));
        state.timeline = ubos_time::apply(&state.timeline, "supersede_from", &vh, &dm, None)
            .map_err(|e| BpaError::new(e.0, "timeline"))?;
        state.head = vh.clone();
        out.versions.push((vh.clone(), rec));
        out.changes.push((m, key.clone(), vh));
        from = cseq;
    }
    out.rows.push((
        oid,
        from,
        None,
        class,
        serde_json::to_value(&state)
            .map_err(|e| BpaError::new("MODEL.INVALID_VALUE", e.to_string()))?,
    ));
    Ok(out)
}

/// Keep only the nodes reachable from `root`.
fn prune(nodes: &mut MemNodes, root: &str) {
    let mut keep: HashMap<String, Node> = HashMap::new();
    let mut stack = vec![root.to_string()];
    while let Some(h) = stack.pop() {
        if keep.contains_key(&h) {
            continue;
        }
        if let Some(n) = nodes.nodes.remove(&h) {
            if n.level > 0 {
                stack.extend(n.entries.iter().map(|e| e.1.clone()));
            }
            keep.insert(h, n);
        }
    }
    nodes.nodes = keep;
}

/// Load a dataset into an emptied database; returns the load report.
pub fn load(url: &str, spec: Spec, threads: u64) -> Result<Value, BpaError> {
    let t0 = Instant::now();
    let classes = workload::classes(spec.seed);
    let reg = {
        let mut e = Engine::new(Box::new(PgStore::connect(url)?))?;
        e.load_genesis(&classes)?;
        e.registry.clone()
    };
    let fields = fields(&reg)?;
    let mut pg = PgStore::connect(url)?;
    let mut initial: Vec<(String, String)> = Vec::with_capacity(spec.objects as usize);
    let mut monthly: Vec<BTreeMap<String, Option<String>>> =
        (0..=spec.months).map(|_| BTreeMap::new()).collect();
    let (mut versions, mut rows) = (0u64, 0u64);
    let chunk = 50_000u64;
    let mut start = 0;
    while start < spec.objects {
        let end = (start + chunk).min(spec.objects);
        let parts: Vec<Result<Vec<ObjectLoad>, BpaError>> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let (reg, fields) = (&reg, &fields);
                    s.spawn(move || {
                        let mut v = Vec::new();
                        let mut i = start + t;
                        while i < end {
                            v.push(object(&spec, reg, fields, i)?);
                            i += threads;
                        }
                        Ok(v)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|_| {
                        Err(BpaError::new("NOD.STORAGE_UNAVAILABLE", "worker panicked"))
                    })
                })
                .collect()
        });
        let mut bulk = BulkRows::default();
        for part in parts {
            for o in part? {
                versions += o.versions.len() as u64;
                rows += o.rows.len() as u64;
                bulk.versions.extend(o.versions);
                bulk.objects.extend(o.rows);
                initial.push(o.initial);
                for (m, k, h) in o.changes {
                    monthly[m as usize].insert(k, Some(h));
                }
            }
        }
        pg.bulk_rows("main", &bulk)?;
        start = end;
    }
    let t_rows = t0.elapsed().as_secs_f64();

    // Tree: full build for the initial load, then one batch update per month.
    let mut written: HashSet<String> = HashSet::new();
    let mut nodes = MemNodes::default();
    let mut new_nodes = |ns: Vec<(String, Node)>, nodes: &mut MemNodes, pg: &mut PgStore| {
        let mut bulk = BulkRows::default();
        for (h, n) in ns {
            if written.insert(h.clone()) {
                bulk.nodes.push((h.clone(), n.to_json()));
            }
            nodes.nodes.insert(h, n);
        }
        let count = bulk.nodes.len() as u64;
        pg.bulk_rows("main", &bulk).map(|_| count)
    };
    let built = ubos_tree::build(&initial)?;
    drop(initial);
    let mut node_count = new_nodes(built.nodes, &mut nodes, &mut pg)?;
    let genesis = Repo::new(&mut pg, &reg).commit(1)?.root;
    let mut commits = vec![(
        2u64,
        1u64,
        json!({"parents": [1], "cseq": 1, "branch": "main", "root": built.root}),
    )];
    let mut root = built.root;
    for m in 1..=spec.months {
        let w = ubos_tree::update(&mut nodes, &root, &monthly[m as usize])?;
        node_count += new_nodes(w.nodes, &mut nodes, &mut pg)?;
        root = w.root;
        prune(&mut nodes, &root);
        let id = 2 + m;
        commits.push((
            id,
            1 + m,
            json!({"parents": [id - 1], "cseq": 1 + m, "branch": "main", "root": root}),
        ));
    }
    pg.bulk_commits("main", &commits)?;
    Ok(json!({
        "spec": spec.to_json(),
        "genesis_root": genesis,
        "head_cseq": 1 + spec.months,
        "root": root,
        "objects": spec.objects,
        "versions": versions,
        "object_rows": rows,
        "tree_nodes": node_count,
        "seconds": {"rows": (t_rows * 10.0).round() / 10.0, "total": (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0},
    }))
}

/// Load a small dataset in bulk and commit the same history through the kernel path on the
/// in-memory store; every commit's state root must agree, and so must head reads.
pub fn check(url: &str, spec: Spec) -> Result<Value, BpaError> {
    let report = load(url, spec, 2)?;
    let classes = workload::classes(spec.seed);
    let mut pg = Engine::attach(Box::new(PgStore::connect(url)?), &classes)?;
    let mut mem = Engine::new(Box::new(MemStore::default()))?;
    mem.load_genesis(&classes)?;
    let fields = fields(&mem.registry)?;
    let mut bodies: Vec<Map<String, Value>> = Vec::new();
    let mut heads: Vec<String> = Vec::new();
    let mut changes = Vec::new();
    for i in 0..spec.objects {
        let class = workload::class_of(spec.seed, i);
        let b = workload::body(spec.seed, i, 0, &fields[&class], None);
        changes.push(
            json!({"op": "create", "object_id": workload::object_id(spec.seed, i), "class": class,
            "slug": workload::slug(i), "body": b, "valid": {"from": workload::month_date(0)}}),
        );
        bodies.push(b);
    }
    let r = mem.commit(&json!({"branch": "main", "changes": changes}))?;
    for i in 0..spec.objects {
        heads.push(
            r["versions"][workload::object_id(spec.seed, i)]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        );
    }
    for m in 1..=spec.months {
        let mut changes = Vec::new();
        for i in 0..spec.objects {
            if !workload::changes_in(spec.seed, i, m) {
                continue;
            }
            let class = workload::class_of(spec.seed, i);
            let b = workload::body(spec.seed, i, m, &fields[&class], Some(&bodies[i as usize]));
            changes.push(
                json!({"op": "update", "object_id": workload::object_id(spec.seed, i),
                "base": heads[i as usize], "body": b,
                "valid": {"from": workload::month_date(m), "kind": "supersede_from"}}),
            );
            bodies[i as usize] = b;
        }
        let r = mem.commit(&json!({"branch": "main", "changes": changes}))?;
        for (oid, h) in r["versions"].as_object().cloned().unwrap_or_default() {
            let i = (0..spec.objects)
                .find(|i| workload::object_id(spec.seed, *i) == oid)
                .unwrap_or_default();
            heads[i as usize] = h.as_str().unwrap_or_default().to_string();
        }
    }
    let mut roots = Vec::new();
    for c in 0..=1 + spec.months {
        let a = Repo::new(mem.store.as_mut(), &mem.registry).root_at("main", Some(c))?;
        let b = Repo::new(pg.store.as_mut(), &pg.registry).root_at("main", Some(c))?;
        if a != b {
            return Err(BpaError::new(
                "NOD.STORAGE_UNAVAILABLE",
                format!("bulk load differs from the commit path at cseq {c}"),
            ));
        }
        roots.push(a);
    }
    for i in (0..spec.objects).step_by(7) {
        let q = json!({"object_id": workload::object_id(spec.seed, i)});
        if mem.read(&q)? != pg.read(&q)? {
            return Err(BpaError::new(
                "NOD.STORAGE_UNAVAILABLE",
                format!("read of object {i} differs"),
            ));
        }
    }
    // Group commit: the same commits one by one (memory) and in groups (PostgreSQL),
    // including an object changed twice in one group and a stale commit inside a group.
    let mut inputs = Vec::new();
    for k in 0..60u64 {
        let i = workload::mix(spec.seed, 77, k % 45) % spec.objects;
        let oid = workload::object_id(spec.seed, i);
        let class = workload::class_of(spec.seed, i);
        let b = workload::body(
            spec.seed ^ 1,
            i,
            500 + k,
            &fields[&class],
            Some(&bodies[i as usize]),
        );
        let input = json!({"branch": "main", "changes": [{"op": "update", "object_id": oid,
            "base": heads[i as usize], "body": b}]});
        let r = mem.commit(&input)?;
        heads[i as usize] = r["versions"][&oid].as_str().unwrap_or_default().to_string();
        bodies[i as usize] = b;
        inputs.push(input);
    }
    let mut stale = inputs[0].clone();
    stale["changes"][0]["base"] =
        json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
    let mut k = 0;
    let mut groups = 0;
    while k < inputs.len() {
        let n = (1 + workload::mix(spec.seed, 78, k as u64) % 8) as usize;
        let mut batch: Vec<Value> = inputs[k..(k + n).min(inputs.len())].to_vec();
        batch.insert(batch.len() / 2, stale.clone());
        let out = pg.commit_group(&batch)?;
        let failed = out.iter().filter(|r| r.is_err()).count();
        if failed != 1 {
            return Err(BpaError::new(
                "NOD.STORAGE_UNAVAILABLE",
                "group commit: only the stale commit may fail",
            ));
        }
        k += n;
        groups += 1;
    }
    let a = Repo::new(mem.store.as_mut(), &mem.registry).root_at("main", None)?;
    let b = Repo::new(pg.store.as_mut(), &pg.registry).root_at("main", None)?;
    if a != b {
        return Err(BpaError::new(
            "NOD.STORAGE_UNAVAILABLE",
            "group commit differs from single commits",
        ));
    }
    for c in 2 + spec.months..=2 + spec.months + inputs.len() as u64 - 1 {
        let a = Repo::new(mem.store.as_mut(), &mem.registry).root_at("main", Some(c))?;
        let b = Repo::new(pg.store.as_mut(), &pg.registry).root_at("main", Some(c))?;
        if a != b {
            return Err(BpaError::new(
                "NOD.STORAGE_UNAVAILABLE",
                format!("group commit differs at cseq {c}"),
            ));
        }
    }
    pg.other("export_state", &json!({"branch": "main"}))?;
    Ok(
        json!({"check": "bulk load equals commit path; group commit equals single commits",
              "commits_compared": roots.len(), "group_commits": inputs.len(), "groups": groups,
              "head_root": b, "load": report}),
    )
}
