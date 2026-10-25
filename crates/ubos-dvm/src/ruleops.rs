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
