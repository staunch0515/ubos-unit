//! Branch metadata, snapshot selection and parent-line views (STD-VER-011, -080, -081,
//! -090; DEC-051).
//!
//! Branch metadata (kind, creation and activity times, parent line, origin) and tags live
//! in the store's settings under `branch:<name>` and `tag:<name>`, so that every backend
//! carries them without a schema change until step 4d.

use crate::state::{ObjectState, Repo, State, Tx};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use ubos_model::types::BpaError;

/// Protected branch kinds (STD-VER-012).
pub const PROTECTED: &[&str] = &["main", "release", "overlay"];

/// Protected targets a branch of each kind may be merged into (STD-VER-011, DEC-041).
fn merge_targets(kind: &str) -> &'static [&'static str] {
    match kind {
        "release" => &["main"],
        "draft" | "feature" | "agent" | "loader" | "evolution" | "correction" | "device" => {
            PROTECTED
        }
        _ => &[],
    }
}

/// Default lifetime in days (STD-VER-011); `draft` counts from the last commit.
fn lifetime(kind: &str) -> Option<i64> {
    match kind {
        "draft" | "feature" => Some(180),
        "agent" => Some(30),
        "evolution" | "correction" => Some(90),
        "sandbox" => Some(14),
        _ => None,
    }
}

/// The kind of a branch from its name.
pub fn branch_kind(name: &str) -> &str {
    if name == "main" {
        "main"
    } else {
        name.split('/').next().unwrap_or_default()
    }
}

/// A resolved parent line.
#[derive(Debug, Clone, PartialEq)]
pub struct ParentLine {
    /// Parent branch.
    pub branch: String,
    /// `live` or `pinned`.
    pub mode: String,
    /// Pinned commit (engine identifier).
    pub pin: Option<u64>,
}

/// Metadata of a branch (`branch:<name>`).
#[derive(Debug, Clone)]
pub struct BranchMeta {
    /// Kind from the name.
    pub kind: String,
    /// Creation instant.
    pub created_at: Option<String>,
    /// Last commit instant.
    pub active_at: Option<String>,
    /// Parent line, if any.
    pub parent_line: Option<ParentLine>,
    /// Commit the branch was created from.
    pub created_from: u64,
}

impl BranchMeta {
    fn to_json(&self) -> Value {
        json!({
            "kind": self.kind, "created_at": self.created_at, "active_at": self.active_at,
            "parent_line": self.parent_line.as_ref().map(|p| json!({"branch": p.branch, "mode": p.mode, "pin": p.pin})),
            "created_from": self.created_from,
        })
    }

    fn from_json(name: &str, v: Option<Value>) -> BranchMeta {
        let v = v.unwrap_or(Value::Null);
        let pl = v
            .get("parent_line")
            .filter(|p| p.is_object())
            .map(|p| ParentLine {
                branch: p["branch"].as_str().unwrap_or_default().to_string(),
                mode: p["mode"].as_str().unwrap_or("live").to_string(),
                pin: p["pin"].as_u64(),
            });
        BranchMeta {
            kind: v["kind"].as_str().unwrap_or(branch_kind(name)).to_string(),
            created_at: v["created_at"].as_str().map(str::to_string),
            active_at: v["active_at"].as_str().map(str::to_string),
            parent_line: pl,
            created_from: v["created_from"].as_u64().unwrap_or_default(),
        }
    }
}

/// Expiry instant from branch metadata (STD-VER-011): the lifetime of the kind counted from
/// the creation, for `draft` from the last commit.
pub fn expires_of(m: &BranchMeta) -> Result<Option<String>, BpaError> {
    let Some(days) = lifetime(&m.kind) else {
        return Ok(None);
    };
    let start = if m.kind == "draft" {
        m.active_at.clone().or(m.created_at.clone())
    } else {
        m.created_at.clone()
    };
    let Some(start) = start else {
        return Ok(None);
    };
    let t = ubos_model::types::time::parse_instant(&start)? + chrono::Duration::days(days);
    Ok(Some(ubos_model::types::time::format_instant(&t)))
}

/// A snapshot selection (STD-VER-090): the commit and the snapshot time (`None` at the
/// head, where live parent lines resolve at their heads).
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Selected commit (engine identifier).
    pub commit: u64,
    /// Snapshot time.
    pub tau: Option<String>,
}

/// A resolved view of a branch: objects of the branch and its parent line, where each
/// object came from, and the commits evaluated.
#[derive(Debug, Clone)]
pub struct View {
    /// Objects and the branch's own ledger.
    pub state: State,
    /// Object → (branch, commit) it was resolved from (STD-VER-081).
    pub provenance: BTreeMap<String, (String, u64)>,
    /// Branch → commit ID evaluated.
    pub evaluated_at: BTreeMap<String, String>,
}

fn not_found(m: &str) -> BpaError {
    BpaError::new("IAM.NOT_FOUND", m)
}

impl Repo<'_> {
    /// Metadata of a branch.
    pub fn branch_meta(&mut self, name: &str) -> Result<BranchMeta, BpaError> {
        self.head(name)?;
        let v = self.store.branch_meta_rec(name)?;
        Ok(BranchMeta::from_json(name, v))
    }

    /// Store the metadata of a branch.
    pub fn put_branch_meta(&mut self, name: &str, m: &BranchMeta) -> Result<(), BpaError> {
        self.store.put_branch_meta_rec(name, &m.to_json())
    }

    /// Expiry instant of a branch, if its kind has a lifetime (STD-VER-011).
    pub fn expires_at(&mut self, name: &str) -> Result<Option<String>, BpaError> {
        let m = self.branch_meta(name)?;
        expires_of(&m)
    }

    /// `active` or `archived` at the adapter clock `now`.
    pub fn branch_state(&mut self, name: &str, now: &str) -> Result<&'static str, BpaError> {
        Ok(match self.expires_at(name)? {
            Some(exp) if now >= exp.as_str() => "archived",
            _ => "active",
        })
    }

    /// Fail with `VER.BRANCH_ARCHIVED` unless the branch is active.
    pub fn check_active(&mut self, name: &str, now: &str) -> Result<(), BpaError> {
        if self.branch_state(name, now)? != "active" {
            return Err(BpaError::new("VER.BRANCH_ARCHIVED", name));
        }
        Ok(())
    }

    /// STD-VER-011 merge targets per branch kind.
    pub fn merge_allowed(
        &mut self,
        source: &str,
        target: &str,
        via_change_set: bool,
    ) -> Result<(), BpaError> {
        let sk = self.branch_meta(source)?.kind;
        let tk = self.branch_meta(target)?.kind;
        if sk == "sandbox"
            || (PROTECTED.contains(&tk.as_str()) && !merge_targets(&sk).contains(&tk.as_str()))
        {
            return Err(BpaError::new(
                "VER.MERGE_NOT_ALLOWED",
                format!("{sk} branches do not merge into {tk}"),
            ));
        }
        if PROTECTED.contains(&tk.as_str()) && sk == "agent" && !via_change_set {
            return Err(BpaError::new(
                "VER.CHANGE_SET_REQUIRED",
                "agent branches reach protected targets by change set only",
            ));
        }
        Ok(())
    }

    /// Resolve and check a parent line given as `{branch, mode, pin}` (STD-VER-080).
    pub fn parent_line(&mut self, name: &str, pl: &Value) -> Result<ParentLine, BpaError> {
        let mode = pl.get("mode").and_then(Value::as_str).unwrap_or("live");
        if !pl.is_object() || !matches!(mode, "live" | "pinned") {
            return Err(BpaError::new(
                "MODEL.INVALID_VALUE",
                "parent_line is {branch, mode, pin}",
            ));
        }
        let p = pl
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.head(&p)?;
        let pin = if mode == "pinned" {
            Some(self.commit_at(&p, pl.get("pin").and_then(Value::as_u64))?)
        } else {
            None
        };
        let mut seen: HashSet<String> = [name.to_string()].into();
        let mut b = Some(p.clone());
        while let Some(x) = b {
            if !seen.insert(x.clone()) {
                return Err(BpaError::new("VER.INHERITANCE_CYCLE", x));
            }
            b = self.branch_meta(&x)?.parent_line.map(|l| l.branch);
        }
        Ok(ParentLine {
            branch: p,
            mode: mode.to_string(),
            pin,
        })
    }

    /// The first-parent history of a commit, the commit first.
    pub fn first_parents(&mut self, mut id: u64) -> Result<Vec<u64>, BpaError> {
        let mut out = vec![id];
        while let Some(p) = self.meta(id)?.parents.first().copied() {
            out.push(p);
            id = p;
        }
        Ok(out)
    }

    /// The commit on a branch's first-parent history with a commit ID.
    pub fn by_id(&mut self, branch: &str, commit_id: &str) -> Result<u64, BpaError> {
        let head = self.head(branch)?;
        for c in self.first_parents(head)? {
            if self.meta(c)?.commit_id == commit_id {
                return Ok(c);
            }
        }
        Err(not_found("commit not on the history of the branch"))
    }

    /// Latest commit on the first-parent history with `tx_time <= t` (STD-VER-090).
    pub fn at_tx(&mut self, branch: &str, t: &str) -> Result<u64, BpaError> {
        let head = self.head(branch)?;
        for c in self.first_parents(head)? {
            match self.meta(c)?.tx_time {
                Some(tx) if tx.as_str() > t => continue,
                _ => return Ok(c),
            }
        }
        Err(not_found("no commit at that time"))
    }

    /// A tag record.
    pub fn tag(&mut self, name: &str) -> Result<Option<Value>, BpaError> {
        self.store.tag_rec(name)
    }

    /// Select a snapshot of a branch (STD-VER-090): at most one of `time`, `commit`, `tag`
    /// and `tx_time`; `evaluated_at` pins replace the selectors.
    pub fn select(&mut self, branch: &str, i: &Value) -> Result<Snapshot, BpaError> {
        let sel: Vec<&str> = ["time", "commit", "tag", "tx_time"]
            .into_iter()
            .filter(|k| i.get(k).is_some_and(|v| !v.is_null()))
            .collect();
        if sel.len() > 1 {
            return Err(BpaError::new(
                "QRY.INVALID_QUERY",
                "at most one of time, commit, tag and tx_time",
            ));
        }
        let pins = i
            .get("evaluated_at")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if !pins.is_empty() && !sel.is_empty() {
            return Err(BpaError::new(
                "QRY.INVALID_QUERY",
                "evaluated_at replaces the selectors",
            ));
        }
        if let Some(p) = pins.get(branch) {
            let c = self.by_id(branch, p.as_str().unwrap_or_default())?;
            return Ok(Snapshot {
                commit: c,
                tau: self.meta(c)?.tx_time,
            });
        }
        let Some(k) = sel.first() else {
            return Ok(Snapshot {
                commit: self.head(branch)?,
                tau: None,
            });
        };
        let c = match *k {
            "time" => {
                let t = i["time"]
                    .as_u64()
                    .ok_or_else(|| not_found("no commit with this cseq"))?;
                self.commit_at(branch, Some(t))?
            }
            "commit" => self.by_id(branch, i["commit"].as_str().unwrap_or_default())?,
            "tag" => {
                let t = self
                    .tag(i["tag"].as_str().unwrap_or_default())?
                    .filter(|t| t["deleted"] != json!(true))
                    .ok_or_else(|| not_found("unknown tag"))?;
                self.by_id(branch, t["commit_id"].as_str().unwrap_or_default())?
            }
            _ => {
                let t = i["tx_time"].as_str().unwrap_or_default();
                ubos_model::types::values::validate_value("instant", None, &i["tx_time"])?;
                let c = self.at_tx(branch, t)?;
                return Ok(Snapshot {
                    commit: c,
                    tau: Some(t.to_string()),
                });
            }
        };
        Ok(Snapshot {
            commit: c,
            tau: self.meta(c)?.tx_time,
        })
    }

    /// The view of a branch's parent line at its head, as the branch resolves it now
    /// (STD-VER-082): `None` for a branch without a parent line.
    pub fn parent_line_view(&mut self, branch: &str) -> Result<Option<View>, BpaError> {
        self.parent_view(branch, None, &Map::new(), 0)
    }

    fn parent_view(
        &mut self,
        branch: &str,
        tau: Option<&str>,
        pins: &Map<String, Value>,
        depth: usize,
    ) -> Result<Option<View>, BpaError> {
        let Some(pl) = self.branch_meta(branch)?.parent_line else {
            return Ok(None);
        };
        if depth > 16 {
            return Err(BpaError::new("VER.INHERITANCE_CYCLE", branch));
        }
        let p = pl.branch.clone();
        let pcid = if let Some(pin) = pins.get(&p) {
            self.by_id(&p, pin.as_str().unwrap_or_default())?
        } else if pl.mode == "pinned" {
            pl.pin.unwrap_or_default()
        } else if let Some(t) = tau {
            self.at_tx(&p, t)?
        } else {
            self.head(&p)?
        };
        let ptau = if pl.mode == "live" && tau.is_none() && !pins.contains_key(&p) {
            None
        } else {
            self.meta(pcid)?.tx_time
        };
        Ok(Some(self.view_at(
            &p,
            pcid,
            ptau.as_deref(),
            pins,
            depth + 1,
        )?))
    }

    /// Objects visible on a branch at a commit: its own versions and hide markers over its
    /// parent line (STD-VER-080), with the branch and commit each was resolved from.
    pub fn view_at(
        &mut self,
        branch: &str,
        cid: u64,
        tau: Option<&str>,
        pins: &Map<String, Value>,
        depth: usize,
    ) -> Result<View, BpaError> {
        let c = self.commit(cid)?;
        let own = self.commit_state(cid)?;
        let mut evaluated_at = BTreeMap::new();
        evaluated_at.insert(branch.to_string(), c.commit_id.clone());
        let Some(parent) = self.parent_view(branch, tau, pins, depth)? else {
            let provenance = own
                .objects
                .keys()
                .map(|o| (o.clone(), (branch.to_string(), cid)))
                .collect();
            return Ok(View {
                state: own,
                provenance,
                evaluated_at,
            });
        };
        let mut objects = parent.state.objects;
        let mut provenance = parent.provenance;
        for h in &c.hidden {
            objects.remove(h);
            provenance.remove(h);
        }
        for (oid, o) in own.objects {
            objects.insert(oid.clone(), o);
            provenance.insert(oid, (branch.to_string(), cid));
        }
        evaluated_at.extend(parent.evaluated_at);
        Ok(View {
            state: State {
                objects,
                ledger: own.ledger,
            },
            provenance,
            evaluated_at,
        })
    }

    /// The view of a branch for a read with selectors (STD-VER-090).
    pub fn view(&mut self, branch: &str, i: &Value) -> Result<(View, Snapshot), BpaError> {
        let snap = self.select(branch, i)?;
        let pins = i
            .get("evaluated_at")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let v = self.view_at(branch, snap.commit, snap.tau.as_deref(), &pins, 0)?;
        Ok((v, snap))
    }

    /// Begin a commit on a branch with the full view loaded: inherited objects can be read
    /// and overridden, hide markers are carried (STD-VER-080).
    pub fn begin_view(&mut self, branch: &str) -> Result<Tx, BpaError> {
        let mut tx = self.begin(branch)?;
        let head = tx.head;
        let v = self.view_at(branch, head, None, &Map::new(), 0)?;
        let mut inherited: HashMap<String, ObjectState> = HashMap::new();
        for (oid, (b, _)) in &v.provenance {
            if b != branch {
                inherited.insert(oid.clone(), v.state.objects[oid].clone());
            }
        }
        for (id, o) in &v.state.objects {
            tx.loaded.insert(id.clone(), Some(o.clone()));
        }
        for (c, k, q) in &v.state.ledger {
            tx.loaded_queues.insert((c.clone(), k.clone()));
            tx.loaded_entries
                .extend(q.iter().map(|e| e.entry_id.clone()));
        }
        tx.state = v.state;
        tx.inherited = inherited;
        tx.hidden = self
            .commit(head)?
            .hidden
            .into_iter()
            .collect::<BTreeSet<_>>();
        Ok(tx)
    }

    /// Record an operation of this commit on an object (STD-TIME-040).
    pub fn op(tx: &mut Tx, oid: &str, kind: &str, from: Option<&str>, to: Option<&str>) {
        let mut op = json!({"kind": kind});
        if let Some(a) = from {
            op["from"] = json!(a);
        }
        if let Some(b) = to {
            op["to"] = json!(b);
        }
        tx.ops.entry(oid.to_string()).or_default().push(op);
    }

    /// The commit record as `commit_info` reports it (STD-TIME-040).
    pub fn commit_info(&mut self, cid: u64) -> Result<Value, BpaError> {
        let c = self.commit(cid)?;
        let parents: Vec<String> = c
            .parents
            .iter()
            .map(|p| self.commit(*p).map(|x| x.commit_id))
            .collect::<Result<_, _>>()?;
        let mut out = json!({
            "branch": c.branch, "cseq": c.cseq, "commit_id": c.commit_id, "tx_time": c.tx_time,
            "parents": parents,
            "rule_binding": c.extra.get("rule_binding").cloned().unwrap_or_else(|| json!({"classes": []})),
        });
        for k in ["process_id", "resolutions", "change_set", "reverts"] {
            if let Some(v) = c.extra.get(k)
                && !v.is_null()
            {
                out[k] = v.clone();
            }
        }
        Ok(out)
    }

    /// The signed commit record of a commit in its portable form (STD-PROOF-011): `{vae,
    /// branch, parents (commit IDs), cseq, tx_time, root, signatures}` plus `commit_id`. The
    /// content ID of the record without `signatures` and `commit_id` is the commit ID.
    pub fn commit_record(&mut self, cid: u64) -> Result<Value, BpaError> {
        let c = self.commit(cid)?;
        let parents: Vec<String> = c
            .parents
            .iter()
            .map(|p| self.commit(*p).map(|x| x.commit_id))
            .collect::<Result<_, _>>()?;
        Ok(json!({
            "vae": self.store.authority(), "branch": c.branch, "parents": parents, "cseq": c.cseq,
            "tx_time": c.tx_time, "root": c.root,
            "signatures": c.extra.get("signatures").cloned().unwrap_or_else(|| json!([])),
            "commit_id": c.commit_id,
        }))
    }

    /// Entries of the state tree whose values differ between two views (STD-VER-031).
    pub fn tree_keys_diff(&mut self, a: &State, b: &State) -> Result<Vec<String>, BpaError> {
        let ea: BTreeMap<String, String> = self.state_entries(a)?.into_iter().collect();
        let eb: BTreeMap<String, String> = self.state_entries(b)?.into_iter().collect();
        let mut keys: Vec<String> = ea
            .keys()
            .chain(eb.keys())
            .filter(|k| ea.get(*k) != eb.get(*k))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        keys.sort_by(|x, y| x.as_bytes().cmp(y.as_bytes()));
        Ok(keys)
    }
}
