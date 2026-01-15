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
    handle_get_task_internal, handle_get_task_status, handle_internal_submit_task,
    handle_replicate_task, handle_submit_task,
};
use distributed_cluster::executor::protocol::{
    ENDPOINT_INTERNAL_SUBMIT, ENDPOINT_SUBMIT_TASK, ENDPOINT_TASK_INTERNAL_GET,
    ENDPOINT_TASK_REPLICATE, ENDPOINT_TASK_STATUS,
};
use distributed_cluster::executor::queue::DistributedQueue;
use distributed_cluster::executor::registry::TaskHandlerRegistry;
use distributed_cluster::executor::types::Task;
use distributed_cluster::membership::service::MembershipService;
use distributed_cluster::ingestion::handlers::{handle_ingest_gutenberg, handle_ingest_status};
use distributed_cluster::ingestion::types::RawDocument;
use distributed_cluster::search::handlers::{handle_search, handle_create_book};
use distributed_cluster::search::tokenizer::tokenize_text;
use distributed_cluster::search::types::BookMetadata;
use distributed_cluster::storage::handlers::*;
use distributed_cluster::storage::memory::DistributedMap;
use distributed_cluster::storage::partitioner::PartitionManager;
use distributed_cluster::storage::protocol::{
    ENDPOINT_FORWARD_PUT, ENDPOINT_GET, ENDPOINT_GET_INTERNAL, ENDPOINT_PUT, ENDPOINT_REPLICATE,
    ForwardPutRequest, GetResponse, PutRequest, PutResponse, ReplicateRequest,
};
use std::net::SocketAddr;
use std::sync::Arc;

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
    let replication_factor = std::env::var("REPLICATION_FACTOR")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2);
    let partitioner = PartitionManager::new_with_replication(membership.clone(), replication_factor);

    let books = Arc::new(DistributedMap::<String, BookMetadata>::new_with_base(
        membership.clone(),
        partitioner.clone(),
        "/books",
    ));

    let datalake = Arc::new(DistributedMap::<String, RawDocument>::new_with_base(
        membership.clone(),
        partitioner.clone(),
        "/datalake",
    ));

    let queue = Arc::new(DistributedQueue::new(
        membership.clone(),
        partitioner.clone(),
    ));

    let index_map = Arc::new(DistributedMap::<String, Vec<String>>::new_with_base(
        membership.clone(),
        partitioner.clone(),
        "/index",
    ));
    let task_registry = TaskHandlerRegistry::new();

    let index_map_clone = index_map.clone();
    let datalake_clone = datalake.clone();
    task_registry.register("index_document", move |task| {
        let index_map = index_map_clone.clone();
        let datalake = datalake_clone.clone();
        async move {
            let Task::Execute { payload, .. } = task;
            let payload: distributed_cluster::ingestion::types::IndexTaskPayload =
                serde_json::from_value(payload)?;

            let Some(raw_doc) = datalake.get(&payload.book_id).await else {
                tracing::warn!("Index task skipped; missing doc {}", payload.book_id);
                return Ok(());
            };

            let tokens = tokenize_text(&raw_doc.body);
            for token in tokens {
                let mut book_ids = index_map.get(&token).await.unwrap_or_default();

                if !book_ids.contains(&payload.book_id) {
                    book_ids.push(payload.book_id.clone());
                }

                index_map.put(token, book_ids).await?;
            }

            tracing::info!("Indexed book {}", payload.book_id);
            Ok(())
        }
    });

    let executor = TaskExecutor::new(queue.clone(), task_registry, 4);

    executor.start().await;

    // 3. HTTP Router:
    let app = Router::new()
        // Ingestion + search routes
        .route("/ingest/:book_id", post(handle_ingest_gutenberg))
        .route("/ingest/status/:book_id", get(handle_ingest_status))
        .route("/search", get(handle_search))
        .route("/books", post(handle_create_book))
        // Metadata storage routes
        .nest(
            "/books",
            Router::new()
                .route(ENDPOINT_PUT, post(handle_put_book))
                .route(&format!("{}/:key", ENDPOINT_GET), get(handle_get_book))
                .route(
                    &format!("{}/:key", ENDPOINT_GET_INTERNAL),
                    get(handle_get_internal_book),
                )
                .route(ENDPOINT_FORWARD_PUT, post(handle_forward_put_book))
                .route(ENDPOINT_REPLICATE, post(handle_replicate_book)),
        )
        // Datalake routes
        .nest(
            "/datalake",
            Router::new()
                .route(ENDPOINT_PUT, post(handle_put_datalake))
                .route(&format!("{}/:key", ENDPOINT_GET), get(handle_get_datalake))
                .route(
                    &format!("{}/:key", ENDPOINT_GET_INTERNAL),
                    get(handle_get_internal_datalake),
                )
                .route(ENDPOINT_FORWARD_PUT, post(handle_forward_put_datalake))
                .route(ENDPOINT_REPLICATE, post(handle_replicate_datalake)),
        )
        // Index routes
        .nest(
            "/index",
            Router::new()
                .route(ENDPOINT_PUT, post(handle_put_index))
                .route(&format!("{}/:key", ENDPOINT_GET), get(handle_get_index))
                .route(
                    &format!("{}/:key", ENDPOINT_GET_INTERNAL),
                    get(handle_get_internal_index),
                )
                .route(ENDPOINT_FORWARD_PUT, post(handle_forward_put_index))
                .route(ENDPOINT_REPLICATE, post(handle_replicate_index)),
        )
        // Executor routes
        .route(ENDPOINT_SUBMIT_TASK, post(handle_submit_task))
        .route(
            &format!("{}/:id", ENDPOINT_TASK_STATUS),
            get(handle_get_task_status),
        )
        .route(
            &format!("{}/:id", ENDPOINT_TASK_INTERNAL_GET),
            get(handle_get_task_internal),
        )
        .route(ENDPOINT_INTERNAL_SUBMIT, post(handle_internal_submit_task))
        .route(ENDPOINT_TASK_REPLICATE, post(handle_replicate_task))
        .layer(Extension(books))
        .layer(Extension(datalake))
        .layer(Extension(queue))
        .layer(Extension(index_map));

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

async fn handle_get_internal_datalake(
    map: Extension<Arc<DistributedMap<String, RawDocument>>>,
    json: Path<String>,
) -> (StatusCode, Json<GetResponse>) {
    handle_get_internal::<String, RawDocument>(map, json).await
}

async fn handle_get_datalake(
    map: Extension<Arc<DistributedMap<String, RawDocument>>>,
    json: Path<String>,
) -> (StatusCode, Json<GetResponse>) {
    handle_get::<String, RawDocument>(map, json).await
}

async fn handle_forward_put_datalake(
    map: Extension<Arc<DistributedMap<String, RawDocument>>>,
    json: Json<ForwardPutRequest>,
) -> (StatusCode, Json<PutResponse>) {
    handle_forward_put::<String, RawDocument>(map, json).await
}

async fn handle_put_datalake(
    map: Extension<Arc<DistributedMap<String, RawDocument>>>,
    json: Json<PutRequest>,
) -> (StatusCode, Json<PutResponse>) {
    handle_put::<String, RawDocument>(map, json).await
}

async fn handle_replicate_datalake(
    map: Extension<Arc<DistributedMap<String, RawDocument>>>,
    json: Json<ReplicateRequest>,
) -> (StatusCode, Json<PutResponse>) {
    handle_replicate::<String, RawDocument>(map, json).await
}

async fn handle_get_internal_index(
    map: Extension<Arc<DistributedMap<String, Vec<String>>>>,
    json: Path<String>,
) -> (StatusCode, Json<GetResponse>) {
    handle_get_internal::<String, Vec<String>>(map, json).await
}

async fn handle_get_index(
    map: Extension<Arc<DistributedMap<String, Vec<String>>>>,
    json: Path<String>,
) -> (StatusCode, Json<GetResponse>) {
    handle_get::<String, Vec<String>>(map, json).await
}

async fn handle_forward_put_index(
    map: Extension<Arc<DistributedMap<String, Vec<String>>>>,
    json: Json<ForwardPutRequest>,
) -> (StatusCode, Json<PutResponse>) {
    handle_forward_put::<String, Vec<String>>(map, json).await
}

async fn handle_put_index(
    map: Extension<Arc<DistributedMap<String, Vec<String>>>>,
    json: Json<PutRequest>,
) -> (StatusCode, Json<PutResponse>) {
    handle_put::<String, Vec<String>>(map, json).await
}

async fn handle_replicate_index(
    map: Extension<Arc<DistributedMap<String, Vec<String>>>>,
    json: Json<ReplicateRequest>,
) -> (StatusCode, Json<PutResponse>) {
    handle_replicate::<String, Vec<String>>(map, json).await
}
