//! Root verifier of the DVM (DSN-DVM-092, STD-PROOF-001).
//!
//! `verify_roots` recomputes the state-tree root of every commit of a branch in a `cseq`
//! range from the committed state and compares it with the root recorded in the commit;
//! it then walks the stored tree nodes from each root and checks that every node's content
//! hashes to its identifier. The index and sequence verifiers live in `ubos-store`
//! (`ubos_store::verify`).

use crate::stateful::Engine;
use std::collections::BTreeSet;
use ubos_store::verify::{Finding, Report};
use ubos_tree::{Node, tree_root};
use ubos_types::BpaError;
use ubos_version::Repo;

/// Verify the state roots and stored tree nodes of a branch for commits with `cseq` in
/// `[from, to]`.
pub fn verify_roots(
    engine: &mut Engine,
    branch: &str,
    from: u64,
    to: u64,
) -> Result<Report, BpaError> {
    let mut report = Report::default();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut repo = Repo::new(engine.store.as_mut(), &engine.registry);
    let head = repo.commit_at(branch, None)?;
    let head_cseq = repo.commit(head)?.cseq;
    for cseq in from..=to.min(head_cseq) {
        let id = repo.commit_at(branch, Some(cseq))?;
        let rec = repo.commit(id)?;
        let st = repo.commit_state(id)?;
        let root = tree_root(&repo.state_entries(&st)?)?;
        report.checked += 1;
        if root != rec.root {
            report.findings.push(Finding {
                check: "root",
                key: format!("{branch}@{cseq}"),
                detail: format!("recorded {}, recomputed {root}", rec.root),
            });
        }
        let mut stack = vec![rec.root.clone()];
        while let Some(h) = stack.pop() {
            if !seen.insert(h.clone()) {
                continue;
            }
            let node = match engine_node(&mut repo, &h) {
                Ok(n) => n,
                Err(e) => {
                    report.findings.push(Finding {
                        check: "node",
                        key: h.clone(),
                        detail: e.to_string(),
                    });
                    continue;
                }
            };
            if node.hash()? != h {
                report.findings.push(Finding {
                    check: "node",
                    key: h.clone(),
                    detail: "content does not hash to the node identifier".into(),
                });
            }
            if node.level > 0 {
                stack.extend(node.entries.iter().map(|(_, c)| c.clone()));
            }
        }
    }
    Ok(report)
}

fn engine_node(repo: &mut Repo<'_>, hash: &str) -> Result<Node, BpaError> {
    Node::from_json(&repo.store.node(hash)?)
}
