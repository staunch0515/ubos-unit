//! Lifecycles in the kernel (BPA 1.0-rc3, STD-CLS-004, STD-ISA-036; PH-1 step 12b,
//! DEC-071; DSN-DVM-730…732): the lifecycle in force for a class on a state, the checks of
//! the lifecycle objects a commit writes, the lifecycle state of an object and the adapter
//! operation `actions`. The pure parts (shape, extension rule, findings, guards, the action
//! list) are `ubos-flow`.

use crate::relations::{extensions_of, uri_of};
use crate::sheets::version_of;
use crate::stateful::Engine;
use serde_json::{Map, Value, json};
use std::borrow::Cow;
use std::collections::BTreeMap;
use ubos_model::Registry;
use ubos_types::BpaError;
use ubos_version::{ObjectState, Repo, State, Tx};

/// A lifecycle in force: `(object_id, version_hash, body)`.
pub(crate) type Lifecycle = (String, String, Value);
/// A principal's permission check: `permit(action, class)`.
pub(crate) type Permit<'a> = &'a mut dyn FnMut(&str, &str) -> bool;

/// The `Lifecycle` objects a commit or a read can see: those of the branch head (loaded
/// without the rest of the state) and those of the transaction, which win.
pub(crate) struct Lifecycles {
    head: BTreeMap<String, ObjectState>,
    authority: String,
}

impl Lifecycles {
    /// The lifecycle objects at a branch head (`cseq`) for a commit.
    pub(crate) fn at(repo: &mut Repo<'_>, branch: &str, cseq: u64) -> Result<Lifecycles, BpaError> {
        Ok(Lifecycles {
            head: repo.objects_in(branch, cseq, Some("Lifecycle"))?,
            authority: repo.store.authority(),
        })
    }

    /// The lifecycle objects of a materialised state (reads and the executor).
    pub(crate) fn of(repo: &Repo<'_>) -> Lifecycles {
        Lifecycles {
            head: BTreeMap::new(),
            authority: repo.store.authority(),
        }
    }

    fn object(&self, st: &State, uri: &str) -> Option<(String, ObjectState)> {
        let rest = uri.strip_prefix("ubos://")?;
        let mut parts = rest.split('/');
        let (auth, class, slug) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() || auth != self.authority || class != "Lifecycle" {
            return None;
        }
        st.objects
            .iter()
            .chain(self.head.iter())
            .find(|(_, o)| o.class == "Lifecycle" && o.slug == slug)
            .map(|(id, o)| (id.clone(), o.clone()))
    }

    /// The live lifecycle object at a URI, or `MODEL.REFERENCE_INVALID`.
    pub(crate) fn resolve(
        &self,
        repo: &mut Repo<'_>,
        tx: Option<&Tx>,
        st: &State,
        uri: &str,
    ) -> Result<Lifecycle, BpaError> {
        let Some((oid, o)) = self.object(st, uri) else {
            return Err(BpaError::new(
                "MODEL.REFERENCE_INVALID",
                format!("lifecycle {uri}"),
            ));
        };
        let v = version_of(repo, tx, &o.head)?;
        if v["tombstone"] == json!(true) {
            return Err(BpaError::new(
                "MODEL.REFERENCE_INVALID",
                format!("lifecycle {uri}"),
            ));
        }
        Ok((oid, o.head.clone(), v["body"].clone()))
    }

    /// The lifecycle in force for a class (STD-CLS-004), or none.
    pub(crate) fn lifecycle(
        &self,
        repo: &mut Repo<'_>,
        tx: Option<&Tx>,
        st: &State,
        reg: &Registry,
        class: &str,
    ) -> Result<Option<Lifecycle>, BpaError> {
        match &reg.class(class)?.lifecycle {
            Some(uri) => self.resolve(repo, tx, st, uri).map(Some),
            None => Ok(None),
        }
    }

    /// STD-CLS-004: the shape of the lifecycle objects written by a commit and the
    /// extension rule against the lifecycles of the classes naming them (FR-FLOW-015).
    pub(crate) fn check_written(
        &self,
        repo: &mut Repo<'_>,
        tx: &Tx,
        reg: &Registry,
        written: &[(String, String)],
    ) -> Result<(), BpaError> {
        for (oid, vh) in written {
            if tx.state.objects.get(oid).map(|o| o.class.as_str()) != Some("Lifecycle") {
                continue;
            }
            let v = repo.version_in(tx, vh)?;
            if v["tombstone"] == json!(true) {
                continue;
            }
            let body = v["body"].clone();
            ubos_flow::check_lifecycle(&body)?;
            let o = &tx.state.objects[oid];
            let uri = uri_of(&self.authority, "Lifecycle", &o.slug);
            let own = |c: &str| -> Option<String> {
                reg.decl(c)
                    .ok()
                    .and_then(|d| d.get("lifecycle").and_then(Value::as_str))
                    .map(str::to_string)
            };
            let names: Vec<String> = reg.classes().map(|c| c.name.clone()).collect();
            for name in &names {
                if own(name).as_deref() != Some(uri.as_str()) {
                    continue;
                }
                let lin = reg.class(name)?.linearization.clone();
                for anc in &lin[1..] {
                    if let Some(puri) = own(anc).filter(|p| p != &uri)
                        && let Some((_, po)) = self.object(&tx.state, &puri)
                    {
                        let pv = repo.version_in(tx, &po.head)?;
                        if pv["tombstone"] != json!(true) && !ubos_flow::extends(&pv["body"], &body)
                        {
                            return Err(BpaError::new(
                                "MODEL.NARROWING_VIOLATION",
                                format!("lifecycle of {name} does not extend {anc}"),
                            ));
                        }
                    }
                }
                for other in &names {
                    if other == name || !reg.class(other)?.linearization.contains(name) {
                        continue;
                    }
                    if let Some(curi) = own(other).filter(|c| c != &uri)
                        && let Some((_, co)) = self.object(&tx.state, &curi)
                    {
                        let cv = repo.version_in(tx, &co.head)?;
                        if cv["tombstone"] != json!(true) && !ubos_flow::extends(&body, &cv["body"])
                        {
                            return Err(BpaError::new(
                                "MODEL.NARROWING_VIOLATION",
                                format!("lifecycle of {other} does not extend {name}"),
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// The lifecycle state of an object's head version.
pub(crate) fn state_of(v: &Value) -> Option<String> {
    v.get("state").and_then(Value::as_str).map(str::to_string)
}

impl Engine {
    /// STD-ISA-036: the ports of an object with their availability and reasons; `permit`
    /// is the principal's permission check (none: every port permitted).
    pub(crate) fn actions_in(
        &mut self,
        st: &State,
        branch: &str,
        oid: &str,
        ctx: &Map<String, Value>,
        permit: Option<Permit<'_>>,
    ) -> Result<Value, BpaError> {
        let Some(o) = st.objects.get(oid).cloned() else {
            return Err(BpaError::new("IAM.NOT_FOUND", "unknown object"));
        };
        let mut repo = Repo::new(self.store.as_mut(), &self.registry);
        let head = repo.version(&o.head)?;
        if head["tombstone"] == json!(true) {
            return Err(BpaError::new("IAM.NOT_FOUND", "unknown object"));
        }
        let exts = extensions_of(&mut repo, st)?;
        let reg: Cow<Registry> = if exts.is_empty() {
            Cow::Borrowed(&self.registry)
        } else {
            Cow::Owned(self.registry.with_extensions(&exts)?)
        };
        let reg = reg.into_owned();
        let lc = Lifecycles::of(&repo).lifecycle(&mut repo, None, st, &reg, &o.class)?;
        let eff = reg.class(&o.class)?.clone();
        let state = state_of(&head);
        let body = head["body"].clone();
        let mut allow = |_: &str, _: &str| true;
        let permit: Permit<'_> = match permit {
            Some(p) => p,
            None => &mut allow,
        };
        let empty = json!({});
        let mut decide = |port: &str| self.decide_in(st, branch, oid, port, &empty, ctx);
        ubos_flow::actions(
            &reg,
            &o.class,
            &eff.ports,
            lc.as_ref().map(|l| &l.2),
            state.as_deref(),
            &body,
            ctx,
            permit,
            &mut decide,
        )
    }

    /// The adapter operation `actions` (STD-CONF-005, since 1.0-rc3): every port permitted,
    /// in the context given.
    pub fn actions(&mut self, input: &Value) -> Result<Value, BpaError> {
        let branch = input
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string();
        let oid = input
            .get("object_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let kind = self.branch_meta(&branch)?.kind;
        let (_, _, ev, st) = self.object_view(&branch, input, &oid)?;
        let st = match st {
            Some(s) => s,
            None => {
                let time = input.get("time").and_then(Value::as_u64);
                Repo::new(self.store.as_mut(), &self.registry).state_at(&branch, time)?
            }
        };
        let mut ctx = input
            .get("ctx")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let vae = self.store.authority();
        for (k, d) in [
            ("principal_kind", "human"),
            ("branch_kind", kind.as_str()),
            ("vae", vae.as_str()),
            ("channel", "api"),
        ] {
            ctx.entry(k.to_string()).or_insert_with(|| json!(d));
        }
        let mut out = self.actions_in(&st, &branch, &oid, &ctx, None)?;
        out["evaluated_at"] = json!(ev);
        Ok(out)
    }
}
