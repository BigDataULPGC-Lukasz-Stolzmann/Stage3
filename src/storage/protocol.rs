//! Storage Network Protocol
//!
//! Defines the API endpoints and Data Transfer Objects (DTOs) used for
//! internode communication (PUT, GET, Replication).

use serde::{Deserialize, Serialize};

pub const ENDPOINT_REPLICATE: &str = "/replicate";
pub const ENDPOINT_FORWARD_PUT: &str = "/forward_put";
pub const ENDPOINT_PUT: &str = "/put";
pub const ENDPOINT_GET: &str = "/get";
pub const ENDPOINT_GET_INTERNAL: &str = "/internal/get";
pub const ENDPOINT_PARTITION_DUMP: &str = "/internal/partition";

/// Payload for synchronizing data from a Primary node to a Backup node.
/// Includes the `op_id` for idempotency checks.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReplicateRequest {
    pub partition: u32,
    pub op_id: String,
    pub key: String,
    pub value_json: String,
}

/// Request used when a node receives a write for a key it doesn't own.
/// It forwards the data to the correct Primary node.
#[derive(Debug, Serialize, Deserialize)]
pub struct ForwardPutRequest {
    pub partition: u32,
    pub op_id: String,
    pub key: String,
    pub value_json: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PutRequest {
    pub op_id: String,
    pub key: String,
    pub value_json: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetResponse {
    pub value_json: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PutResponse {
    pub success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KeyValueJson {
    pub key: String,
    pub value_json: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PartitionDumpResponse {
    pub partition: u32,
    pub entries: Vec<KeyValueJson>,
}
