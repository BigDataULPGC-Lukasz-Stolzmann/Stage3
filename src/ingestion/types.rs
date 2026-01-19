//! Ingestion Data Types
//!
//! Defines the Data Transfer Objects (DTOs) and storage structures used within the ingestion pipeline.
//! Includes structures for API responses, queue payloads, and persistent storage.

use serde::{Deserialize, Serialize};

/// Represents a fully processed document stored in the Datalake.
///
/// Contains the raw content split into header and body, preserving the original
/// source URL for provenance. This structure is what the `Indexer` task retrieves later
/// from the distributed storage to perform tokenization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDocument {
    pub book_id: String,
    pub header: String,
    pub body: String,
    pub source_url: String,
}

/// Response returned to the client immediately after the ingestion request is processed.
///
/// Indicates whether the download and storage in the Datalake were successful.
#[derive(Debug, Serialize)]
pub struct IngestResponse {
    pub book_id: String,
    pub status: String,
    pub source_url: String,
}

/// Response format for the status check endpoint.
///
/// Used by the frontend or external clients to verify if a book is currently available
/// in the cluster's Datalake.
#[derive(Debug, Serialize)]
pub struct IngestStatusResponse {
    pub book_id: String,
    pub status: String,
}

/// The payload sent to the `DistributedQueue` to trigger indexing.
///
/// This minimal structure notifies the Executor that a document is available
/// in the Datalake and ready to be tokenized and added to the Inverted Index.
/// By passing only metadata, we keep the message size small and network efficient.
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexTaskPayload {
    pub book_id: String,
}