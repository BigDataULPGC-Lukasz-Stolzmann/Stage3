use super::types::*;
use crate::membership::types::NodeId;
use serde::{Deserialize, Serialize};

// Endpoints
pub const ENDPOINT_SUBMIT_TASK: &str = "/task/submit";
pub const ENDPOINT_INTERNAL_SUBMIT: &str = "/internal/submit_task";
pub const ENDPOINT_TASK_STATUS: &str = "/task/status";

// Submit task (public API)
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitTaskRequest {
    pub task: Task,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitTaskResponse {
    pub task_id: TaskId,
}

// Forward task (internal inter-node communication)
#[derive(Debug, Serialize, Deserialize)]
pub struct ForwardTaskRequest {
    pub partition: u32,
    pub task_id: TaskId,
    pub task: Task,
}

// Task status check (public API)
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskStatusResponse {
    pub task_id: TaskId,
    pub status: TaskStatus,
    pub assigned_to: Option<NodeId>,
    pub created_at: u64,
}
