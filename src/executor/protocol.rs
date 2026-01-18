//! Network Protocol Definitions
//!
//! Defines the Data Transfer Objects (DTOs) used for HTTP communication between nodes
//! regarding task submission, forwarding, and replication.
//!
//! Constants define the specific API endpoints used for internal cluster communication.

use super::types::*;
use crate::membership::types::NodeId;
use serde::{Deserialize, Serialize};

pub const ENDPOINT_SUBMIT_TASK: &str = "/task/submit";
pub const ENDPOINT_INTERNAL_SUBMIT: &str = "/internal/submit_task";
pub const ENDPOINT_TASK_INTERNAL_GET: &str = "/internal/get_task";
pub const ENDPOINT_TASK_STATUS: &str = "/task/status";
pub const ENDPOINT_TASK_REPLICATE: &str = "/internal/replicate_task";
pub const ENDPOINT_TASK_PARTITION_DUMP: &str = "/internal/task_partition";

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitTaskRequest {
    pub task: Task,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitTaskResponse {
    pub task_id: TaskId,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetTaskResponse {
    pub task: Option<TaskEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ForwardTaskRequest {
    pub partition: u32,
    pub task_id: TaskId,
    pub task: Task,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskStatusResponse {
    pub task_id: TaskId,
    pub status: TaskStatus,
    pub assigned_to: Option<NodeId>,
    pub created_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReplicateTaskRequest {
    pub partition: u32,
    pub task_id: TaskId,
    pub entry: TaskEntry,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskPartitionEntry {
    pub task_id: TaskId,
    pub entry: TaskEntry,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskPartitionDumpResponse {
    pub partition: u32,
    pub entries: Vec<TaskPartitionEntry>,
}
