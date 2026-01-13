use serde::{Deserialize, Serialize};

pub const ENDPOINT_REPLICATE: &str = "/replicate";
pub const ENDPOINT_FORWARD_PUT: &str = "/forward_put";
pub const ENSPOINT_PUT: &str = "/put";
pub const ENDPOINT_GET: &str = "/get";
pub const ENDPOINT_GET_INTERNAL: &str = "/internal/get";

#[derive(Debug, Serialize, Deserialize)]
pub struct ReplicateRequest {
    pub partition: u32,
    pub key: String,
    pub value_json: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ForwardPutRequest {
    pub partition: u32,
    pub key: String,
    pub value_json: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PutRequest {
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
