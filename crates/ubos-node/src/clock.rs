//! The node's clock (PH-1 step 18b, DEC-079): the system clock, or in the `dev` preset a
//! test clock that an operator sets or advances through `/api/v1/admin/clock`, so that
//! scenarios over UBTP can let time pass (SCN-006, SCN-007). The kernel clock of every VAE,
//! the became-effective timers, the schedule evaluation and the event-log sweep follow it;
//! sessions, leases and the job queue keep the system clock.

use std::sync::Mutex;
use ubos_dvm::types::BpaError;

/// The fixed instant of the test clock, if set.
static TEST_NOW: Mutex<Option<String>> = Mutex::new(None);

fn system_now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
}

/// The current instant of the node.
pub fn now() -> chrono::DateTime<chrono::Utc> {
    let fixed = TEST_NOW.lock().ok().and_then(|g| g.clone());
    fixed
        .and_then(|t| ubos_dvm::types::time::parse_instant(&t).ok())
        .unwrap_or_else(system_now)
}

/// The current instant of the node, canonical text (STD-TIME-001).
pub fn now_text() -> String {
    ubos_dvm::types::time::format_instant(&now())
}

/// Whether the test clock is set.
pub fn is_test() -> bool {
    TEST_NOW.lock().map(|g| g.is_some()).unwrap_or(false)
}

/// Set the test clock to an instant, or return to the system clock with `None`.
pub fn set(at: Option<&str>) -> Result<(), BpaError> {
    let t = match at {
        Some(t) => Some(ubos_dvm::types::time::format_instant(
            &ubos_dvm::types::time::parse_instant(t)?,
        )),
        None => None,
    };
    if let Ok(mut g) = TEST_NOW.lock() {
        *g = t;
    }
    Ok(())
}

/// Advance the test clock by `<n>d|h|m|s` (from the current instant when it is not set).
pub fn advance(by: &str) -> Result<String, BpaError> {
    let invalid = || BpaError::new("MODEL.INVALID_VALUE", "advance is <n>d|h|m|s");
    let (n, unit) = by.split_at(by.len().saturating_sub(1));
    let n: i64 = n.parse().map_err(|_| invalid())?;
    let secs = match unit {
        "d" => n * 86_400,
        "h" => n * 3_600,
        "m" => n * 60,
        "s" => n,
        _ => return Err(invalid()),
    };
    if secs < 0 {
        return Err(BpaError::new(
            "MODEL.INVALID_VALUE",
            "the clock only advances",
        ));
    }
    let t = ubos_dvm::types::time::format_instant(
        &(now() + ubos_dvm::types::time::Duration::seconds(secs)),
    );
    set(Some(&t))?;
    Ok(t)
}

/// The kernel clock of a VAE (CTR-002 `clock.now`).
pub struct NodeClock;

impl ubos_dvm::host::Clock for NodeClock {
    fn now(&self) -> String {
        now_text()
    }
}
