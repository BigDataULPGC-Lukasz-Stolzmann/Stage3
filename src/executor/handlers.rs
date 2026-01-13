use super::protocol::*;
use super::queue::DistributedQueue;
use super::types::*;

use axum::{Extension, Json, extract::Path, http::StatusCode};
use std::sync::Arc;

pub async fn handle_submit_task(
    Extension(queue): Extension<Arc<DistributedQueue>>,
    Json(req): Json<SubmitTaskRequest>,
) -> (StatusCode, Json<SubmitTaskResponse>) {
    match queue.submit(req.task).await {
        Ok(task_id) => {
            tracing::info!("Task submitted successfully: {}", task_id.0);
            (StatusCode::OK, Json(SubmitTaskResponse { task_id }))
        }
        Err(e) => {
            tracing::error!("Failed to submit task: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SubmitTaskResponse {
                    task_id: TaskId::new(), // Dummy ID for error response TODO: to be changed
                }),
            )
        }
    }
}

pub async fn handle_internal_submit_task(
    Extension(queue): Extension<Arc<DistributedQueue>>,
    Json(req): Json<ForwardTaskRequest>,
) -> (StatusCode, Json<SubmitTaskResponse>) {
    tracing::debug!(
        "Received forwarded task {} for partition {}",
        req.task_id.0,
        req.partition
    );

    queue.store_local(
        req.partition,
        req.task_id.clone(),
        TaskEntry {
            task: req.task,
            status: TaskStatus::Pending,
            assigned_to: None,
            created_at: now_ms(),
            lease_expires: None,
        },
    );

    (
        StatusCode::OK,
        Json(SubmitTaskResponse {
            task_id: req.task_id,
        }),
    )
}

pub async fn handle_get_task_status(
    Extension(queue): Extension<Arc<DistributedQueue>>,
    Path(task_id_str): Path<String>,
) -> (StatusCode, Json<Option<TaskStatusResponse>>) {
    let task_id = TaskId(task_id_str);

    match queue.get_task(&task_id) {
        Some(entry) => {
            tracing::debug!("Task status query: {} -> {:?}", task_id.0, entry.status);
            (
                StatusCode::OK,
                Json(Some(TaskStatusResponse {
                    task_id,
                    status: entry.status,
                    assigned_to: entry.assigned_to,
                    created_at: entry.created_at,
                })),
            )
        }
        None => {
            tracing::debug!("Task not found: {}", task_id.0);
            (StatusCode::NOT_FOUND, Json(None))
        }
    }
}
