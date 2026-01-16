use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::Html;
use axum::{routing::get, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Clone)]
struct AppState {
    node_url: String,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct IngestParams {
    id: String,
    node: Option<String>,
}

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    limit: Option<usize>,
    node: Option<String>,
}

#[derive(Deserialize)]
struct NodeParams {
    node: Option<String>,
}

#[derive(Serialize)]
struct ProxyResponse {
    status: u16,
    body: serde_json::Value,
}

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

async fn ui() -> Html<&'static str> {
    Html(include_str!("ui.html"))
}

async fn api_ingest(
    axum::extract::State(state): axum::extract::State<AppState>,
    Query(params): Query<IngestParams>,
) -> Result<Json<ProxyResponse>, (StatusCode, String)> {
    let node_url = resolve_node_url(&state, params.node);
    let url = format!("{}/ingest/{}", node_url, params.id);
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
