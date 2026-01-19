//! Storage Network Protocol
//!
//! Defines the API endpoints and Data Transfer Objects (DTOs) used for
//! internode communication (PUT, GET, Replication).
//!
//! These structures are serialized (typically via JSON) and sent over HTTP to coordinate
//! state changes across the cluster.

use serde::{Deserialize, Serialize};

// --- API Endpoints ---

/// Endpoint for synchronizing data from a Primary to a Backup node.
pub const ENDPOINT_REPLICATE: &str = "/replicate";
/// Endpoint for forwarding a write request from a non-owner to the Primary.
pub const ENDPOINT_FORWARD_PUT: &str = "/forward_put";
/// Public endpoint for client write requests.
pub const ENDPOINT_PUT: &str = "/put";
/// Public endpoint for client read requests.
pub const ENDPOINT_GET: &str = "/get";
/// Internal endpoint for direct key retrieval (bypassing routing logic).
pub const ENDPOINT_GET_INTERNAL: &str = "/internal/get";
/// Internal endpoint for bulk data transfer (Anti-Entropy).
pub const ENDPOINT_PARTITION_DUMP: &str = "/internal/partition";

// --- Data Transfer Objects ---

/// Payload for synchronizing data from a Primary node to a Backup node.
///
/// Sent by the Primary node immediately after a successful local write.
/// Includes the `op_id` to allow the backup node to perform idempotency checks,
/// ensuring that retried requests do not cause duplicate processing.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReplicateRequest {
    /// The partition identifier this key belongs to.
    pub partition: u32,
    /// Unique Operation ID (UUID) for deduplication.
    pub op_id: String,
    /// The data key.
    pub key: String,
    /// The serialized JSON string of the value.
    pub value_json: String,
}

/// Request used when a node receives a write for a key it doesn't own.
///
/// If Node A receives a PUT for Key K, and the partitioner determines Node B is the Primary,
/// Node A wraps the request in this structure and forwards it to Node B.
#[derive(Debug, Serialize, Deserialize)]
pub struct ForwardPutRequest {
    /// The target partition.
    pub partition: u32,
    /// Unique Operation ID to track this write across hops.
    pub op_id: String,
    /// The data key.
    pub key: String,
    /// The serialized JSON string of the value.
    pub value_json: String,
}

/// Standard client request for writing data.
#[derive(Debug, Serialize, Deserialize)]
pub struct PutRequest {
    /// Optional Operation ID (if provided by client) or generated internally.
    pub op_id: String,
    /// The data key.
    pub key: String,
    /// The serialized JSON string of the value.
    pub value_json: String,
}

/// Standard response for data retrieval.
#[derive(Debug, Serialize, Deserialize)]
pub struct GetResponse {
    /// The value, if found, serialized as a JSON string.
    /// `None` indicates the key does not exist.
    pub value_json: Option<String>,
}

/// Standard acknowledgment for write operations.
#[derive(Debug, Serialize, Deserialize)]
pub struct PutResponse {
    /// Indicates if the operation was successfully persisted (and replicated).
    pub success: bool,
}

/// A single key-value pair used in bulk transfer.
#[derive(Debug, Serialize, Deserialize)]
pub struct KeyValueJson {
    pub key: String,
    pub value_json: String,
}

/// Response format for partition dump requests (Anti-Entropy).
///
/// Contains the complete dataset for a specific partition, used to synchronize
/// nodes that have fallen behind or are recovering from failure.
#[derive(Debug, Serialize, Deserialize)]
pub struct PartitionDumpResponse {
    pub partition: u32,
    pub entries: Vec<KeyValueJson>,
}