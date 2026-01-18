//! Ingestion Data Types
//!
//! Defines the Data Transfer Objects (DTOs) and storage structures used within the ingestion pipeline.
//! Includes structures for API responses, queue payloads, and persistent storage.

use serde::{Deserialize, Serialize};

/// Represents a fully processed document stored in the Datalake.
///
/// Contains the raw content split into header and body, preserving the original
/// source URL for provenance. This structure is what the `Indexer` retrieves later.
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

/// The payload sent to the `DistributedQueue` to trigger indexing.
///
/// This minimal structure notifies the Executor that a document is available
/// in the Datalake and ready to be tokenized and added to the Inverted Index.
#[derive(Debug, Serialize)]
pub struct IngestStatusResponse {
    pub book_id: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexTaskPayload {
    pub book_id: String,
}
