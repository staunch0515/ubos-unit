//! Leadership leases of the nodes (DSN-NOD-228, CTR-008; PH-1 step 15a, DEC-076): a named
//! lease is held by one node until it expires; every change of holder increases the fencing
//! token, so a node that lost its lease and acts on an old token can be refused. Default
//! lease 15 s, renewed every 5 s.

use crate::db::{Db, Param};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use ubos_dvm::types::BpaError;

/// Default lease length in milliseconds.
pub const LEASE_MS: i64 = 15_000;
/// Renewal interval in milliseconds.
pub const RENEW_MS: i64 = 5_000;

/// A lease service.
pub trait Leases: Send + Sync {
    /// Acquire or renew the lease `name` for `holder` for `ttl_ms`: `Some(token)` when the
    /// holder has it (the same token on a renewal, a larger one when it changed hands),
    /// `None` when another holder's lease is live.
    fn acquire(&self, name: &str, holder: &str, ttl_ms: i64) -> Result<Option<i64>, BpaError>;
    /// Give a lease up (a clean shutdown).
    fn release(&self, name: &str, holder: &str) -> Result<(), BpaError>;
    /// The current token of a lease, live or not (a fencing check).
    fn token(&self, name: &str) -> Result<Option<i64>, BpaError>;
}

/// Leases in memory (the `memory` preset, suites); `now_ms` may be fixed by suites.
#[derive(Default)]
pub struct MemLeases {
    rows: Mutex<HashMap<String, (String, i64, i64)>>,
    fixed_now: Mutex<Option<i64>>,
}

impl MemLeases {
    /// Fix the clock in milliseconds (suites); `None` returns to the system clock.
    pub fn set_now(&self, at: Option<i64>) {
        if let Ok(mut g) = self.fixed_now.lock() {
            *g = at;
        }
    }

    fn now(&self) -> i64 {
        if let Ok(g) = self.fixed_now.lock()
            && let Some(t) = *g
        {
            return t;
        }
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default()
    }
}

fn lock_err() -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", "lease lock")
}

impl Leases for MemLeases {
    fn acquire(&self, name: &str, holder: &str, ttl_ms: i64) -> Result<Option<i64>, BpaError> {
        let now = self.now();
        let mut rows = self.rows.lock().map_err(|_| lock_err())?;
        let next = match rows.get(name) {
            None => 1,
            Some((h, t, exp)) if h == holder && *exp > now => *t,
            Some((_, _, exp)) if *exp > now => return Ok(None),
            Some((_, t, _)) => t + 1,
        };
        rows.insert(name.to_string(), (holder.to_string(), next, now + ttl_ms));
        Ok(Some(next))
    }
    fn release(&self, name: &str, holder: &str) -> Result<(), BpaError> {
        let mut rows = self.rows.lock().map_err(|_| lock_err())?;
        if let Some(r) = rows.get_mut(name)
            && r.0 == holder
        {
            r.2 = 0;
        }
        Ok(())
    }
    fn token(&self, name: &str) -> Result<Option<i64>, BpaError> {
        Ok(self
            .rows
            .lock()
            .map_err(|_| lock_err())?
            .get(name)
            .map(|r| r.1))
    }
}

/// Leases in the node's `node_lease` table (owner role; migration 0009).
pub struct PgLeases {
    db: Arc<Db>,
}

impl PgLeases {
    /// Leases over the owner database of a node.
    pub fn new(db: Arc<Db>) -> PgLeases {
        PgLeases { db }
    }
}

impl Leases for PgLeases {
    fn acquire(&self, name: &str, holder: &str, ttl_ms: i64) -> Result<Option<i64>, BpaError> {
        // one statement: a live lease of another holder is left alone; a renewal keeps the
        // token; a takeover after expiry increments it
        let rows = self.db.query(
            "INSERT INTO node_lease (name, holder, token, expires_at) VALUES ($1, $2, 1, now() + make_interval(secs => $3::bigint / 1000.0)) \
             ON CONFLICT (name) DO UPDATE SET \
               token = CASE WHEN node_lease.holder = EXCLUDED.holder AND node_lease.expires_at > now() THEN node_lease.token ELSE node_lease.token + 1 END, \
               acquired_at = CASE WHEN node_lease.holder = EXCLUDED.holder AND node_lease.expires_at > now() THEN node_lease.acquired_at ELSE now() END, \
               holder = EXCLUDED.holder, expires_at = EXCLUDED.expires_at \
             WHERE node_lease.holder = EXCLUDED.holder OR node_lease.expires_at <= now() \
             RETURNING token",
            &[Param::Text(name), Param::Text(holder), Param::Int(ttl_ms)],
        )?;
        Ok(rows.first().and_then(|r| r["token"].as_i64()))
    }
    fn release(&self, name: &str, holder: &str) -> Result<(), BpaError> {
        self.db.execute(
            "UPDATE node_lease SET expires_at = now() WHERE name = $1 AND holder = $2",
            &[Param::Text(name), Param::Text(holder)],
        )?;
        Ok(())
    }
    fn token(&self, name: &str) -> Result<Option<i64>, BpaError> {
        let rows = self.db.query(
            "SELECT token FROM node_lease WHERE name = $1",
            &[Param::Text(name)],
        )?;
        Ok(rows.first().and_then(|r| r["token"].as_i64()))
    }
}
