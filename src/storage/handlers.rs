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
use super::protocol::{ForwardPutRequest, GetResponse, PutRequest, PutResponse, ReplicateRequest};

pub async fn handle_put<K, V>(
    Extension(map): Extension<Arc<DistributedMap<K, V>>>,
    Json(req): Json<PutRequest>,
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
    tracing::info!("PUT SUCCESS: {:?}", key.to_string());
    (StatusCode::OK, Json(PutResponse { success: true }))
    // match map.put(key, value).await {
    //     Ok(_) => (StatusCode::OK, Json(PutResponse { success: true })),
    //     Err(e) => {
    //         tracing::error!("Failed to put: {}", e);
    //         (
    //             StatusCode::INTERNAL_SERVER_ERROR,
    //             Json(PutResponse { success: false }),
    //         )
    //     }
    // }
}

pub async fn handle_get<K, V>(
    Extension(map): Extension<Arc<DistributedMap<K, V>>>,
    Path(key_str): Path<String>,
) -> (StatusCode, Json<GetResponse>)
where
    K: ToString + FromStr + Clone + Hash + Eq + Send + Sync + 'static,
    <K as FromStr>::Err: std::fmt::Display,
    V: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    let key: K = match key_str.parse() {
        Ok(k) => k,
        Err(e) => {
            tracing::error!("Failed to parse key: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(GetResponse { value_json: None }),
            );
        }
    };

    tracing::info!("SUCCES GET KEY: {}", key.to_string());
    (StatusCode::OK, Json(GetResponse { value_json: None }))
    // match map.get(&key).await {
    //     Some(value) => match serde_json::to_string(&value) {
    //         Ok(value_json) => (
    //             StatusCode::OK,
    //             Json(GetResponse {
    //                 value_json: Some(value_json),
    //             }),
    //         ),
    //         Err(_) => (
    //             StatusCode::INTERNAL_SERVER_ERROR,
    //             Json(GetResponse { value_json: None }),
    //         ),
    //     },
    //     None => (
    //         StatusCode::NOT_FOUND,
    //         Json(GetResponse { value_json: None }),
    //     ),
    // }
}

pub async fn handle_forward_put<K, V>(
    Extension(map): Extension<Arc<DistributedMap<K, V>>>,
    Json(req): Json<ForwardPutRequest>,
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
    (StatusCode::OK, Json(PutResponse { success: true }))
    // match map.put_local(key, value).await {
    //     Ok(_) => (StatusCode::OK, Json(PutResponse { success: true })),
    //     Err(e) => {
    //         tracing::error!("Failed to put local: {}", e);
    //         (
    //             StatusCode::INTERNAL_SERVER_ERROR,
    //             Json(PutResponse { success: false }),
    //         )
    //     }
    // }
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
