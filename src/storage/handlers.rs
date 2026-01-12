use axum::{
    Json,
    extract::{Extension, Path},
    http::StatusCode,
};
use serde::{Serialize, de::DeserializeOwned};
use std::hash::Hash;
use std::str::FromStr;
use std::sync::Arc;

use super::memory::DistributedMap;
use super::protocol::{ForwardPutRequest, GetResponse, PutResponse, ReplicateRequest};

pub async fn handle_get<K, V>(
    Path(key_str): Path<String>,
    Extension(map): Extension<Arc<DistributedMap<K, V>>>,
) -> Result<Json<GetResponse>, StatusCode>
where
    K: ToString + FromStr + Clone + Hash + Eq + Send + Sync + 'static,
    <K as FromStr>::Err: std::fmt::Display,
    V: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    let key: K = key_str.parse().map_err(|e| {
        tracing::error!("Failed to parse key: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    if let Some(value) = map.get_local(&key) {
        let value_json =
            serde_json::to_string(&value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        return Ok(Json(GetResponse {
            value_json: Some(value_json),
        }));
    }

    Ok(Json(GetResponse { value_json: None }))
}

pub async fn handle_forward_put<K, V>(
    Json(req): Json<ForwardPutRequest>,
    Extension(map): Extension<Arc<DistributedMap<K, V>>>,
) -> Result<Json<PutResponse>, StatusCode>
where
    K: ToString + FromStr + Clone + Hash + Eq + Send + Sync + 'static,
    <K as FromStr>::Err: std::fmt::Display,
    V: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    let key: K = req.key.parse().map_err(|e| {
        tracing::error!("Failed to parse key: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    let value: V = serde_json::from_str(&req.value_json).map_err(|e| {
        tracing::error!("Failed to deserialize value: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    map.put_local(key, value).await.map_err(|e| {
        tracing::error!("Failed to put local: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(PutResponse { success: true }))
}

// Generic handlers - używane przez concrete wrappers w main.rs
pub async fn handle_replicate<K, V>(
    Extension(map): Extension<Arc<DistributedMap<K, V>>>,
    Json(req): Json<ReplicateRequest>,
) -> (StatusCode, Json<PutResponse>)
where
    K: ToString + FromStr + Clone + Hash + Eq + Send + Sync + 'static,
    <K as FromStr>::Err: std::fmt::Display,
    V: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    let key: K = match req.key.parse() {
        Ok(k) => k,
        Err(e) => {
            tracing::error!("Failed to parse key: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(PutResponse { success: false }),
            );
        }
    };

    let value: V = match serde_json::from_str(&req.value_json) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Failed to deserialize value: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(PutResponse { success: false }),
            );
        }
    };

    map.store_local(req.partition, key, value);

    tracing::info!("Stored replica for partition {}", req.partition);

    (StatusCode::OK, Json(PutResponse { success: true }))
}
