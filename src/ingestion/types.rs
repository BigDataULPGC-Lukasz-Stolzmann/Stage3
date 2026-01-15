use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDocument {
    pub book_id: String,
    pub header: String,
    pub body: String,
    pub source_url: String,
}

#[derive(Debug, Serialize)]
pub struct IngestResponse {
    pub book_id: String,
    pub status: String,
    pub source_url: String,
}

#[derive(Debug, Serialize)]
pub struct IngestStatusResponse {
    pub book_id: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexTaskPayload {
    pub book_id: String,
}
