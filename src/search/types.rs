//! Search Data Types
//!
//! Defines the Data Transfer Objects (DTOs) for the search API, including
//! request parameters, result structures, and book metadata.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a simplified view of a book result.
///
/// This structure is often used for internal data passing or simplified API responses
/// where scoring information is not required.
#[derive(Debug, Serialize, Deserialize)]
pub struct BookResult {
    pub book_id: String,
    pub title: String,
    pub author: String,
    pub language: String,
    pub year: Option<u32>,
}

/// The standard response format for search queries.
///
/// Wraps the actual search results with metadata about the execution context,
/// enabling pagination and client-side filtering.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    /// The original query string provided by the user.
    pub query: String,
    /// Active filters applied to the search (e.g., language=en).
    pub filters: HashMap<String, String>,
    /// The total number of documents matching the query (before pagination).
    pub total_count: usize,
    /// The number of results returned in this specific response page.
    pub count: usize,
    /// The list of ranked search results.
    pub results: Vec<SearchResultItem>,
}

/// A single item in the search results list.
///
/// Represents a document that matched the query, including its relevance score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    /// The unique identifier of the book.
    pub book_id: String,
    /// The title of the book.
    pub title: String,
    /// The author of the book.
    pub author: String,
    /// The calculated relevance score (higher is better).
    /// Currently based on the term frequency of query terms within the document.
    pub score: usize,
}

/// Comprehensive metadata for an indexed book.
///
/// This data is stored in the `books` DistributedMap, separate from the Inverted Index.
/// It is retrieved (hydrated) only when a book ID appears in the search results.
/// Separation of keys (index) and values (metadata) optimizes memory usage during
/// the ranking phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookMetadata {
    pub book_id: String,
    pub title: String,
    pub author: String,
    pub language: String,
    pub year: Option<u32>,
    /// Total number of words in the document.
    pub word_count: usize,
    /// Count of distinct tokens in the document.
    pub unique_words: usize,
}