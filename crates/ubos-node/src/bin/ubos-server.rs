//! `ubos-server` — the single-node Server over UBTP (PH-1 step 14b, DEC-075).
//!
//! ```text
//! ubos-server [--listen 127.0.0.1:8080] [--database URL | --memory [--genesis FILE]]
//!             [--tenant SLUG] [--vae AUTHORITY] [--preset dev|server]
//! ```
//!
//! With `--database` the Server serves the VAEs of the node's tenancy registry on
//! PostgreSQL; with `--memory` one VAE over the memory store (development and suites),
//! its genesis classes from `--genesis` (a JSON list of class declarations).

use serde_json::Value;
use ubos_node::server::{NodeVaes, Preset, Server, SingleVae, VaeHandle, memory_vae};

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
    let server = if flag("--memory") || opt("--database").is_none() {
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
        let handle = VaeHandle::spawn(move || memory_vae(classes.as_ref())).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        });
        Server::new(Box::new(SingleVae(handle)), preset, &tenant, &authority)
    } else {
        let url = opt("--database").unwrap_or_default();
        let node = ubos_node::Node::connect(&url).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        });
        Server::new(
            Box::new(NodeVaes(std::sync::Arc::new(node))),
            preset,
            &tenant,
            &authority,
        )
    };
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
