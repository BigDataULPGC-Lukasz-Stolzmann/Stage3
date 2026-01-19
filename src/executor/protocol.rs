//! Network Protocol Definitions
//!
//! Defines the Data Transfer Objects (DTOs) used for HTTP communication between nodes
//! regarding task submission, forwarding, and replication.
//!
//! Constants define the specific API endpoints used for internal cluster communication.

use super::types::*;
use crate::membership::types::NodeId;
use serde::{Deserialize, Serialize};

// --- API Endpoints ---

/// Public endpoint for external clients to submit tasks.
/// Note: External clients usually hit Nginx, which routes to this endpoint on any node.
pub const ENDPOINT_SUBMIT_TASK: &str = "/task/submit";
/// Internal endpoint for forwarding a task from a non-owner node to the primary owner.
pub const ENDPOINT_INTERNAL_SUBMIT: &str = "/internal/submit_task";
/// Internal endpoint to fetch a single task's data (used during remote lookups).
pub const ENDPOINT_TASK_INTERNAL_GET: &str = "/internal/get_task";
/// Public endpoint to check the status of a specific task.
pub const ENDPOINT_TASK_STATUS: &str = "/task/status";
/// Internal endpoint for Primary nodes to push task data to Backup nodes.
pub const ENDPOINT_TASK_REPLICATE: &str = "/internal/replicate_task";
/// Internal endpoint for Anti-Entropy to fetch all tasks in a specific partition.
pub const ENDPOINT_TASK_PARTITION_DUMP: &str = "/internal/task_partition";

// --- Data Transfer Objects (DTOs) ---

/// Payload for submitting a new task.
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitTaskRequest {
    pub task: Task,
}

/// Response containing the ID of the submitted task.
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitTaskResponse {
    pub task_id: TaskId,
}

/// Response for an internal task fetch.
#[derive(Debug, Serialize, Deserialize)]
pub struct GetTaskResponse {
    pub task: Option<TaskEntry>,
}

/// Payload used when forwarding a task submission to the correct Primary node.
#[derive(Debug, Serialize, Deserialize)]
pub struct ForwardTaskRequest {
    /// The target partition ID (already calculated by the sender).
    pub partition: u32,
    pub task_id: TaskId,
    pub task: Task,
}

/// Public response object for task status queries.
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskStatusResponse {
    pub task_id: TaskId,
    pub status: TaskStatus,
    pub assigned_to: Option<NodeId>,
    pub created_at: u64,
}

/// Payload for replicating a task entry to a Backup node.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReplicateTaskRequest {
    pub partition: u32,
    pub task_id: TaskId,
    pub entry: TaskEntry,
}

/// A single item in a partition dump.
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskPartitionEntry {
    pub task_id: TaskId,
    pub entry: TaskEntry,
}

/// Response format for bulk partition transfer (Anti-Entropy).
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskPartitionDumpResponse {
    pub partition: u32,
    pub entries: Vec<TaskPartitionEntry>,
}