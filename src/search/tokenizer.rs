//! Text Tokenizer
//!
//! Provides utilities for converting raw text into normalized tokens for indexing and querying.
//! Uses Regular Expressions to filter out non-alphabetic characters and short words.

use regex::Regex;
use std::collections::HashSet;

/// Tokenizes a full document body for indexing.
///
/// # Operations
/// - **Normalization**: Converts all text to lowercase to ensure case-insensitive matching.
/// - **Filtering**: Removes words with length <= 2 (e.g., "is", "at", "to") to reduce index size.
/// - **Deduplication**: Returns a `HashSet` to ensure only unique terms are stored in the index
///   for a specific document (Boolean model optimization).
pub fn tokenize_text(text: &str) -> HashSet<String> {
    let re = Regex::new(r"\b[a-zA-Z]+\b").unwrap();
    re.find_iter(&text.to_lowercase())
        .map(|m| m.as_str().to_string())
        .filter(|word| word.len() > 2)
        .collect()
}

/// Tokenizes a user search query.
///
/// Similar to `tokenize_text`, but returns a `Vec` to allow the search engine
/// to potentially consider term frequency in the query (although the current engine uses a boolean-like match).
///
/// # Operations
/// - **Normalization**: Converts text to lowercase.
/// - **Splitting**: Splits by whitespace.
/// - **Sanitization**: Trims non-alphanumeric characters from the edges of words.
/// - **Filtering**: Removes short words (length <= 2).
pub fn tokenize_query(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split_whitespace()
        .filter(|word| word.len() > 2)
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .filter(|word| !word.is_empty())
        .collect()
}