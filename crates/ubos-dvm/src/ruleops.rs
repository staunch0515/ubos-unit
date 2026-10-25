//! Rule operations (PH-2 steps 5d and 5e, DEC-115): the live impact of a rule change
//! (STD-LGS-071, BPA 1.0-rc11, DSN-DVM-763).

use crate::relations::extensions_of;
use crate::rules::{eval_rule, governed};
use crate::sheets::cascade_state;
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::borrow::Cow;
use std::collections::BTreeMap;
use ubos_model::Registry;
use ubos_types::BpaError;
use ubos_version::{Repo, State, at};

/// The rule objects a candidate branch contributes (STD-LGS-071).
const CANDIDATES: &[&str] = &["GovernanceSheet", "PropertyDecl", "DecisionTable"];

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// `(effective declarations, failing validation names)` of an object over a state.
type Outcome = (BTreeMap<String, String>, Vec<String>);

impl Engine {
    #[allow(clippy::too_many_arguments)]
    fn outcome_in(
        &mut self,
        st: &State,
        reg_ext: &Registry,
        class: &str,
        body: &Value,
        state: Option<&str>,
        asof: &str,
        ctx: &Map<String, Value>,
    ) -> Result<Outcome, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), reg_ext);
        let (eff, _, _) = cascade_state(
            &mut repo,
            None,
            st,
            reg_ext,
            &mut self.rules,
            class,
            body,
            asof,
            ctx,
            state,
        )?;
        let mut values = BTreeMap::new();
        for (p, (v, _)) in &eff {
            values.insert(p.clone(), ubos_types::canonical(v)?);
        }
        let mut rules: Vec<Value> = reg_ext.class(class)?.validations.clone();
        if let Some((v, _)) = eff.get("validations") {
            rules.extend(v.as_array().cloned().unwrap_or_default());
        }
        let mut failing = Vec::new();
        for r in &rules {
            let rule = s(r, "rule").unwrap_or_default();
            if !eval_rule(reg_ext, class, rule, body, None, None, None)? {
                failing.push(s(r, "name").unwrap_or_default().to_string());
            }
        }
        failing.sort();
        Ok((values, failing))
    }

    /// Adapter operation `rule_impact` (STD-LGS-071): the current objects of the target
    /// whose effective declarations or failing validations differ under the source
    /// branch's sheets, property declarations and decision tables, per class.
    pub fn rule_impact(&mut self, i: &Value) -> Result<Value, BpaError> {
        let target = s(i, "branch").unwrap_or("main").to_string();
        let source = s(i, "source").unwrap_or_default().to_string();
        let kind = self.branch_meta(&target)?.kind;
        let (t_state, s_state) = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            repo.head(&source)?;
            (repo.state_at(&target, None)?, repo.state_at(&source, None)?)
        };
        let mut mixed = State::default();
        for (k, o) in &t_state.objects {
            if !CANDIDATES.contains(&o.class.as_str()) {
                mixed.objects.insert(k.clone(), o.clone());
            }
        }
        for (k, o) in &s_state.objects {
            if CANDIDATES.contains(&o.class.as_str()) {
                mixed.objects.insert(k.clone(), o.clone());
            }
        }
        let exts = extensions_of(
            &mut Repo::new(self.store.as_mut(), &self.registry),
            &t_state,
        )?;
        let reg_ext: Registry = if exts.is_empty() {
            self.registry.clone()
        } else {
            self.registry.with_extensions(&exts)?
        };
        let reg_ext = Cow::Owned::<Registry>(reg_ext);
        let mut ctx: Map<String, Value> = i
            .get("ctx")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        ctx.entry("branch_kind".to_string())
            .or_insert_with(|| json!(kind));
        let only: Option<Vec<String>> = i.get("classes").and_then(Value::as_array).map(|l| {
            l.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        });
        let mut by_class: BTreeMap<String, u64> = BTreeMap::new();
        let mut evaluated = 0u64;
        for o in t_state.objects.values() {
            if !governed(&reg_ext, &o.class) {
                continue;
            }
            let lin = reg_ext.class(&o.class)?.linearization.clone();
            if only
                .as_ref()
                .is_some_and(|c| !c.iter().any(|x| lin.contains(x)))
            {
                continue;
            }
            let asof = self.default_valid_of(&o.class);
            let (body, state) = {
                let mut repo = Repo::new(self.store.as_mut(), &self.registry);
                let head = repo.version(&o.head)?;
                if head["tombstone"] == json!(true) {
                    continue;
                }
                let v = match at(&o.timeline, &asof) {
                    Some(seg) => repo.version(&seg.version)?,
                    None => head,
                };
                (v["body"].clone(), crate::flow::state_of(&v))
            };
            evaluated += 1;
            let before = self.outcome_in(
                &t_state,
                &reg_ext,
                &o.class,
                &body,
                state.as_deref(),
                &asof,
                &ctx,
            )?;
            let after = self.outcome_in(
                &mixed,
                &reg_ext,
                &o.class,
                &body,
                state.as_deref(),
                &asof,
                &ctx,
            )?;
            if before != after {
                *by_class.entry(o.class.clone()).or_default() += 1;
            }
        }
        let changed: u64 = by_class.values().sum();
        Ok(
            json!({"branch": target, "source": source, "evaluated": evaluated,
                  "changed": changed, "by_class": by_class}),
        )
    }
}

// ---- rule tests (STD-LGS-072) and the test gate (STD-LGS-073), PH-2 step 5e ----

const RULE_TEST: &str = "RuleTest";
const TEST_KINDS: &[&str] = &["cascade", "decision", "table", "validation"];

fn incomplete(what: &str) -> BpaError {
    BpaError::new("MODEL.DECLARATION_INCOMPLETE", what.to_string())
}

/// STD-LGS-072: the declaration of a rule test.
fn check_test(b: &Value) -> Result<(), BpaError> {
    let kind = s(b, "kind").unwrap_or_default();
    if !TEST_KINDS.contains(&kind) {
        return Err(BpaError::new(
            "MODEL.INVALID_VALUE",
            "a rule test kind is cascade, decision, table or validation",
        ));
    }
    let (inp, exp) = (&b["input"], &b["expect"]);
    if !inp.is_object() || !exp.is_object() {
        return Err(incomplete("a rule test has an input and an expectation"));
    }
    if kind == "table" {
        if s(inp, "table").is_none() || !inp["inputs"].is_object() {
            return Err(incomplete("a table test names the table and its inputs"));
        }
    } else {
        if s(inp, "class").is_none() || !inp["body"].is_object() {
            return Err(incomplete(
                "a rule test names the class and the body of its input object",
            ));
        }
        if kind == "decision" && s(inp, "port").is_none() {
            return Err(incomplete("a decision test names the port"));
        }
    }
    let key = match kind {
        "cascade" => "properties",
        "decision" => "outcome",
        "table" => "outputs",
        _ => "violations",
    };
    if exp.get(key).is_none() && exp.get("error").is_none() {
        return Err(incomplete(&format!(
            "the expectation of a {kind} test has {key} or error"
        )));
    }
    Ok(())
}

/// STD-LGS-072 (BPA 1.0-rc11): the rule tests the commit wrote.
pub(crate) fn check(
    repo: &mut Repo,
    tx: &mut ubos_version::Tx,
    written: &[(String, String)],
) -> Result<(), BpaError> {
    for (_, vh) in written {
        let v = repo.version_in(tx, vh)?;
        if s(&v, "class") == Some(RULE_TEST) && v["tombstone"] != json!(true) {
            check_test(&v["body"])?;
        }
    }
    Ok(())
}

/// What the tests hit: sheet blocks, table rows and validations.
#[derive(Default)]
struct Hits {
    blocks: std::collections::BTreeSet<(String, usize)>,
    rows: std::collections::BTreeSet<(String, usize)>,
    validations: std::collections::BTreeSet<String>,
}

impl Engine {
    /// The live objects of `classes` in a state, with their head bodies, by object ID.
    fn live_in(&mut self, st: &State, classes: &[&str]) -> Result<Vec<(String, Value)>, BpaError> {
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let mut out = Vec::new();
        for (oid, o) in &st.objects {
            if classes.contains(&o.class.as_str()) {
                let v = repo.version(&o.head)?;
                if v["tombstone"] != json!(true) {
                    out.push((oid.clone(), v["body"].clone()));
                }
            }
        }
        Ok(out)
    }

    /// The table a test names by object ID or URI.
    fn table_in(
        &mut self,
        st: &State,
        r: &str,
    ) -> Result<(String, ubos_version::ObjectState), BpaError> {
        let found = if let Some(rest) = r.strip_prefix("ubos://") {
            let slug = rest.rsplit('/').next().unwrap_or_default();
            st.objects
                .iter()
                .find(|(_, o)| o.class == "DecisionTable" && o.slug == slug)
        } else {
            st.objects
                .get_key_value(r)
                .filter(|(_, o)| o.class == "DecisionTable")
        };
        found
            .map(|(k, o)| (k.clone(), o.clone()))
            .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown decision table"))
    }

    fn test_actual(
        &mut self,
        st: &State,
        branch: &str,
        kind: &str,
        inp: &Value,
        exp: &Value,
        hits: &mut Hits,
    ) -> Result<Value, BpaError> {
        if kind == "table" {
            let (oid, o) = self.table_in(st, s(inp, "table").unwrap_or_default())?;
            let asof = s(inp, "asof")
                .map(str::to_string)
                .unwrap_or_else(|| self.now[..10].to_string());
            let seg = at(&o.timeline, &asof).cloned().ok_or_else(|| {
                BpaError::new("QRY.NOT_FOUND_AT_TIME", "no version of the table at asof")
            })?;
            let body =
                Repo::new(self.store.as_mut(), &self.registry).version(&seg.version)?["body"]
                    .clone();
            let r = ubos_rules::tables::evaluate(&body, &inp["inputs"])?;
            for n in r["rows"].as_array().into_iter().flatten() {
                hits.rows
                    .insert((oid.clone(), n.as_u64().unwrap_or_default() as usize));
            }
            return Ok(json!({"outputs": r["outputs"]}));
        }
        let class = s(inp, "class").unwrap_or_default().to_string();
        self.registry.class(&class)?;
        let body = &inp["body"];
        let kind_b = self.branch_meta(branch)?.kind;
        let mut ctx: Map<String, Value> = inp
            .get("ctx")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        ctx.entry("branch_kind".to_string())
            .or_insert_with(|| json!(kind_b));
        let asof = self.default_valid_of(&class);
        let exts = extensions_of(&mut Repo::new(self.store.as_mut(), &self.registry), st)?;
        let reg_ext: Registry = if exts.is_empty() {
            self.registry.clone()
        } else {
            self.registry.with_extensions(&exts)?
        };
        let eff = {
            let mut repo = Repo::new(self.store.as_mut(), &reg_ext);
            cascade_state(
                &mut repo,
                None,
                st,
                &reg_ext,
                &mut self.rules,
                &class,
                body,
                &asof,
                &ctx,
                None,
            )?
            .0
        };
        for (_, cands) in eff.values() {
            for c in cands {
                hits.blocks.insert((c.position.0.clone(), c.position.1));
            }
        }
        let value = |p: &str| eff.get(p).map(|(v, _)| v.clone()).unwrap_or(Value::Null);
        Ok(match kind {
            "cascade" => {
                let mut props = Map::new();
                for p in exp["properties"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(k, _)| k)
                {
                    props.insert(p.clone(), value(p));
                }
                json!({"properties": props})
            }
            "decision" => {
                let r = self.decide_body(
                    st,
                    branch,
                    &class,
                    body,
                    None,
                    s(inp, "port").unwrap_or_default(),
                    inp.get("params")
                        .filter(|p| !p.is_null())
                        .unwrap_or(&json!({})),
                    &ctx,
                    None,
                )?;
                let mut a = json!({"outcome": r["outcome"]});
                if r["steps"].as_array().is_some_and(|x| !x.is_empty()) {
                    a["steps"] = r["steps"].clone();
                }
                a
            }
            _ => {
                let mut rules: Vec<Value> = reg_ext.class(&class)?.validations.clone();
                rules.extend(value("validations").as_array().cloned().unwrap_or_default());
                let mut failing = Vec::new();
                for r in &rules {
                    let name = s(r, "name").unwrap_or_default().to_string();
                    hits.validations.insert(name.clone());
                    if !eval_rule(
                        &reg_ext,
                        &class,
                        s(r, "rule").unwrap_or_default(),
                        body,
                        None,
                        None,
                        None,
                    )? {
                        failing.push(name);
                    }
                }
                failing.sort();
                json!({"violations": failing})
            }
        })
    }

    /// One rule test: `{test_id, name, kind, passed, actual}`.
    fn run_test(
        &mut self,
        st: &State,
        branch: &str,
        tid: &str,
        b: &Value,
        hits: &mut Hits,
    ) -> Result<Value, BpaError> {
        let kind = s(b, "kind").unwrap_or_default();
        let exp = &b["expect"];
        let actual = match self.test_actual(st, branch, kind, &b["input"], exp, hits) {
            Ok(a) => a,
            Err(e) => json!({"error": e.code}),
        };
        let passed = if exp.get("error").is_some() || actual.get("error").is_some() {
            exp.get("error") == actual.get("error")
        } else {
            match kind {
                "decision" => {
                    actual["outcome"] == exp["outcome"]
                        && (exp.get("steps").is_none()
                            || exp["steps"] == actual.get("steps").cloned().unwrap_or(json!([])))
                }
                "validation" => {
                    let mut want: Vec<String> = exp["violations"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect();
                    want.sort();
                    actual["violations"] == json!(want)
                }
                "cascade" => actual["properties"] == exp["properties"],
                _ => actual["outputs"] == exp["outputs"],
            }
        };
        Ok(
            json!({"test_id": tid, "name": b["name"], "kind": kind, "passed": passed, "actual": actual}),
        )
    }

    fn run_tests_in(
        &mut self,
        st: &State,
        branch: &str,
        ids: Option<&[String]>,
    ) -> Result<(Vec<Value>, Hits), BpaError> {
        let mut hits = Hits::default();
        let mut results = Vec::new();
        for (tid, b) in self.live_in(st, &[RULE_TEST])? {
            if ids.is_some_and(|ids| !ids.contains(&tid)) {
                continue;
            }
            results.push(self.run_test(st, branch, &tid, &b, &mut hits)?);
        }
        Ok((results, hits))
    }

    /// Adapter operation `run_rule_tests` (STD-LGS-072): run the rule tests of a branch, all
    /// or those named, and report their results and the coverage of the rules.
    pub fn run_rule_tests(&mut self, i: &Value) -> Result<Value, BpaError> {
        let branch = s(i, "branch").unwrap_or("main").to_string();
        let time = i.get("time").and_then(Value::as_u64);
        let st = Repo::new(self.store.as_mut(), &self.registry).state_at(&branch, time)?;
        let ids: Option<Vec<String>> = i.get("tests").and_then(Value::as_array).map(|l| {
            l.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        });
        let (results, hits) = self.run_tests_in(&st, &branch, ids.as_deref())?;
        let mut blocks = std::collections::BTreeSet::new();
        let mut validations = std::collections::BTreeSet::new();
        for (sid, b) in self.live_in(&st, &["GovernanceSheet"])? {
            for (n, blk) in b["ast"]["blocks"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                blocks.insert((sid.clone(), n));
                for d in blk["declarations"].as_array().into_iter().flatten() {
                    if s(d, "property") == Some("validations") {
                        for v in d["value"].as_array().into_iter().flatten() {
                            if let Some(n) = s(v, "name") {
                                validations.insert(n.to_string());
                            }
                        }
                    }
                }
            }
        }
        let mut rows = std::collections::BTreeSet::new();
        for (tid, b) in self.live_in(&st, &["DecisionTable"])? {
            for n in 0..b["rows"].as_array().map_or(0, Vec::len) {
                rows.insert((tid.clone(), n));
            }
        }
        for c in self.registry.classes() {
            for v in &c.validations {
                if let Some(n) = s(v, "name") {
                    validations.insert(n.to_string());
                }
            }
        }
        let cov = |hit: usize, total: usize| json!({"hit": hit, "total": total});
        let passed = results
            .iter()
            .filter(|r| r["passed"] == json!(true))
            .count();
        Ok(
            json!({"results": results, "passed": passed, "failed": results.len() - passed,
                  "coverage": {
                      "blocks": cov(hits.blocks.intersection(&blocks).count(), blocks.len()),
                      "rows": cov(hits.rows.intersection(&rows).count(), rows.len()),
                      "validations": cov(hits.validations.intersection(&validations).count(), validations.len())}}),
        )
    }

    /// STD-LGS-073 (BPA 1.0-rc11): before a change set merges into a protected branch, the
    /// rule tests the change affects, run at the head of the source, must pass.
    pub(crate) fn test_gate(&mut self, source: &str, target: &str) -> Result<(), BpaError> {
        let (s_state, t_state) = {
            let mut repo = Repo::new(self.store.as_mut(), &self.registry);
            if !ubos_version::PROTECTED.contains(&repo.branch_meta(target)?.kind.as_str()) {
                return Ok(());
            }
            (repo.state_at(source, None)?, repo.state_at(target, None)?)
        };
        let changed: Vec<&String> = s_state
            .objects
            .iter()
            .filter(|(k, o)| t_state.objects.get(*k).is_none_or(|t| t.head != o.head))
            .map(|(k, _)| k)
            .collect();
        let rules_changed = changed.iter().any(|k| {
            let c = s_state.objects[*k].class.as_str();
            CANDIDATES.contains(&c) || c == "Logic" || c == "ClassExtension"
        });
        let mut ids = Vec::new();
        for (tid, b) in self.live_in(&s_state, &[RULE_TEST])? {
            let tgt = s(&b, "target");
            if changed.contains(&&tid)
                || tgt.is_some_and(|t| changed.iter().any(|c| c.as_str() == t))
                || (tgt.is_none() && rules_changed)
            {
                ids.push(tid);
            }
        }
        if ids.is_empty() {
            return Ok(());
        }
        let (results, _) = self.run_tests_in(&s_state, source, Some(&ids))?;
        let failed: Vec<Value> = results
            .iter()
            .filter(|r| r["passed"] != json!(true))
            .map(|r| r["test_id"].clone())
            .collect();
        if failed.is_empty() {
            Ok(())
        } else {
            Err(BpaError::with_details(
                "RULE.TESTS_FAILED",
                json!({"failed": failed}),
            ))
        }
    }
}
