//! Text Tokenizer
//!
//! Provides utilities for converting raw text into normalized tokens for indexing and querying.
//! Uses Regular Expressions to filter out non-alphabetic characters and short words.

use regex::Regex;
use std::collections::HashSet;

/// Tokenizes a full document body.
///
/// - Normalizes text to lowercase.
/// - Filters out words with length <= 2.
/// - Returns a `HashSet` to ensure only unique terms are stored in the index for a specific doc.
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
/// to potentially weigh repeated terms higher (though current implementation is boolean).
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

