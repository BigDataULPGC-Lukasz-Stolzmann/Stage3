use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::{
    Router,
    extract::Extension,
    routing::{get, post},
};
use distributed_cluster::executor::executor::TaskExecutor;
use distributed_cluster::executor::handlers::{
    handle_get_task_status, handle_internal_submit_task, handle_submit_task,
};
use distributed_cluster::executor::queue::DistributedQueue;
use distributed_cluster::executor::registry::TaskHandlerRegistry;
use distributed_cluster::membership::service::MembershipService;
use distributed_cluster::storage::handlers::*;
use distributed_cluster::storage::memory::DistributedMap;
use distributed_cluster::storage::partitioner::PartitionManager;
use distributed_cluster::storage::protocol::*;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        // .with_max_level(tracing::Level::DEBUG)
        .with_max_level(tracing::Level::INFO)
        .init();

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

    // 1. Membership (UDP gossip):
    let membership = MembershipService::new(bind_addr, seed_nodes).await?;
    tracing::info!("Node ID: {:?}", membership.local_node.id);

    // 2. Storage layer:
    let partitioner = PartitionManager::new(membership.clone());

    let books = Arc::new(DistributedMap::<String, BookMetadata>::new(
        membership.clone(),
        partitioner.clone(),
    ));

    let queue = Arc::new(DistributedQueue::new(
        membership.clone(),
        partitioner.clone(),
    ));
    let reqistry = TaskHandlerRegistry::new();

    reqistry.register("test_handler", |_task| async move {
        tracing::info!("Executing test task!");
        tokio::time::sleep(Duration::from_secs(2)).await;
        Ok(())
    });

    let executor = TaskExecutor::new(queue.clone(), reqistry, 4);

    executor.start().await;

    // 3. HTTP Router:
    let app = Router::new()
        .route("/put", post(handle_put_book))
        .route("/get/:key", get(handle_get_book))
        .route("/internal/get/:key", get(handle_get_internal_book))
        .route("/forward_put", post(handle_forward_put_book))
        .route("/replicate", post(handle_replicate_book))
        .route("/task/submit", post(handle_submit_task))
        .route("/task/status/:id", get(handle_get_task_status))
        .route("/internal/submit_task", post(handle_internal_submit_task))
        .layer(Extension(books))
        .layer(Extension(queue));

    // 4. Spawn membership service:
    let service_clone = membership.clone();
    tokio::spawn(async move {
        service_clone.start().await;
    });

    // 5. Spawn stats reporter:
    let stats_service = membership.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

        loop {
            interval.tick().await;
            let alive = stats_service.get_alive_members();
            tracing::info!("Cluster stats: {} alive nodes", alive.len());
            for node in alive {
                tracing::info!(
                    "  - {:?} gossip={} http={} (inc={})",
                    node.id,
                    node.gossip_addr,
                    node.http_addr,
                    node.incarnation
                );
            }
        }
    });

    // 6. Start HTTP server:
    let http_port = bind_addr.port() + 1000;
    let http_addr = SocketAddr::new(bind_addr.ip(), http_port);

    tracing::info!("HTTP server listening on {}", http_addr);
    tracing::info!("Press Ctrl+C to shutdown");

    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BookMetadata {
    name: String,
    author: String,
}

async fn handle_get_internal_book(
    map: Extension<Arc<DistributedMap<String, BookMetadata>>>,
    json: Path<String>,
) -> (StatusCode, Json<GetResponse>) {
    handle_get_internal::<String, BookMetadata>(map, json).await
}

async fn handle_get_book(
    map: Extension<Arc<DistributedMap<String, BookMetadata>>>,
    json: Path<String>,
) -> (StatusCode, Json<GetResponse>) {
    handle_get::<String, BookMetadata>(map, json).await
}

async fn handle_forward_put_book(
    map: Extension<Arc<DistributedMap<String, BookMetadata>>>,
    json: Json<ForwardPutRequest>,
) -> (StatusCode, Json<PutResponse>) {
    handle_forward_put::<String, BookMetadata>(map, json).await
}

async fn handle_put_book(
    map: Extension<Arc<DistributedMap<String, BookMetadata>>>,
    json: Json<PutRequest>,
) -> (StatusCode, Json<PutResponse>) {
    handle_put::<String, BookMetadata>(map, json).await
}

async fn handle_replicate_book(
    map: Extension<Arc<DistributedMap<String, BookMetadata>>>,
    json: Json<ReplicateRequest>,
) -> (StatusCode, Json<PutResponse>) {
    handle_replicate::<String, BookMetadata>(map, json).await
}
