use crate::membership::types::NodeId;
use serde::{Deserialize, Serialize};

/// Unique identifier for a task within the cluster.
/// Wrapper around a UUID string to ensure global uniqueness.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TaskId(pub String);

impl TaskId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed { error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Task {
    Execute {
        handler: String,
        payload: serde_json::Value,
    },
}

/// The internal representation of a task within the queue.
///
/// Includes the task definition itself, as well as metadata regarding its
/// lifecycle state (status, assignment, lease expiration).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEntry {
    pub task: Task,
    pub status: TaskStatus,
    pub assigned_to: Option<NodeId>,
    pub created_at: u64,
    pub lease_expires: Option<u64>,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
