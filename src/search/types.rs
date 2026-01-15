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
