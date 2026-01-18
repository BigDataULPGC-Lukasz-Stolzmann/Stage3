//! Search Data Types
//!
//! Defines the Data Transfer Objects (DTOs) for the search API, including
//! request parameters, result structures, and book metadata.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
/// Includes metadata about the query execution (filters, counts) and the
/// paginated list of results.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub query: String,
    pub filters: HashMap<String, String>,
    pub total_count: usize,
    pub count: usize,
    pub results: Vec<SearchResultItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub book_id: String,
    pub title: String,
    pub author: String,
    pub score: usize,
}

/// Comprehensive metadata for an indexed book.
///
/// This data is stored separately from the inverted index and is retrieved
/// only when a book appears in search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookMetadata {
    pub book_id: String,
    pub title: String,
    pub author: String,
    pub language: String,
    pub year: Option<u32>,
    pub word_count: usize,
    pub unique_words: usize,
}
