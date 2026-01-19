//! Frontend Dashboard Server & Proxy
//!
//! This crate serves as the **Dashboard Server** and **CORS Proxy** for the cluster.
//!
//! ## Architectural Role
//! - **NOT a Load Balancer**: This server does not make traffic distribution decisions.
//!   Load balancing is handled externally by the **Nginx** container (using `least_conn` strategy).
//! - **Static Asset Server**: Serves the Single Page Application (SPA) embedded in the binary.
//! - **CORS Proxy**: Browsers cannot access the cluster nodes directly due to CORS restrictions
//!   and network topology (e.g., Docker networks). This server exposes a unified API (`/api/*`)
//!   that forwards requests to the cluster, acting as a gateway for the frontend.
//!
//! ## Flow
//! `Browser -> UI Server (Proxy) -> Nginx/Cluster Node`

use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::Html;
use axum::{routing::get, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Application state shared across handlers.
/// Contains the default cluster connection URL and a reusable HTTP client.
#[derive(Clone)]
struct AppState {
    /// Default upstream node (usually the Nginx load balancer or a seed node).
    node_url: String,
    /// HTTP client for forwarding requests to the cluster.
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct IngestParams {
    id: String,
    /// Optional: Override the target node for this specific request.
    node: Option<String>,
}

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    limit: Option<usize>,
    /// Optional: Override the target node for this specific request.
    node: Option<String>,
}

#[derive(Deserialize)]
struct NodeParams {
    /// Optional: Override the target node for this specific request.
    node: Option<String>,
}

/// Standardized JSON response wrapper for the frontend.
#[derive(Serialize)]
struct ProxyResponse {
    status: u16,
    body: serde_json::Value,
}

/// Entry Point
///
/// Configures the logging system and starts the UI/Proxy server.
///
/// ## Environment Variables
/// - `NODE_URL`: The default upstream address (e.g., `http://localhost:80` for Nginx).
/// - `UI_BIND`: The local address to bind the dashboard to (default: `127.0.0.1:8080`).
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let node_url =
        std::env::var("NODE_URL").unwrap_or_else(|_| "http://localhost".to_string());
    let bind_addr: SocketAddr = std::env::var("UI_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        .parse()?;

    let state = AppState {
        node_url: node_url.trim_end_matches('/').to_string(),
        client: reqwest::Client::new(),
    };

    let app = Router::new()
        .route("/", get(ui))
        .route("/api/ingest", post(api_ingest))
        .route("/api/search", get(api_search))
        .route("/api/status", get(api_status))
        .route("/api/stats", get(api_stats))
        .with_state(state);

    tracing::info!("UI listening on {}", bind_addr);
    axum::serve(tokio::net::TcpListener::bind(bind_addr).await?, app).await?;

    Ok(())
}

/// Serves the main Dashboard HTML.
///
/// The HTML content is compiled into the binary using `include_str!`,
/// ensuring the dashboard is a single, portable executable.
async fn ui() -> Html<&'static str> {
    Html(include_str!("ui.html"))
}

/// Ingestion Proxy Endpoint.
///
/// **Role**: Pass-through proxy.
/// Forwards `POST /api/ingest` from the browser to the target node's `/ingest/{id}` endpoint.
///
/// **Why?**: Allows the frontend to issue requests without worrying about CORS headers
/// or direct network access to the Docker container network.
async fn api_ingest(
    axum::extract::State(state): axum::extract::State<AppState>,
    Query(params): Query<IngestParams>,
) -> Result<Json<ProxyResponse>, (StatusCode, String)> {
    let node_url = resolve_node_url(&state, params.node);
    let url = format!("{}/ingest/{}", node_url, params.id);
    
    // Proxy the request to the cluster
    let resp = state
        .client
        .post(url)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    let status = resp.status().as_u16();
    let body = resp
        .json::<serde_json::Value>()
        .await
        .unwrap_or_else(|_| serde_json::json!({"error": "invalid json"}));

    Ok(Json(ProxyResponse { status, body }))
}

/// Search Proxy Endpoint.
///
/// **Role**: Pass-through proxy.
/// Forwards `GET /api/search` to the target node.
/// Handles URL encoding of query parameters to ensure safe transmission.
async fn api_search(
    axum::extract::State(state): axum::extract::State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<ProxyResponse>, (StatusCode, String)> {
    let node_url = resolve_node_url(&state, params.node);
    let limit = params.limit.unwrap_or(5);
    let url = format!(
        "{}/search?q={}&limit={}",
        node_url,
        urlencoding::encode(&params.q),
        limit
    );

    let resp = state
        .client
        .get(url)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    let status = resp.status().as_u16();
    let body = resp
        .json::<serde_json::Value>()
        .await
        .unwrap_or_else(|_| serde_json::json!({"error": "invalid json"}));

    Ok(Json(ProxyResponse { status, body }))
}

/// Status Check Proxy Endpoint.
///
/// Proxies requests to check if a specific book ID has been indexed.
async fn api_status(
    axum::extract::State(state): axum::extract::State<AppState>,
    Query(params): Query<IngestParams>,
) -> Result<Json<ProxyResponse>, (StatusCode, String)> {
    let node_url = resolve_node_url(&state, params.node);
    let url = format!("{}/ingest/status/{}", node_url, params.id);

    let resp = state
        .client
        .get(url)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    let status = resp.status().as_u16();
    let body = resp
        .json::<serde_json::Value>()
        .await
        .unwrap_or_else(|_| serde_json::json!({"error": "invalid json"}));

    Ok(Json(ProxyResponse { status, body }))
}

/// Cluster Statistics Proxy.
///
/// Fetches internal metrics (`/health/stats`) from a specific node.
/// This allows the UI to aggregate and visualize metrics from all nodes in the cluster
/// by querying them individually via this proxy.
async fn api_stats(
    axum::extract::State(state): axum::extract::State<AppState>,
    Query(params): Query<NodeParams>,
) -> Result<Json<ProxyResponse>, (StatusCode, String)> {
    let node_url = resolve_node_url(&state, params.node);
    let url = format!("{}/health/stats", node_url);
    let resp = state
        .client
        .get(url)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    let status = resp.status().as_u16();
    let body = resp
        .json::<serde_json::Value>()
        .await
        .unwrap_or_else(|_| serde_json::json!({"error": "invalid json"}));

    Ok(Json(ProxyResponse { status, body }))
}

/// Helper: Target URL Resolution.
///
/// Determines which upstream URL to use for the proxy request:
/// 1. **Override**: If the client specified a `node` param, use that (allows querying specific nodes).
/// 2. **Default**: Otherwise, use the configured `NODE_URL` (typically the Nginx gateway).
/// 3. **Normalization**: Ensures protocol (`http://`) presence and proper formatting.
fn resolve_node_url(state: &AppState, override_url: Option<String>) -> String {
    let candidate = override_url.unwrap_or_else(|| state.node_url.clone());
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return state.node_url.clone();
    }

    let normalized = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{}", trimmed)
    };

    normalized.trim_end_matches('/').to_string()
}