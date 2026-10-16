//! `ubos-server` — the single-node Server over UBTP (PH-1 step 14b, DEC-075).
//!
//! ```text
//! ubos-server [--listen 127.0.0.1:8080] [--database URL | --memory [--genesis FILE]]
//!             [--tenant SLUG] [--vae AUTHORITY] [--preset dev|server]
//!             [--admin USERNAME:PASSWORD]
//!
//! `--genesis FILE` declares its classes in the memory VAE, or with `--database` in the
//! default VAE when the first start creates it.
//! ```
//!
//! With `--database` the Server serves the VAEs of the node's tenancy registry on
//! PostgreSQL; with `--memory` one VAE over the memory store (development and suites),
//! its genesis classes from `--genesis` (a JSON list of class declarations).

use serde_json::Value;
use std::sync::Arc;
use ubos_node::server::{NodeVaes, Preset, Server, VaeHandle, memory_vae_with};
use ubos_node::services::{MemEventLog, MemJobQueue};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let flag = |k: &str| args.iter().any(|a| a == k);
    let listen: std::net::SocketAddr = opt("--listen")
        .unwrap_or_else(|| "127.0.0.1:8080".into())
        .parse()
        .unwrap_or_else(|e| {
            eprintln!("--listen: {e}");
            std::process::exit(2)
        });
    let preset = match opt("--preset").as_deref() {
        Some("server") => Preset::Server,
        _ => Preset::Dev,
    };
    let tenant = opt("--tenant").unwrap_or_else(|| "acme".into());
    let authority = opt("--vae").unwrap_or_else(|| "acme".into());
    let classes: Option<Value> = opt("--genesis").map(|p| {
        serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| {
            eprintln!("{p}: {e}");
            std::process::exit(2)
        }))
        .unwrap_or_else(|e| {
            eprintln!("{p}: {e}");
            std::process::exit(2)
        })
    });
    let server = if flag("--memory") || opt("--database").is_none() {
        let events = Arc::new(MemEventLog::default());
        let jobs = Arc::new(MemJobQueue::default());
        let (ev, jb) = (events.clone(), jobs.clone());
        let handle =
            VaeHandle::spawn(move || memory_vae_with(classes.as_ref(), Some(ev), Some(jb)))
                .unwrap_or_else(|e| {
                    eprintln!("{e}");
                    std::process::exit(1)
                });
        let identity =
            ubos_node::identity::Identity::memory(&tenant, &authority).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1)
            });
        Server::single_with(
            handle,
            Arc::new(identity),
            Some(events),
            Some(jobs),
            preset,
            &tenant,
            &authority,
        )
    } else {
        let url = opt("--database").unwrap_or_default();
        let node = ubos_node::Node::connect(&url).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        });
        // the default tenant and its VAE exist (created on the first start)
        let created = (|| -> Result<bool, ubos_dvm::types::BpaError> {
            let t = match node.tenant(&tenant)? {
                Some(_) => return Ok(false),
                None => node.create_tenant(&tenant)?,
            };
            node.create_vae(&t, &authority)?;
            // the genesis classes of a VAE created on the first start
            if let Some(c) = &classes {
                node.open_vae(&t, &authority)?
                    .call("load_genesis", &serde_json::json!({"classes": c}))?;
            }
            Ok(true)
        })()
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        });
        if created {
            eprintln!("tenant {tenant} with VAE {authority} created");
        }
        Server::new(
            Box::new(NodeVaes(Arc::new(node))),
            preset,
            &tenant,
            &authority,
        )
    };
    // the first administrator of the default tenant (`--admin` or `UBOS_ADMIN`), when the
    // username is free
    let admin = opt("--admin").or_else(|| std::env::var("UBOS_ADMIN").ok());
    if let Some(a) = admin {
        let Some((user, password)) = a.split_once(':') else {
            eprintln!("--admin takes USERNAME:PASSWORD");
            std::process::exit(2)
        };
        match server
            .identity(&tenant)
            .and_then(|id| id.bootstrap_admin(user, password))
        {
            Ok(Some(did)) => eprintln!("administrator {user} created as {did}"),
            Ok(None) => eprintln!("administrator {user} exists"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1)
            }
        }
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    eprintln!("ubos-server listening on {listen} ({preset:?} preset)");
    if let Err(e) = rt.block_on(server.serve(listen)) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
