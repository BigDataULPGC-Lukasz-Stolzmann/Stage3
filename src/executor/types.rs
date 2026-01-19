use crate::membership::types::NodeId;
use serde::{Deserialize, Serialize};

/// Unique identifier for a task within the cluster.
///
/// Wrapper around a UUID string to ensure global uniqueness.
/// This ID is hashed to determine which partition (and thus which node) owns the task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TaskId(pub String);

impl TaskId {
    /// Generates a new random UUID v4-based TaskId.
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// Represents the lifecycle state of a task in the queue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    /// Task has been submitted but not yet picked up by any worker.
    Pending,
    /// Task is currently being processed by a worker.
    /// This state is accompanied by a `lease_expires` timestamp in `TaskEntry`.
    Running,
    /// Task finished successfully.
    Completed,
    /// Task execution encountered a panic or returned an `Err`.
    Failed { error: String },
}

/// The definition of a unit of work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Task {
    /// A generic execution task.
    Execute {
        /// The name of the registered handler to invoke (e.g., "index_document").
        handler: String,
        /// Arbitrary JSON payload passed to the handler function.
        payload: serde_json::Value,
    },
}

/// The internal representation of a task stored within the `DistributedQueue`.
///
/// Contains the task definition and mutable metadata regarding its execution state.
/// This structure is what gets replicated across nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEntry {
    /// The actual work definition.
    pub task: Task,
    /// Current execution status.
    pub status: TaskStatus,
    /// The ID of the node currently processing this task (if Running).
    pub assigned_to: Option<NodeId>,
    /// Timestamp (ms) when the task was submitted.
    pub created_at: u64,
    /// Timestamp (ms) when the current execution lease expires.
    /// If `now > lease_expires`, the task is considered abandoned and can be reclaimed.
    pub lease_expires: Option<u64>,
}

/// Helper to get the current system time in milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}