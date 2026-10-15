//! The process state machine of DSN-DVM-501 (UBS-SYS-DVM-05 §1; PH-1 step 12c, DEC-071):
//! a process is created by the host API, runs from its first instruction, is staged when
//! its commit has writes, flushes under the branch lock and ends committed, aborted or
//! deferred. Only the transitions of the table are possible; the phases a process passed
//! are recorded with it (`process_info`, `phases` and `attempts`). The externally visible
//! states of `DAT-Process` (STD-TXN-001: `running`, `committed`, `aborted`, `failed`) are
//! unchanged: `aborted` here is recorded as `failed` when the process ended in an error.

use serde_json::{Value, json};

/// A phase of the process state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Created by the host API with a context; no instruction ran yet.
    Created,
    /// Executing instructions.
    Running,
    /// `commit` with staged writes; pre-lock validation.
    Staged,
    /// Enqueued for the branch lock; CAS, validation and storage commit.
    Flushing,
    /// The commit is visible, or the process had nothing to commit (terminal).
    Committed,
    /// Ended without a commit (terminal).
    Aborted,
    /// A decision returned `require`; a pending change was written (terminal).
    Deferred,
}

/// An event of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The runtime dispatches the first instruction.
    FirstInstruction,
    /// `commit` without staged writes.
    CommitEmpty,
    /// `commit` with staged writes.
    CommitStaged,
    /// Pre-lock validation fails.
    ValidationFailed,
    /// A decision returns `require`.
    Requires,
    /// Enqueued by the sequencer.
    Enqueued,
    /// CAS and validation pass, the storage commit succeeds.
    Flushed,
    /// `TXN.HEAD_MOVED` or `TXN.READ_SET_CHANGED`; `retry` when the process is re-executable
    /// and attempts remain.
    Conflict {
        /// Whether the process is re-executed.
        retry: bool,
    },
    /// Limit, cancel or sandbox fault.
    Fault,
    /// Storage failure under the lock.
    StorageFailure,
}

impl Phase {
    /// The name as recorded.
    pub fn name(self) -> &'static str {
        match self {
            Phase::Created => "created",
            Phase::Running => "running",
            Phase::Staged => "staged",
            Phase::Flushing => "flushing",
            Phase::Committed => "committed",
            Phase::Aborted => "aborted",
            Phase::Deferred => "deferred",
        }
    }

    /// Terminal phases absorb every event.
    pub fn is_terminal(self) -> bool {
        matches!(self, Phase::Committed | Phase::Aborted | Phase::Deferred)
    }
}

/// The transition table: the phase an event leads to, or `None` for a transition the
/// table does not list (illegal).
pub fn step(from: Phase, event: Event) -> Option<Phase> {
    use Event::*;
    use Phase::*;
    Some(match (from, event) {
        (Created, FirstInstruction) => Running,
        (Running, CommitEmpty) => Committed,
        (Running, CommitStaged) => Staged,
        (Staged, ValidationFailed) => Aborted,
        (Staged, Requires) => Deferred,
        (Staged, Enqueued) => Flushing,
        (Flushing, Flushed) => Committed,
        (Flushing, Conflict { retry: true }) => Running,
        (Flushing, Conflict { retry: false }) => Aborted,
        (Running | Staged, Fault) => Aborted,
        (Flushing, StorageFailure) => Aborted,
        _ => return None,
    })
}

/// The machine of one process: its phase and the phases passed.
#[derive(Debug, Clone)]
pub struct Machine {
    /// Current phase.
    pub phase: Phase,
    /// Every phase entered, the first being `created`.
    pub phases: Vec<Phase>,
    /// Attempts started (re-executions count).
    pub attempts: u64,
}

impl Default for Machine {
    fn default() -> Self {
        Machine::new()
    }
}

impl Machine {
    /// A process just created.
    pub fn new() -> Machine {
        Machine {
            phase: Phase::Created,
            phases: vec![Phase::Created],
            attempts: 0,
        }
    }

    /// Fire an event; an illegal transition is a kernel invariant violation and is reported
    /// as an error rather than applied.
    pub fn fire(&mut self, event: Event) -> Result<Phase, String> {
        let to = step(self.phase, event)
            .ok_or_else(|| format!("illegal process transition {:?} on {event:?}", self.phase))?;
        self.phase = to;
        self.phases.push(to);
        Ok(to)
    }

    /// The phases as recorded with the process.
    pub fn to_json(&self) -> Value {
        json!(self.phases.iter().map(|p| p.name()).collect::<Vec<_>>())
    }
}
