//! Backups, point-in-time restore and upgrade verification (PH-1 step 16e, DEC-077;
//! DSN-NOD-620, -623, -633; FR-OPS-021; NR-DUR-002…-004; NR-OPER-002; SCN-017).
//!
//! **Layout.** Everything lives under `backup.dir`, a directory the operator mounts from a
//! separate failure domain (the S3 object store follows in PH-2):
//!
//! * `base/<id>/` — a PostgreSQL base backup (`pg_basebackup`, tar format, compressed):
//!   `base.tar.gz.enc`, `pg_wal.tar.gz.enc`, `backup_manifest.enc` and `manifest.json.enc`
//!   (`{id, started_at, finished_at, start_wal, end_wal, timeline, heads}`).
//! * `wal/<segment>.enc` — the continuous WAL archive, written by PostgreSQL's
//!   `archive_command = 'ubos-server backup wal-push %p %f --config …'`.
//! * `points/<time>-<name>.json.enc` — named restore points (`pg_create_restore_point`)
//!   with the branch heads and state roots of every VAE at that point: the reference a
//!   restore is verified against (SCN-017).
//!
//! **Encryption (DSN-NOD-623).** Every file is sealed with XChaCha20-Poly1305 under the
//! backup key (`backup.key`, 32 bytes, separate from the root and tenant KEKs) in chunks
//! of 1 MiB: header `UBOSBK01 · key id (8) · file id (16)`, then per chunk `last (1) ·
//! length (4) · ciphertext`; the nonce is the file id and the chunk counter, the
//! associated data the header, counter and `last` flag, so a reordered, truncated or
//! modified file fails to open. Without the key a copy is unreadable.
//!
//! **Restore.** [`prepare_restore`] picks the newest base backup before the target,
//! extracts it into a new data directory, stages the archived WAL it needs and writes the
//! recovery settings for a named point or a time; a gap in the WAL chain stops it at the
//! last consistent point (SCN-017 A1). [`verify`] then compares the restored heads with
//! the point's and runs the DVM root verifier and the index rebuild check over the last
//! commits of every VAE (DSN-NOD-620 acceptance 1).
//!
//! **Retention.** [`prune`] keeps every base backup younger than `backup.retention_days`
//! and the newest older one, so the whole window stays restorable, and the WAL and points
//! from the oldest kept base backup on.

use crate::config::{Config, resolve};
use crate::db::{Db, Param};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use ubos_dvm::crypto;
use ubos_dvm::types::BpaError;

const MAGIC: &[u8; 8] = b"UBOSBK01";
const CHUNK: usize = 1 << 20;

fn fail(m: impl Into<String>) -> BpaError {
    BpaError::new("NOD.STORAGE_UNAVAILABLE", m.into())
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> BpaError + '_ {
    move |e| fail(format!("{}: {e}", path.display()))
}

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
}

fn stamp(t: chrono::DateTime<chrono::Utc>) -> String {
    t.format("%Y%m%dT%H%M%S%.3fZ").to_string()
}

/// The backup settings of a configuration: the directory and the key.
#[derive(Clone)]
pub struct Target {
    /// `backup.dir`.
    pub dir: PathBuf,
    key: [u8; 32],
    /// `backup.retention_days`.
    pub retention_days: i64,
}

impl Target {
    /// The backup target of a configuration; `None` when `backup.dir` is not set. The key
    /// is 64 hexadecimal digits behind a reference and must differ from the root KEK.
    pub fn of(cfg: &Config) -> Result<Option<Target>, BpaError> {
        let Some(dir) = cfg.text("backup.dir") else {
            return Ok(None);
        };
        let r = cfg
            .text("backup.key")
            .ok_or_else(|| fail("backup.key is required with backup.dir"))?;
        let k = resolve(r)?;
        let key = crypto::unhex32(&json!(k.trim()), "backup.key").map_err(|_| {
            BpaError::new("IAM.KEY_UNAVAILABLE", "backup.key is 64 hexadecimal digits")
        })?;
        if let Some(root) = cfg.text("keys.root_kek").and_then(|r| resolve(r).ok())
            && root.trim() == k.trim()
        {
            return Err(BpaError::new(
                "IAM.KEY_UNAVAILABLE",
                "backup.key must differ from the root KEK (DSN-NOD-623)",
            ));
        }
        Ok(Some(Target {
            dir: PathBuf::from(dir),
            key,
            retention_days: cfg.int("backup.retention_days").max(1),
        }))
    }

    /// The configured target, or an error naming the missing setting.
    pub fn required(cfg: &Config) -> Result<Target, BpaError> {
        Target::of(cfg)?.ok_or_else(|| fail("backup.dir and backup.key are not configured"))
    }

    fn key_id(&self) -> [u8; 8] {
        let h = crypto::hmac_sha256(&self.key, b"ubos.backup.key-id.v1");
        let mut id = [0u8; 8];
        id.copy_from_slice(&h[..8]);
        id
    }

    fn sub(&self, name: &str) -> Result<PathBuf, BpaError> {
        let p = self.dir.join(name);
        std::fs::create_dir_all(&p).map_err(io(&p))?;
        Ok(p)
    }

    // ---- the sealed file format ----

    /// Seal a stream into `dst` (written to a temporary name, then renamed).
    pub fn seal_to(&self, mut src: impl Read, dst: &Path) -> Result<u64, BpaError> {
        let tmp = dst.with_extension("partial");
        let mut out = std::io::BufWriter::new(std::fs::File::create(&tmp).map_err(io(&tmp))?);
        let file_id: [u8; 16] = crypto::random_bytes()?;
        let mut header = Vec::with_capacity(32);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&self.key_id());
        header.extend_from_slice(&file_id);
        out.write_all(&header).map_err(io(&tmp))?;
        let mut buf = vec![0u8; CHUNK];
        let mut next = vec![0u8; CHUNK];
        let mut n = fill(&mut src, &mut buf).map_err(io(dst))?;
        let (mut counter, mut total) = (0u64, 0u64);
        loop {
            let m = if n == CHUNK {
                fill(&mut src, &mut next).map_err(io(dst))?
            } else {
                0
            };
            let last = n < CHUNK || m == 0;
            let ct = crypto::seal(
                &self.key,
                &nonce(&file_id, counter),
                &aad(&header, counter, last),
                &buf[..n],
            )?;
            out.write_all(&[last as u8]).map_err(io(&tmp))?;
            out.write_all(&(ct.len() as u32).to_be_bytes())
                .map_err(io(&tmp))?;
            out.write_all(&ct).map_err(io(&tmp))?;
            total += n as u64;
            if last {
                break;
            }
            std::mem::swap(&mut buf, &mut next);
            n = m;
            counter += 1;
        }
        let f = out.into_inner().map_err(|e| fail(e.to_string()))?;
        f.sync_all().map_err(io(&tmp))?;
        std::fs::rename(&tmp, dst).map_err(io(dst))?;
        Ok(total)
    }

    /// Open a sealed file into a writer; a wrong key, a modified or truncated file fails.
    pub fn open_to(&self, src: &Path, mut dst: impl Write) -> Result<u64, BpaError> {
        let mut f = std::io::BufReader::new(std::fs::File::open(src).map_err(io(src))?);
        let mut header = [0u8; 32];
        f.read_exact(&mut header).map_err(io(src))?;
        if &header[..8] != MAGIC {
            return Err(crypto::integrity("not a UBOS backup file"));
        }
        if header[8..16] != self.key_id() {
            return Err(BpaError::new(
                "IAM.KEY_UNAVAILABLE",
                format!("{}: sealed with another backup key", src.display()),
            ));
        }
        let mut file_id = [0u8; 16];
        file_id.copy_from_slice(&header[16..32]);
        let (mut counter, mut total) = (0u64, 0u64);
        loop {
            let mut head = [0u8; 5];
            if f.read_exact(&mut head).is_err() {
                return Err(crypto::integrity("the file is truncated"));
            }
            let last = head[0] == 1;
            let len = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
            if len > CHUNK + 64 {
                return Err(crypto::integrity("a chunk is too long"));
            }
            let mut ct = vec![0u8; len];
            f.read_exact(&mut ct)
                .map_err(|_| crypto::integrity("the file is truncated"))?;
            let plain = crypto::open(
                &self.key,
                &nonce(&file_id, counter),
                &aad(&header, counter, last),
                &ct,
            )?;
            dst.write_all(&plain).map_err(io(src))?;
            total += plain.len() as u64;
            if last {
                let mut rest = [0u8; 1];
                if f.read(&mut rest).map_err(io(src))? != 0 {
                    return Err(crypto::integrity("data after the last chunk"));
                }
                return Ok(total);
            }
            counter += 1;
        }
    }

    fn seal_json(&self, v: &Value, dst: &Path) -> Result<(), BpaError> {
        let text = serde_json::to_vec_pretty(v).unwrap_or_default();
        self.seal_to(&text[..], dst).map(|_| ())
    }

    fn open_json(&self, src: &Path) -> Result<Value, BpaError> {
        let mut out = Vec::new();
        self.open_to(src, &mut out)?;
        serde_json::from_slice(&out).map_err(|e| fail(format!("{}: {e}", src.display())))
    }
}

fn fill(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

fn nonce(file_id: &[u8; 16], counter: u64) -> [u8; crypto::NONCE_LEN] {
    let mut n = [0u8; crypto::NONCE_LEN];
    n[..16].copy_from_slice(file_id);
    n[16..].copy_from_slice(&counter.to_be_bytes());
    n
}

fn aad(header: &[u8], counter: u64, last: bool) -> Vec<u8> {
    let mut a = header.to_vec();
    a.extend_from_slice(&counter.to_be_bytes());
    a.push(last as u8);
    a
}

// ---- heads and restore points ----

/// The heads of every branch of every VAE: `[{tenant, vae, branch, cseq, head, root}]`
/// (the owner connection; content IDs and state roots in hexadecimal).
pub fn heads(db: &Db) -> Result<Vec<Value>, BpaError> {
    db.query(
        "SELECT t.slug AS tenant, v.authority AS vae, b.name AS branch, b.head_cseq AS cseq, \
         encode(b.head_commit, 'hex') AS head, encode(c.state_root, 'hex') AS root \
         FROM branch_ref b JOIN vae v ON v.tenant_id = b.tenant_id AND v.vae_id = b.vae_id \
         JOIN tenant t ON t.tenant_id = b.tenant_id \
         LEFT JOIN commit c ON c.tenant_id = b.tenant_id AND c.vae_id = b.vae_id AND c.commit_id = b.head_commit \
         ORDER BY t.slug, v.authority, b.name",
        &[],
    )
}

/// Create a named restore point and record the heads at it (`points/`): the heads are
/// read before and after the point and kept only when they agree, so they are exactly the
/// heads of a restore to that point. A point a commit raced is left unrecorded and the
/// next attempt takes a suffixed name (a recovery stops at the first point of a name).
pub fn restore_point(db: &Db, target: &Target, name: &str) -> Result<Value, BpaError> {
    let ok = !name.is_empty()
        && name.len() <= 60
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        return Err(BpaError::new(
            "MODEL.INVALID_VALUE",
            "a restore point name is 1…60 of [A-Za-z0-9_-]",
        ));
    }
    for attempt in 0..10 {
        let used = if attempt == 0 {
            name.to_string()
        } else {
            format!("{name}-{attempt}")
        };
        let before = heads(db)?;
        let p = db.query(
            "WITH p AS (SELECT pg_create_restore_point($1) AS lsn) \
             SELECT lsn::text AS lsn, pg_walfile_name(lsn) AS wal_file, \
             to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS at FROM p",
            &[Param::Text(&used)],
        )?;
        let after = heads(db)?;
        if before != after {
            continue;
        }
        let p = p.into_iter().next().unwrap_or(Value::Null);
        let rec = json!({"name": used, "at": p["at"], "lsn": p["lsn"], "wal_file": p["wal_file"], "heads": after});
        let file = target
            .sub("points")?
            .join(format!("{}-{used}.json.enc", stamp(now())));
        target.seal_json(&rec, &file)?;
        return Ok(
            json!({"name": used, "at": rec["at"], "lsn": rec["lsn"], "wal_file": rec["wal_file"],
            "vaes": rec["heads"].as_array().map_or(0, Vec::len)}),
        );
    }
    Err(fail(
        "the heads kept changing around the restore point; try again",
    ))
}

/// The recorded restore points, oldest first (`{name, at, lsn, wal_file, heads}`).
pub fn points(target: &Target) -> Result<Vec<Value>, BpaError> {
    let mut out = Vec::new();
    for f in sorted(&target.sub("points")?)? {
        if f.extension().is_some_and(|e| e == "enc") {
            let mut v = target.open_json(&f)?;
            v["file"] = json!(f.file_name().map(|n| n.to_string_lossy().to_string()));
            out.push(v);
        }
    }
    Ok(out)
}

fn sorted(dir: &Path) -> Result<Vec<PathBuf>, BpaError> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(io(dir))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    v.sort();
    Ok(v)
}

// ---- base backups and the WAL archive ----

/// The WAL segment name of a position (`0/2000028`) on a timeline, 16 MiB segments.
pub fn segment(timeline: u64, lsn: &str) -> Option<String> {
    let (hi, lo) = lsn.split_once('/')?;
    let (hi, lo) = (
        u64::from_str_radix(hi, 16).ok()?,
        u64::from_str_radix(lo, 16).ok()?,
    );
    Some(format!("{timeline:08X}{hi:08X}{:08X}", lo >> 24))
}

/// Take a base backup with `pg_basebackup` (tar format, compressed, the WAL it needs
/// streamed alongside), seal it into `base/<id>/` and prune by retention.
pub fn base_backup(db: &Db, url: &str, target: &Target) -> Result<Value, BpaError> {
    let started = now();
    let id = stamp(started);
    let work = target.sub(".work")?.join(&id);
    std::fs::create_dir_all(&work).map_err(io(&work))?;
    let out = std::process::Command::new("pg_basebackup")
        .args(["-d", url, "-D"])
        .arg(&work)
        .args(["-Ft", "-z", "-X", "stream", "-c", "fast", "-l"])
        .arg(format!("ubos {id}"))
        .output()
        .map_err(|e| fail(format!("pg_basebackup: {e}")))?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&work);
        return Err(fail(format!(
            "pg_basebackup failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let manifest: Value = std::fs::read(work.join("backup_manifest"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    let range = &manifest["WAL-Ranges"][0];
    let tl = range["Timeline"].as_u64().unwrap_or(1);
    let start_wal = range["Start-LSN"].as_str().and_then(|l| segment(tl, l));
    let end_wal = range["End-LSN"].as_str().and_then(|l| segment(tl, l));
    let dest = target.sub("base")?.join(&id);
    std::fs::create_dir_all(&dest).map_err(io(&dest))?;
    let mut bytes = 0;
    for f in ["base.tar.gz", "pg_wal.tar.gz", "backup_manifest"] {
        let src = work.join(f);
        if src.exists() {
            let r = std::fs::File::open(&src).map_err(io(&src))?;
            bytes += target.seal_to(r, &dest.join(format!("{f}.enc")))?;
        }
    }
    let _ = std::fs::remove_dir_all(&work);
    let heads = heads(db)?;
    let rec = json!({"id": id, "started_at": started.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "finished_at": now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "timeline": tl, "start_wal": start_wal, "end_wal": end_wal, "bytes": bytes, "heads": heads,
        "ubos_server": env!("CARGO_PKG_VERSION")});
    target.seal_json(&rec, &dest.join("manifest.json.enc"))?;
    let pruned = prune(target)?;
    Ok(
        json!({"id": id, "start_wal": start_wal, "end_wal": end_wal, "bytes": bytes,
        "vaes": heads.len(), "pruned": pruned}),
    )
}

/// The base backups, oldest first (their manifests).
pub fn bases(target: &Target) -> Result<Vec<Value>, BpaError> {
    let mut out = Vec::new();
    for d in sorted(&target.sub("base")?)? {
        let m = d.join("manifest.json.enc");
        if m.exists() {
            let mut v = target.open_json(&m)?;
            if let Some(o) = v.as_object_mut() {
                o.remove("heads");
            }
            out.push(v);
        }
    }
    Ok(out)
}

/// Archive one WAL file (`archive_command`): sealed into `wal/`. A file archived already
/// with the same content succeeds again; another content under the name is refused.
pub fn wal_push(target: &Target, path: &Path, name: &str) -> Result<(), BpaError> {
    if name.is_empty() || name.contains('/') {
        return Err(fail("a WAL file name"));
    }
    let dst = target.sub("wal")?.join(format!("{name}.enc"));
    if dst.exists() {
        let mut old = Vec::new();
        target.open_to(&dst, &mut old)?;
        let new = std::fs::read(path).map_err(io(path))?;
        return if old == new {
            Ok(())
        } else {
            Err(fail(format!(
                "{name} is archived already with other content"
            )))
        };
    }
    let r = std::fs::File::open(path).map_err(io(path))?;
    target.seal_to(r, &dst).map(|_| ())
}

/// Fetch one archived WAL file (`restore_command`).
pub fn wal_fetch(target: &Target, name: &str, path: &Path) -> Result<(), BpaError> {
    let src = target.dir.join("wal").join(format!("{name}.enc"));
    if !src.exists() {
        return Err(BpaError::new(
            "IAM.NOT_FOUND",
            format!("{name} is not archived"),
        ));
    }
    let tmp = path.with_extension("ubos-partial");
    let f = std::fs::File::create(&tmp).map_err(io(&tmp))?;
    target.open_to(&src, f)?;
    std::fs::rename(&tmp, path).map_err(io(path))
}

/// The archived WAL segments (24 hexadecimal digits), sorted.
fn segments(target: &Target) -> Result<Vec<String>, BpaError> {
    Ok(sorted(&target.sub("wal")?)?
        .iter()
        .filter_map(|p| {
            p.file_name()?
                .to_str()?
                .strip_suffix(".enc")
                .map(str::to_string)
        })
        .filter(|n| n.len() == 24 && n.chars().all(|c| c.is_ascii_hexdigit()))
        .collect())
}

/// The segment after another (16 MiB segments: 256 per log file).
fn next_segment(s: &str) -> Option<String> {
    let tl = &s[..8];
    let log = u64::from_str_radix(&s[8..16], 16).ok()?;
    let seg = u64::from_str_radix(&s[16..24], 16).ok()?;
    let (log, seg) = if seg >= 0xFF {
        (log + 1, 0)
    } else {
        (log, seg + 1)
    };
    Some(format!("{tl}{log:08X}{seg:08X}"))
}

/// The last segment of the unbroken chain from `start` (`None` when `start` is missing).
pub fn chain_end(target: &Target, start: &str) -> Result<Option<String>, BpaError> {
    let have: std::collections::BTreeSet<String> = segments(target)?.into_iter().collect();
    if !have.contains(start) {
        return Ok(None);
    }
    let mut cur = start.to_string();
    while let Some(n) = next_segment(&cur) {
        if !have.contains(&n) {
            break;
        }
        cur = n;
    }
    Ok(Some(cur))
}

/// Delete what the retention no longer needs: base backups older than the retention
/// except the newest of them, then the WAL and points before the oldest kept backup.
pub fn prune(target: &Target) -> Result<Value, BpaError> {
    let cutoff = now() - chrono::Duration::days(target.retention_days);
    let all = bases(target)?;
    let old: Vec<&Value> = all
        .iter()
        .filter(|b| {
            b["finished_at"]
                .as_str()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .is_some_and(|t| t < cutoff)
        })
        .collect();
    let mut removed = Vec::new();
    // the newest base backup older than the cutoff still covers the window's start
    for b in old.iter().take(old.len().saturating_sub(1)) {
        let id = b["id"].as_str().unwrap_or_default();
        if !id.is_empty() {
            let d = target.dir.join("base").join(id);
            std::fs::remove_dir_all(&d).map_err(io(&d))?;
            removed.push(id.to_string());
        }
    }
    let kept = bases(target)?;
    let mut wal = 0;
    let mut pts = 0;
    if let Some(first) = kept.first() {
        if let Some(start) = first["start_wal"].as_str() {
            for s in segments(target)? {
                if s[8..] < start[8..] {
                    let f = target.dir.join("wal").join(format!("{s}.enc"));
                    std::fs::remove_file(&f).map_err(io(&f))?;
                    wal += 1;
                }
            }
        }
        let from = first["id"].as_str().unwrap_or_default().to_string();
        for f in sorted(&target.sub("points")?)? {
            let n = f
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if n < from {
                std::fs::remove_file(&f).map_err(io(&f))?;
                pts += 1;
            }
        }
    }
    Ok(json!({"base_backups": removed, "wal_segments": wal, "points": pts}))
}

// ---- restore ----

/// What a restore stops at.
#[derive(Debug, Clone)]
pub enum Stop {
    /// A recorded restore point, by name.
    Point(String),
    /// A time (RFC 3339).
    Time(String),
    /// The end of the archive.
    Latest,
}

/// Prepare a restore into `into` (a new, empty directory): the newest base backup before
/// the target is extracted, the archived WAL from its start is staged in `<into>.wal`
/// and the recovery settings are written (`recovery.signal`, `postgresql.auto.conf`).
/// The restored server is started by the operator (`pg_ctl -D <into> start`) and then
/// checked with [`verify`]. A gap in the WAL chain before the target refuses with the
/// last consistent point (SCN-017 A1).
pub fn prepare_restore(
    target: &Target,
    stop: &Stop,
    into: &Path,
    port: u16,
    hba_from: Option<&Path>,
) -> Result<Value, BpaError> {
    if into.exists() && std::fs::read_dir(into).map_err(io(into))?.next().is_some() {
        return Err(fail(format!("{} is not empty", into.display())));
    }
    let pts = points(target)?;
    let point = match stop {
        Stop::Point(n) => Some(
            pts.iter()
                .find(|p| p["name"] == json!(n))
                .cloned()
                .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", format!("no restore point {n}")))?,
        ),
        _ => None,
    };
    let at = match (stop, &point) {
        (_, Some(p)) => p["at"].as_str().map(str::to_string),
        (Stop::Time(t), _) => Some(t.clone()),
        _ => None,
    };
    let parse = |t: &str| chrono::DateTime::parse_from_rfc3339(t).ok();
    let all = bases(target)?;
    let base = all
        .iter()
        .rev()
        .find(|b| match (&at, b["finished_at"].as_str()) {
            (Some(a), Some(f)) => parse(f) <= parse(a),
            (None, _) => true,
            _ => false,
        })
        .cloned()
        .ok_or_else(|| fail("no base backup before the target"))?;
    let start = base["start_wal"].as_str().unwrap_or_default().to_string();
    let end = chain_end(target, &start)?.unwrap_or_else(|| start.clone());
    // the last consistent point: the newest restore point inside the unbroken chain
    let consistent = pts
        .iter()
        .rev()
        .find(|p| {
            p["wal_file"]
                .as_str()
                .is_some_and(|w| w[8..] <= end[8..] && w[8..] >= start[8..])
        })
        .map(|p| json!({"name": p["name"], "at": p["at"]}));
    let have_after_gap = segments(target)?.iter().any(|s| s[8..] > end[8..]);
    if let Some(p) = &point
        && p["wal_file"].as_str().is_some_and(|w| w[8..] > end[8..])
    {
        return Err(BpaError::with_details(
            "NOD.STORAGE_UNAVAILABLE",
            json!({"reason": "wal_chain_broken", "chain_end": end, "last_consistent": consistent,
                "message": "the WAL archive has a gap before the restore point; restore to the last consistent point"}),
        ));
    }
    if have_after_gap && matches!(stop, Stop::Latest | Stop::Time(_)) {
        let last_time = std::fs::metadata(target.dir.join("wal").join(format!("{end}.enc")))
            .and_then(|m| m.modified())
            .ok()
            .map(chrono::DateTime::<chrono::Utc>::from);
        let beyond = match &at {
            Some(a) => match (parse(a), last_time) {
                (Some(a), Some(l)) => a > l,
                _ => true,
            },
            None => true,
        };
        if beyond {
            return Err(BpaError::with_details(
                "NOD.STORAGE_UNAVAILABLE",
                json!({"reason": "wal_chain_broken", "chain_end": end, "last_consistent": consistent,
                    "message": "the WAL archive has a gap before the target; restore to the last consistent point"}),
            ));
        }
    }
    std::fs::create_dir_all(into).map_err(io(into))?;
    let id = base["id"].as_str().unwrap_or_default();
    let bdir = target.dir.join("base").join(id);
    untar(target, &bdir.join("base.tar.gz.enc"), into)?;
    let pg_wal = into.join("pg_wal");
    std::fs::create_dir_all(&pg_wal).map_err(io(&pg_wal))?;
    if bdir.join("pg_wal.tar.gz.enc").exists() {
        untar(target, &bdir.join("pg_wal.tar.gz.enc"), &pg_wal)?;
    }
    // the WAL of the chain, and the timeline histories
    let stage = PathBuf::from(format!("{}.wal", into.display()));
    std::fs::create_dir_all(&stage).map_err(io(&stage))?;
    let mut staged = 0;
    for f in sorted(&target.dir.join("wal"))? {
        let Some(n) = f
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".enc"))
        else {
            continue;
        };
        let seg = n.len() == 24;
        if (seg && n[8..] >= start[8..] && n[8..] <= end[8..]) || n.ends_with(".history") {
            wal_fetch(target, n, &stage.join(n))?;
            staged += 1;
        }
    }
    let mut conf = format!(
        "\n# restore prepared by ubos-server (DSN-NOD-620)\nrestore_command = 'cp {}/%f \"%p\"'\nrecovery_target_action = 'promote'\narchive_mode = 'off'\nport = {port}\n",
        stage.display()
    );
    match (stop, &point) {
        (_, Some(p)) => {
            conf += &format!(
                "recovery_target_name = '{}'\n",
                p["name"].as_str().unwrap_or_default()
            )
        }
        (Stop::Time(t), _) => {
            // PostgreSQL's form of the time, in UTC
            let t = parse(t)
                .map(|t| {
                    t.with_timezone(&chrono::Utc)
                        .format("%Y-%m-%d %H:%M:%S%.6f+00")
                        .to_string()
                })
                .ok_or_else(|| {
                    BpaError::new("MODEL.INVALID_VALUE", "--time is an RFC 3339 time")
                })?;
            conf += &format!("recovery_target_time = '{t}'\n")
        }
        _ => {}
    }
    // a packaged cluster keeps its configuration outside the data directory
    let conf_file = into.join("postgresql.conf");
    if !conf_file.exists() {
        std::fs::write(
            &conf_file,
            "# written by ubos-server restore; the restored instance listens locally only\nlisten_addresses = 'localhost'\n",
        )
        .map_err(io(&conf_file))?;
    }
    let hba = into.join("pg_hba.conf");
    match hba_from {
        Some(src) => {
            std::fs::copy(src, &hba).map_err(io(src))?;
        }
        None if !hba.exists() => {
            std::fs::write(
                &hba,
                "# written by ubos-server restore\nlocal all all peer\nhost all all 127.0.0.1/32 scram-sha-256\nhost all all ::1/128 scram-sha-256\n",
            )
            .map_err(io(&hba))?;
        }
        None => {}
    }
    let auto = into.join("postgresql.auto.conf");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&auto)
        .map_err(io(&auto))?;
    f.write_all(conf.as_bytes()).map_err(io(&auto))?;
    std::fs::write(into.join("recovery.signal"), b"").map_err(io(into))?;
    for d in [into, stage.as_path()] {
        set_mode(d, 0o700);
    }
    Ok(
        json!({"base_backup": id, "start_wal": start, "chain_end": end, "wal_staged": staged,
        "target": match stop { Stop::Point(n) => json!({"point": n}), Stop::Time(t) => json!({"time": t}), Stop::Latest => json!("latest") },
        "expected_heads": point.as_ref().map(|p| p["heads"].clone()),
        "data_directory": into.display().to_string(), "port": port,
        "start": format!("pg_ctl -D {} -w start", into.display())}),
    )
}

#[cfg(unix)]
fn set_mode(p: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
fn set_mode(_p: &Path, _mode: u32) {}

/// Open a sealed `.tar.gz` straight into `tar -xz` (no plaintext copy on disk).
fn untar(target: &Target, src: &Path, into: &Path) -> Result<(), BpaError> {
    let mut child = std::process::Command::new("tar")
        .args(["-xzf", "-", "-C"])
        .arg(into)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| fail(format!("tar: {e}")))?;
    let r = {
        let stdin = child.stdin.take().ok_or_else(|| fail("tar: no input"))?;
        target.open_to(src, stdin)
    };
    let st = child.wait().map_err(|e| fail(format!("tar: {e}")))?;
    r?;
    if !st.success() {
        return Err(fail(format!("tar could not extract {}", src.display())));
    }
    Ok(())
}

// ---- verification ----

/// Verify a node's database (a restore, or after an upgrade): the heads against the
/// expected ones (when given), then per VAE the DVM root verifier and the index rebuild
/// check over the last `last` commits of `main` (DSN-NOD-620 acceptance 1, SCN-017).
pub fn verify(
    url: &str,
    root: [u8; 32],
    expected: Option<&Value>,
    last: u64,
) -> Result<Value, BpaError> {
    let node = crate::Node::connect_with_root(url, root)?;
    let current = heads(node.owner_db())?;
    let heads_equal = expected.map(|e| e.as_array().is_some_and(|a| *a == current));
    let mut mismatches = Vec::new();
    if let Some(e) = expected.and_then(Value::as_array) {
        for x in e {
            let found = current.iter().find(|c| {
                c["tenant"] == x["tenant"] && c["vae"] == x["vae"] && c["branch"] == x["branch"]
            });
            if found != Some(x) {
                mismatches.push(json!({"expected": x, "found": found}));
            }
        }
    }
    let mut checks = Vec::new();
    let mut ok = heads_equal != Some(false);
    for h in current.iter().filter(|h| h["branch"] == "main") {
        let (slug, auth) = (
            h["tenant"].as_str().unwrap_or_default(),
            h["vae"].as_str().unwrap_or_default(),
        );
        let r = (|| -> Result<Value, BpaError> {
            let t = node
                .tenant(slug)?
                .ok_or_else(|| BpaError::new("IAM.NOT_FOUND", "unknown tenant"))?;
            let mut vae = node.open_vae(&t, auth)?;
            let head = h["cseq"].as_u64().unwrap_or(0);
            let from = head.saturating_sub(last.saturating_sub(1)).max(1);
            let roots = ubos_dvm::verify::verify_roots(vae.engine_mut(), "main", from, head)?;
            let indexes = node.verify_indexes(&t, auth, "main", from, head)?;
            Ok(
                json!({"commits": roots.checked, "root_findings": roots.findings.len(),
                "index_findings": indexes["findings"], "from": from, "to": head}),
            )
        })();
        match r {
            Ok(v) => {
                ok &= v["root_findings"] == json!(0) && v["index_findings"] == json!(0);
                checks.push(json!({"tenant": slug, "vae": auth, "ok": v["root_findings"] == json!(0) && v["index_findings"] == json!(0), "detail": v}));
            }
            Err(e) => {
                ok = false;
                checks.push(
                    json!({"tenant": slug, "vae": auth, "ok": false, "error": e.to_string()}),
                );
            }
        }
    }
    Ok(
        json!({"ok": ok, "heads_equal": heads_equal, "mismatches": mismatches, "heads": current,
        "checks": checks, "verified_at": now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)}),
    )
}

/// Write a report under `backup.dir/reports/` (plain JSON: heads and results, no data).
pub fn write_report(target: &Target, kind: &str, report: &Value) -> Result<PathBuf, BpaError> {
    let p = target
        .sub("reports")?
        .join(format!("{kind}-{}.json", stamp(now())));
    std::fs::write(&p, serde_json::to_vec_pretty(report).unwrap_or_default()).map_err(io(&p))?;
    Ok(p)
}

/// The backup state for the self-checks: the newest base backup's age in hours and the
/// newest archived WAL's age in minutes.
pub fn status(target: &Target) -> Value {
    let age = |p: &Path| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|d| d.as_secs())
    };
    let newest = |d: &Path| -> Option<u64> { sorted(d).ok()?.iter().filter_map(|p| age(p)).min() };
    json!({"base_age_secs": newest(&target.dir.join("base")), "wal_age_secs": newest(&target.dir.join("wal"))})
}
