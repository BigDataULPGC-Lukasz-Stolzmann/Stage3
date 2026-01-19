//! HTTP Request Handlers
//!
//! Axum route handlers that expose the `DistributedQueue` functionality via HTTP.
//! These endpoints allow external clients to submit tasks and internal nodes
//! to forward or replicate tasks.
//!
//! **Note on Load Balancing**:
//! The external Nginx container handles global load balancing (using `least_conn`)
//! to distribute client requests across the cluster.
//!
//! These handlers below are responsible for **Internal Partition Routing** (forwarding
//! a task from a node that received it to the node that actually owns the partition).

use super::protocol::*;
use super::queue::DistributedQueue;
use super::types::*;

use axum::{Extension, Json, extract::Path, http::StatusCode};
use std::sync::Arc;

/// External API: Submits a task to the cluster.
///
/// If this node is NOT the partition owner, `queue.submit()` handles
/// the forwarding logic internally.
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
                    task_id: TaskId::new(), // Dummy ID for error response
                }),
            )
        }
    }
}

/// Internal Endpoint: Handles tasks forwarded from other nodes.
///
/// When Node A receives a task meant for Node B (the primary), Node A forwards it here.
/// This handler forces storage as a primary task without re-calculating ownership,
/// preventing infinite forwarding loops.
pub async fn handle_internal_submit_task(
    Extension(queue): Extension<Arc<DistributedQueue>>,
    Json(req): Json<ForwardTaskRequest>,
) -> (StatusCode, Json<SubmitTaskResponse>) {
    tracing::debug!(
        "Received forwarded task {} for partition {}",
        req.task_id.0,
        req.partition
    );

    if let Err(e) = queue
        .store_as_primary(
            req.partition,
            req.task_id.clone(),
            TaskEntry {
                task: req.task,
                status: TaskStatus::Pending,
                assigned_to: None,
                created_at: now_ms(),
                lease_expires: None,
            },
        )
        .await
    {
        tracing::error!("Failed to store forwarded task: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SubmitTaskResponse {
                task_id: req.task_id,
            }),
        );
    }

    (
        StatusCode::OK,
        Json(SubmitTaskResponse {
            task_id: req.task_id,
        }),
    )
}

/// Public API: Checks the status of a specific task.
///
/// 1. Checks if the task is local.
/// 2. If not, proxies the request to the task's owner node.
pub async fn handle_get_task_status(
    Extension(queue): Extension<Arc<DistributedQueue>>,
    Path(task_id_str): Path<String>,
) -> (StatusCode, Json<Option<TaskStatusResponse>>) {
    let task_id = TaskId(task_id_str);
    
    // Check if we are the owner
    let partition = queue.partitioner.get_partition(&task_id.0);
    let owners = queue.partitioner.get_owners(partition);
    
    // Optimization: If we own the partition, serve immediately
    if !owners.is_empty() && owners[0] == queue.membership.local_node.id {
        match queue.get_task(&task_id).await {
            Some(entry) => {
                return (StatusCode::OK, Json(Some(TaskStatusResponse {
                    task_id,
                    status: entry.status,
                    assigned_to: entry.assigned_to,
                    created_at: entry.created_at,
                })));
            }
            None => {
                return (StatusCode::NOT_FOUND, Json(None));
            }
        }
    }

    // Otherwise, perform distributed lookup (queue.get_task handles remote fetch)
    match queue.get_task(&task_id).await {
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

/// Internal Endpoint: Retrieves raw task entry.
/// Used by other nodes when they need to look up a task status but don't own it.
pub async fn handle_get_task_internal(
    Extension(queue): Extension<Arc<DistributedQueue>>,
    Path(task_id_str): Path<String>,
) -> (StatusCode, Json<GetTaskResponse>) {
    let task_id = TaskId(task_id_str);

    match queue.get_task_local(&task_id) {
        Some(entry) => (
            StatusCode::OK,
            Json(GetTaskResponse { task: Some(entry) }),
        ),
        None => (StatusCode::NOT_FOUND, Json(GetTaskResponse { task: None })),
    }
}

/// Internal Endpoint: Handles task replication.
///
/// Invoked by a Primary node to store a copy of a task on a Backup node.
/// This ensures that if the Primary fails, the Backup has the data to take over.
pub async fn handle_replicate_task(
    Extension(queue): Extension<Arc<DistributedQueue>>,
    Json(req): Json<ReplicateTaskRequest>,
) -> StatusCode {
    queue.store_local(req.partition, req.task_id.clone(), req.entry);

    tracing::debug!(
        "Stored replicated task {} in partition {}",
        req.task_id.0,
        req.partition
    );

    StatusCode::OK
}

/// Internal Endpoint: Dumps all tasks in a partition.
///
/// Used by the Anti-Entropy mechanism to synchronize data when a node
/// realizes it is missing a partition it should own (e.g., after restart).
pub async fn handle_task_partition_dump(
    Extension(queue): Extension<Arc<DistributedQueue>>,
    Path(partition): Path<u32>,
) -> (StatusCode, Json<TaskPartitionDumpResponse>) {
    let entries = queue
        .dump_partition(partition)
        .into_iter()
        .map(|(task_id, entry)| TaskPartitionEntry { task_id, entry })
        .collect();

    (
        StatusCode::OK,
        Json(TaskPartitionDumpResponse { partition, entries }),
    )
}