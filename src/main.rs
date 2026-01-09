use distributed_cluster::membership::service::MembershipService;
use std::net::SocketAddr;

use tracing_subscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} --bind <addr:port> [--seed <addr:port>]", args[0]);
        eprintln!("Example: {} -- bind 127.0.0.1:5000", args[0]);
        eprintln!(
            "Example: {} --bind 127.0.0.1:5001 --seed 127.0.0.1:5000",
            args[0]
        );

        std::process::exit(1);
    }

    let mut bind_addr: Option<SocketAddr> = None;
    let mut seed_nodes: Vec<SocketAddr> = vec![];

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--bind" => {
                bind_addr = Some(args[i + 1].parse()?);
                i += 2;
            }
            "--seed" => {
                seed_nodes.push(args[i + 1].parse()?);
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    let bind_addr = bind_addr.expect("--bind is required");

    tracing::info!("Starting node on {}", bind_addr);
    if !seed_nodes.is_empty() {
        tracing::info!("Seed nodes: {:?}", seed_nodes);
    } else {
        tracing::info!("Starting as seed node (founder)");
    }

    let service = MembershipService::new(bind_addr, seed_nodes).await?;

    tracing::info!("Node ID: {:?}", service.local_node.id);
    tracing::info!("Initial cluster size: {}", service.members.len());

    let service_clone = service.clone();
    tokio::spawn(async move {
        service_clone.start().await;
    });

    let stats_service = service.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

        loop {
            interval.tick().await;
            let alive = stats_service.get_alive_members();
            tracing::info!("Cluster stats: {} alive nodes", alive.len());
            for node in alive {
                tracing::info!(
                    "  - {:?} at {} (inc={})",
                    node.id,
                    node.addr,
                    node.incarnation
                );
            }
        }
    });

    tracing::info!("Press Ctrl+C to shutdown");
    tokio::signal::ctrl_c().await?;

    tracing::info!("Shutting down...");
    Ok(())
}
