//! History queries, the audit trail and the security log (PH-1 step 13c, DEC-073).
//!
//! `changes` answers change queries (FR-QRY-051, DSN-DVM-907) and `transitions` transition
//! queries (FR-QRY-052) over the first-parent history of a branch; `audit` reports, for
//! one object, who changed what, when, why, how and under which rules from the commit and
//! process records (FR-AUD-011); the security log is a hash-chained, append-only log of
//! the security-relevant events of the kernel (FR-AUD-012, FR-AUD-014; DSN-NOD-111): the
//! denials of the decision log (DSN-DVM-805), policy changes, key operations, exports and
//! administrative operations. No operation changes or removes a log entry;
//! `verify_security_log` detects a removed or altered one.
//!
//! The three queries page with the signed snapshot cursors of DSN-DVM-908: a cursor binds
//! the query, the branch and the head commit of the first page, so later pages read the
//! same history. Items are ordered by `(cseq, object_id)` (FR-QRY-072).

use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ubos_query::Cursor;
use ubos_types::{BpaError, content_id, values::validate_value};
use ubos_version::{CommitRecord, ObjectState, Repo};

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn invalid(m: &str) -> BpaError {
    BpaError::new("QRY.INVALID_QUERY", m)
}

/// Default and largest page of the history queries.
const DEFAULT_LIMIT: u64 = 100;
const MAX_LIMIT: u64 = 10_000;

/// One bound of a commit range: a transaction-time instant or a commit.
enum Bound {
    Time(String),
    Cseq(u64),
}

/// A bound of the input: an instant text, `{commit}` (a commit ID on the branch) or
/// `{cseq}`.
fn bound(repo: &mut Repo<'_>, branch: &str, v: Option<&Value>) -> Result<Option<Bound>, BpaError> {
    let Some(v) = v.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    if let Some(c) = s(v, "commit") {
        let id = repo.by_id(branch, c)?;
        return Ok(Some(Bound::Cseq(repo.commit(id)?.cseq)));
    }
    if let Some(c) = v.get("cseq").and_then(Value::as_u64) {
        return Ok(Some(Bound::Cseq(c)));
    }
    if v.is_string() {
        let t = validate_value("instant", None, v)?;
        return Ok(Some(Bound::Time(
            t.as_str().unwrap_or_default().to_string(),
        )));
    }
    Err(invalid("a bound is an instant, {commit} or {cseq}"))
}

/// The commits of the first-parent history of `head` within the bounds, oldest first:
/// after `from` and up to `to` for commit bounds, at or after `from` and before `to` for
/// time bounds. The genesis commit (no transaction time) is never included.
fn commits_in(
    repo: &mut Repo<'_>,
    head: u64,
    from: Option<&Bound>,
    to: Option<&Bound>,
) -> Result<Vec<CommitRecord>, BpaError> {
    let mut out = Vec::new();
    for cid in repo.first_parents(head)? {
        let c = repo.commit(cid)?;
        let Some(t) = c.tx_time.as_deref() else {
            continue;
        };
        let after_from = match from {
            None => true,
            Some(Bound::Time(f)) => t >= f.as_str(),
            Some(Bound::Cseq(f)) => c.cseq > *f,
        };
        let before_to = match to {
            None => true,
            Some(Bound::Time(u)) => t < u.as_str(),
            Some(Bound::Cseq(u)) => c.cseq <= *u,
        };
        if after_from && before_to {
            out.push(c);
        }
    }
    out.reverse();
    Ok(out)
}

/// The field changes between two versions, as `DAT-DiffEntry` lists them (STD-VER-030).
fn field_changes(vx: &Value, vy: &Value) -> Vec<Value> {
    let (bx, by) = (
        vx["body"].as_object().cloned().unwrap_or_default(),
        vy["body"].as_object().cloned().unwrap_or_default(),
    );
    let keys: BTreeSet<&String> = bx.keys().chain(by.keys()).collect();
    keys.into_iter()
        .filter(|k| bx.get(*k) != by.get(*k))
        .map(|k| json!({"path": format!("/{k}"), "old": bx.get(k), "new": by.get(k)}))
        .collect()
}

/// A change of one object in one commit: the object before and after, the field changes,
/// the states left and entered.
struct Change {
    oid: String,
    before: Option<(ObjectState, Value)>,
    after: Option<(ObjectState, Value)>,
    op: &'static str,
    fields: Vec<Value>,
    state_from: Option<String>,
    state_to: Option<String>,
}

impl Change {
    fn class(&self) -> &str {
        self.after
            .as_ref()
            .or(self.before.as_ref())
            .map(|(o, _)| o.class.as_str())
            .unwrap_or_default()
    }

    fn item(&self, c: &CommitRecord) -> Value {
        let mut item = json!({"object_id": self.oid, "class": self.class(), "op": self.op,
            "cseq": c.cseq, "commit_id": c.commit_id, "tx_time": c.tx_time});
        if !self.fields.is_empty() {
            item["fields"] = json!(self.fields);
        }
        if self.state_from.is_some() || self.state_to.is_some() {
            item["state"] = json!({"from": self.state_from, "to": self.state_to});
        }
        if let Some(a) = c.extra.get("author") {
            item["author"] = a.clone();
        }
        if let Some(p) = c.extra.get("process_id") {
            item["process_id"] = p.clone();
        }
        if let Some((o, _)) = &self.after {
            item["version_hash"] = json!(o.head);
        }
        item
    }
}

/// The paging of a history query: the head commit the page reads and the offset.
struct Page {
    head: u64,
    offset: u64,
    limit: u64,
    q: String,
}

impl Engine {
    /// The linearizations of every class, for the class filter of a history query (a
    /// class matches its subclasses); empty without a filter.
    fn lineages(&self, class: Option<&str>) -> Result<BTreeMap<String, Vec<String>>, BpaError> {
        let mut out = BTreeMap::new();
        if let Some(c) = class {
            self.registry.decl(c)?;
            for d in self.registry.classes() {
                out.insert(d.name.clone(), d.linearization.clone());
            }
        }
        Ok(out)
    }

    /// The page of a history query: the cursor given binds the query hash, the branch and
    /// the head commit of its first page; without one the branch head is the snapshot.
    fn page(&mut self, op: &str, input: &Value, branch: &str) -> Result<Page, BpaError> {
        let mut spec = input.as_object().cloned().unwrap_or_default();
        for k in [
            "cursor",
            "limit",
            "principal",
            "principal_kind",
            "role",
            "roles",
            "auth_level",
            "channel",
            "on_behalf_of",
        ] {
            spec.remove(k);
        }
        spec.insert("op".into(), json!(op));
        let q = content_id("ubos.chunk.v1", &Value::Object(spec))?;
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_LIMIT)
            .clamp(1, MAX_LIMIT);
        let key = self.cursor_key();
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        match s(input, "cursor").filter(|c| !c.is_empty()) {
            Some(c) => {
                let cur = Cursor::decode(&key, c)?;
                if cur.q != q || cur.branch != branch {
                    return Err(invalid("the cursor belongs to another query"));
                }
                let head = repo.by_id(branch, &cur.commit)?;
                Ok(Page {
                    head,
                    offset: cur.offset,
                    limit,
                    q,
                })
            }
            None => Ok(Page {
                head: repo.head(branch)?,
                offset: 0,
                limit,
                q,
            }),
        }
    }

    /// The changes of the commits in range, oldest commit first and objects in identifier
    /// order within a commit; `only` restricts them to one object.
    fn changes_in(
        &mut self,
        commits: &[CommitRecord],
        only: Option<&str>,
    ) -> Result<Vec<(usize, Change)>, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let mut out = Vec::new();
        for (ix, c) in commits.iter().enumerate() {
            let ids: Vec<String> = c
                .ops
                .keys()
                .filter(|k| only.is_none_or(|o| o == k.as_str()))
                .cloned()
                .collect();
            if ids.is_empty() {
                continue;
            }
            let after = repo.objects_in_ids(&c.branch, c.cseq, &ids)?;
            let before: BTreeMap<String, ObjectState> = match c.parents.first() {
                Some(p) => {
                    let pc = repo.commit(*p)?;
                    repo.objects_in_ids(&pc.branch, pc.cseq, &ids)?
                }
                None => BTreeMap::new(),
            };
            for oid in ids {
                let x = before.get(&oid).cloned();
                let y = after.get(&oid).cloned();
                if x.is_none() && y.is_none() {
                    continue;
                }
                let vx = match &x {
                    Some(o) => Some(repo.version(&o.head)?),
                    None => None,
                };
                let vy = match &y {
                    Some(o) => Some(repo.version(&o.head)?),
                    None => None,
                };
                let same = matches!((&x, &y), (Some(a), Some(b)) if a.head == b.head);
                let state_from = vx.as_ref().and_then(crate::flow::state_of);
                let state_to = vy.as_ref().and_then(crate::flow::state_of);
                let (op, fields): (&'static str, Vec<Value>) = match (&vx, &vy) {
                    (None, Some(_)) => ("added", Vec::new()),
                    (Some(_), None) => ("ended", Vec::new()),
                    (Some(a), Some(b)) => {
                        if b["tombstone"] == json!(true) && a["tombstone"] != json!(true) {
                            ("ended", Vec::new())
                        } else if same {
                            // a timeline change of the same version (set_period, hide)
                            ("changed", Vec::new())
                        } else {
                            let f = field_changes(a, b);
                            if f.is_empty() && a["slug"] != b["slug"] {
                                ("moved", f)
                            } else {
                                ("changed", f)
                            }
                        }
                    }
                    (None, None) => continue,
                };
                let ch = Change {
                    oid,
                    before: x.zip(vx),
                    after: y.zip(vy),
                    op,
                    fields,
                    state_from,
                    state_to,
                };
                out.push((ix, ch));
            }
        }
        Ok(out)
    }

    /// Whether the principal of the input may read the history of the changed object
    /// (`read_history`; DSN-DVM-804, -820): rows the principal may not read are left out.
    fn may_read_history(
        &mut self,
        input: &Value,
        branch: &str,
        ch: &Change,
    ) -> Result<bool, BpaError> {
        let Some(principal) = Engine::principal_of(input) else {
            return Ok(true);
        };
        let Some(set) = self.policy_set_at(branch, None)? else {
            return Ok(true);
        };
        let Some((o, v)) = ch.after.as_ref().or(ch.before.as_ref()) else {
            return Ok(false);
        };
        let ctx = self.access_ctx(input, branch)?;
        let res = self.object_resource(branch, &ch.oid, o, v)?;
        let d = self.decide_access(&set, branch, &principal, "read_history", &res, &ctx, None)?;
        Ok(d.allow)
    }

    /// Run a history query: the commits in range, the changes filtered by `keep`, the page
    /// at the cursor's offset and the next cursor.
    fn history_query(
        &mut self,
        op: &str,
        input: &Value,
        only: Option<&str>,
        mut keep: impl FnMut(&Change, &CommitRecord) -> bool,
        item: impl Fn(&mut Engine, &Change, &CommitRecord) -> Result<Value, BpaError>,
    ) -> Result<Value, BpaError> {
        let branch = s(input, "branch").unwrap_or("main").to_string();
        let page = self.page(op, input, &branch)?;
        let (commits, head_id) = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            let from = bound(&mut repo, &branch, input.get("from"))?;
            let to = bound(&mut repo, &branch, input.get("to"))?;
            let commits = commits_in(&mut repo, page.head, from.as_ref(), to.as_ref())?;
            (commits, repo.commit(page.head)?.commit_id)
        };
        let changes = self.changes_in(&commits, only)?;
        let mut items = Vec::new();
        let mut seen: u64 = 0;
        let mut more = false;
        for (ix, ch) in &changes {
            let c = &commits[*ix];
            if !keep(ch, c) || !self.may_read_history(input, &branch, ch)? {
                continue;
            }
            if seen < page.offset {
                seen += 1;
                continue;
            }
            if items.len() as u64 >= page.limit {
                more = true;
                break;
            }
            items.push(item(self, ch, c)?);
            seen += 1;
        }
        let mut out = json!({"items": items, "evaluated_at": {branch.clone(): head_id.clone()}});
        if more {
            let cur = Cursor {
                q: page.q,
                branch: branch.clone(),
                commit: head_id,
                offset: seen,
            };
            out["cursor"] = json!(cur.encode(&self.cursor_key())?);
        }
        Ok(out)
    }

    /// `changes` (FR-QRY-051, DSN-DVM-907): the objects changed on a branch between two
    /// commits or in a transaction-time range, by class (and its subclasses), author,
    /// process, object and changed field.
    pub fn changes(&mut self, input: &Value) -> Result<Value, BpaError> {
        let class = s(input, "class").map(str::to_string);
        let lineage_of = self.lineages(class.as_deref())?;
        let author = s(input, "author").map(str::to_string);
        let process = s(input, "process_id").map(str::to_string);
        let object = s(input, "object_id").map(str::to_string);
        let field = s(input, "field").map(|f| format!("/{}", f.trim_start_matches('/')));
        let keep = move |ch: &Change, c: &CommitRecord| {
            if let Some(cls) = &class
                && !lineage_of
                    .get(ch.class())
                    .is_some_and(|l| l.iter().any(|x| x == cls))
            {
                return false;
            }
            if let Some(a) = &author
                && c.extra.get("author").and_then(Value::as_str) != Some(a.as_str())
            {
                return false;
            }
            if let Some(p) = &process
                && c.extra.get("process_id").and_then(Value::as_str) != Some(p.as_str())
            {
                return false;
            }
            if let Some(f) = &field
                && !ch.fields.iter().any(|x| s(x, "path") == Some(f.as_str()))
            {
                return false;
            }
            true
        };
        self.history_query("changes", input, object.as_deref(), keep, |_, ch, c| {
            Ok(ch.item(c))
        })
    }

    /// `transitions` (FR-QRY-052, DSN-DVM-907): the lifecycle transitions on a branch in a
    /// range, by class, object, state left and state entered, with time and actor.
    pub fn transitions(&mut self, input: &Value) -> Result<Value, BpaError> {
        let class = s(input, "class").map(str::to_string);
        let lineage_of = self.lineages(class.as_deref())?;
        let object = s(input, "object_id").map(str::to_string);
        let from_state = s(input, "from_state").map(str::to_string);
        let to_state = s(input, "to_state").map(str::to_string);
        let keep = move |ch: &Change, _: &CommitRecord| {
            if ch.state_to.is_none() || ch.state_from == ch.state_to {
                return false;
            }
            if let Some(cls) = &class
                && !lineage_of
                    .get(ch.class())
                    .is_some_and(|l| l.iter().any(|x| x == cls))
            {
                return false;
            }
            if from_state.is_some() && ch.state_from != from_state {
                return false;
            }
            if to_state.is_some() && ch.state_to != to_state {
                return false;
            }
            true
        };
        self.history_query("transitions", input, object.as_deref(), keep, |_, ch, c| {
            let mut item = json!({"object_id": ch.oid, "class": ch.class(),
                "from": ch.state_from, "to": ch.state_to,
                "cseq": c.cseq, "commit_id": c.commit_id, "tx_time": c.tx_time});
            if let Some(a) = c.extra.get("author") {
                item["actor"] = a.clone();
            }
            if let Some(p) = c.extra.get("process_id") {
                item["process_id"] = p.clone();
            }
            Ok(item)
        })
    }

    /// `audit` (FR-AUD-011, CAP-AUD-01): the audit trail of one object from its commits
    /// and their process records: who (principal, on behalf of whom), what (operation,
    /// fields before and after, states), when (transaction time, valid time), why
    /// (message, reason code, change set), how (channel, client, process kind and
    /// trigger) and under which rules (the rule binding of the commit).
    pub fn audit(&mut self, input: &Value) -> Result<Value, BpaError> {
        let oid = s(input, "object_id")
            .ok_or_else(|| invalid("audit names an object"))?
            .to_string();
        let branch = s(input, "branch").unwrap_or("main").to_string();
        // the object must exist at the head (DSN-DVM-804: not visible means not found)
        let (o, _, _, _) = self.object_view(&branch, &json!({"branch": branch}), &oid)?;
        if let Some(principal) = Engine::principal_of(input)
            && let Some(set) = self.policy_set_at(&branch, None)?
        {
            let v = Repo::new(self.store.as_mut(), &self.registry).version(&o.head)?;
            let resource = self.object_resource(&branch, &oid, &o, &v)?;
            let ctx = self.access_ctx(input, &branch)?;
            self.enforce(&set, &branch, &principal, "read_history", &resource, &ctx)?;
        }
        self.history_query(
            "audit",
            input,
            Some(&oid),
            |_, _| true,
            |e, ch, c| e.audit_record(ch, c),
        )
    }

    /// One audit record (FR-AUD-011).
    fn audit_record(&mut self, ch: &Change, c: &CommitRecord) -> Result<Value, BpaError> {
        let x = |k: &str| c.extra.get(k).cloned().filter(|v| !v.is_null());
        let mut who = json!({"principal": x("author")});
        if let Some(b) = x("on_behalf_of") {
            who["on_behalf_of"] = b;
        }
        let mut what = json!({"op": ch.op, "object_id": ch.oid, "class": ch.class()});
        if !ch.fields.is_empty() {
            what["fields"] = json!(ch.fields);
        }
        if ch.state_from.is_some() || ch.state_to.is_some() {
            what["state"] = json!({"from": ch.state_from, "to": ch.state_to});
        }
        if let Some((o, _)) = &ch.after {
            what["version_hash"] = json!(o.head);
        }
        let mut when = json!({"tx_time": c.tx_time, "cseq": c.cseq, "commit_id": c.commit_id});
        if let Some((o, _)) = &ch.after
            && let Some(seg) = o.timeline.iter().find(|t| t.version == o.head)
        {
            when["valid"] = json!({"from": seg.from, "to": seg.to});
        }
        let mut why = Map::new();
        for k in ["message", "reason", "change_set"] {
            if let Some(v) = x(k) {
                why.insert(k.into(), v);
            }
        }
        let mut how = Map::new();
        for k in ["channel", "client"] {
            if let Some(v) = x(k) {
                how.insert(k.into(), v);
            }
        }
        if let Some(p) = x("process_id") {
            how.insert("process_id".into(), p.clone());
            if let Some(rec) = self.store.process_rec(p.as_str().unwrap_or_default())? {
                for k in ["kind", "trigger", "parent_process"] {
                    if let Some(v) = rec.get(k).filter(|v| !v.is_null()) {
                        how.insert(k.into(), v.clone());
                    }
                }
                if let Some(a) = rec.get("approvals").filter(|v| !v.is_null()) {
                    why.insert("approvals".into(), a.clone());
                }
            }
        }
        let rules = x("rule_binding").unwrap_or_else(|| json!({"classes": []}));
        Ok(json!({"who": who, "what": what, "when": when, "why": why, "how": how, "rules": rules}))
    }

    // ---- security log (FR-AUD-012, FR-AUD-014) ----

    /// The head of the security log `(seq, hash)`, `(0, "")` when empty.
    fn security_head(&mut self) -> Result<(u64, String), BpaError> {
        match self.store.meta("seclog:head")? {
            Some(h) => Ok((
                h["seq"].as_u64().unwrap_or_default(),
                s(&h, "hash").unwrap_or_default().to_string(),
            )),
            None => Ok((0, String::new())),
        }
    }

    /// Append an event to the security log: the entry carries its sequence number, the
    /// kernel time, the kind, the record and the hash of the previous entry; its hash is
    /// the content ID of those members. Returns the sequence number.
    pub(crate) fn security_append(&mut self, kind: &str, record: Value) -> Result<u64, BpaError> {
        let (seq, prev) = self.security_head()?;
        let seq = seq + 1;
        let mut entry =
            json!({"seq": seq, "at": self.now, "kind": kind, "record": record, "prev": prev});
        let hash = content_id("ubos.chunk.v1", &entry)?;
        entry["hash"] = json!(hash);
        self.store.put_meta(&format!("seclog:{seq}"), &entry)?;
        self.store
            .put_meta("seclog:head", &json!({"seq": seq, "hash": hash}))?;
        Ok(seq)
    }

    /// `security_log {from?, to?, limit?}`: the entries with `from <= seq <= to`, oldest
    /// first, and the head.
    pub fn security_log(&mut self, input: &Value) -> Result<Value, BpaError> {
        let (head, hash) = self.security_head()?;
        let from = input
            .get("from")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1);
        let to = input
            .get("to")
            .and_then(Value::as_u64)
            .unwrap_or(head)
            .min(head);
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_LIMIT)
            .clamp(1, MAX_LIMIT);
        let mut entries = Vec::new();
        let mut seq = from;
        while seq <= to && (entries.len() as u64) < limit {
            if let Some(e) = self.store.meta(&format!("seclog:{seq}"))? {
                entries.push(e);
            }
            seq += 1;
        }
        Ok(json!({"entries": entries, "head": {"seq": head, "hash": hash}}))
    }

    /// `verify_security_log`: walk the chain from the first entry to the head; a missing
    /// entry is a gap, an entry whose hash or `prev` does not match is broken (FR-AUD-012
    /// acceptance 1).
    pub fn verify_security_log(&mut self) -> Result<Value, BpaError> {
        let (head, hash) = self.security_head()?;
        let mut gaps = Vec::new();
        let mut broken = Vec::new();
        let mut prev = String::new();
        let mut entries: u64 = 0;
        for seq in 1..=head {
            let Some(e) = self.store.meta(&format!("seclog:{seq}"))? else {
                gaps.push(seq);
                prev.clear();
                continue;
            };
            entries += 1;
            let mut body = e.clone();
            body.as_object_mut().map(|m| m.remove("hash"));
            let want = content_id("ubos.chunk.v1", &body)?;
            let ok = s(&e, "hash") == Some(want.as_str())
                && e["seq"].as_u64() == Some(seq)
                && (seq == 1
                    || gaps.last() == Some(&(seq - 1))
                    || s(&e, "prev") == Some(prev.as_str()));
            if !ok {
                broken.push(seq);
            }
            prev = s(&e, "hash").unwrap_or_default().to_string();
        }
        let head_ok = head == 0 || (prev == hash && gaps.last() != Some(&head));
        if !head_ok && head > 0 && !broken.contains(&head) && !gaps.contains(&head) {
            broken.push(head);
        }
        let ok = gaps.is_empty() && broken.is_empty() && head_ok;
        Ok(
            json!({"ok": ok, "entries": entries, "head": {"seq": head, "hash": hash},
            "gaps": gaps, "broken": broken}),
        )
    }

    /// The security-log record of a policy object written by a commit (no field values
    /// beyond the policy's name and enabled flag, DSN-DVM-805).
    pub(crate) fn log_policy_changes(
        &mut self,
        branch: &str,
        result: &Value,
    ) -> Result<(), BpaError> {
        let versions = result["versions"].as_object().cloned().unwrap_or_default();
        if versions.is_empty() {
            return Ok(());
        }
        let mut written: Vec<(String, Value)> = Vec::new();
        {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            for (oid, vh) in &versions {
                let Some(vh) = vh.as_str() else {
                    continue;
                };
                let v = repo.version(vh)?;
                if s(&v, "class") == Some("Policy") {
                    written.push((oid.clone(), v));
                }
            }
        }
        for (oid, v) in written {
            let rec = json!({"branch": branch, "cseq": result["cseq"], "object_id": oid,
                "name": v["body"]["name"], "enabled": v["body"]["enabled"] != json!(false),
                "ended": v["tombstone"] == json!(true)});
            self.security_append("policy.change", rec)?;
        }
        Ok(())
    }
}
