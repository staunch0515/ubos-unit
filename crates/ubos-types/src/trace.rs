//! Phase spans of the kernel for the node's traces (PH-1 step 16c, DSN-NOD-610).
//!
//! A host that wants the phases of an operation calls [`begin`] on the thread that runs
//! it and [`take`] afterwards; in between, [`span`] guards record `{name, start, end}` in
//! Unix nanoseconds. Without [`begin`] a guard records nothing and costs a thread-local
//! read. The kernel names its phases `dvm.<phase>` and the stores `store.<operation>`.

use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

/// A recorded phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phase {
    /// `dvm.rules`, `dvm.flush`, `store.flush`, …
    pub name: &'static str,
    /// Start, Unix nanoseconds.
    pub start_ns: u64,
    /// End, Unix nanoseconds.
    pub end_ns: u64,
}

thread_local! {
    static PHASES: RefCell<Option<Vec<Phase>>> = const { RefCell::new(None) };
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or_default()
}

/// Start collecting the phases of this thread.
pub fn begin() {
    PHASES.with(|p| *p.borrow_mut() = Some(Vec::new()));
}

/// Stop collecting and return the phases recorded since [`begin`].
pub fn take() -> Vec<Phase> {
    PHASES.with(|p| p.borrow_mut().take().unwrap_or_default())
}

/// A phase in progress; it is recorded when dropped.
pub struct Guard {
    name: &'static str,
    start_ns: u64,
    active: bool,
}

/// Start a phase.
pub fn span(name: &'static str) -> Guard {
    let active = PHASES.with(|p| p.borrow().is_some());
    Guard {
        name,
        start_ns: if active { now_ns() } else { 0 },
        active,
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.active {
            let ph = Phase {
                name: self.name,
                start_ns: self.start_ns,
                end_ns: now_ns(),
            };
            PHASES.with(|p| {
                if let Some(v) = p.borrow_mut().as_mut() {
                    v.push(ph);
                }
            });
        }
    }
}
