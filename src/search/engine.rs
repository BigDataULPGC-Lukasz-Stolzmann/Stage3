//! Search Engine Logic
//!
//! Implements the core information retrieval algorithm.
//! Coordinates between the Inverted Index (term -> doc_ids) and the Metadata Store.

use super::types::BookMetadata;
use crate::search::tokenizer::tokenize_query;
use crate::storage::memory::DistributedMap;
use std::collections::HashMap;
use std::sync::Arc;

/// Executes the ranking algorithm.
///
/// 1. **Tokenize**: Splits the user query into keywords.
/// 2. **Lookup**: Fetches the list of document IDs for each token from the `index_map`.
/// 3. **Score**: Calculates a relevance score based on how many query terms appear in the document.
/// 4. **Hydrate**: Fetches full metadata (title, author) for the matching document IDs.
/// 5. **Sort**: Orders results by score (descending).
pub async fn search(
    query: &str,
    index_map: Arc<DistributedMap<String, Vec<String>>>,
    books_map: Arc<DistributedMap<String, BookMetadata>>,
) -> Vec<(BookMetadata, usize)> {
    let query_tokens = tokenize_query(query);

    let mut book_scores: HashMap<String, usize> = HashMap::new();
    for token in query_tokens.iter() {
        if let Some(book_index) = index_map.get(token).await {
            for book_id in book_index {
                book_scores
                    .entry(book_id.clone())
                    .and_modify(|score| *score += 1)
                    .or_insert(1);
            }
        }
    }

    let mut results: Vec<(BookMetadata, usize)> = Vec::new();
    for (book_id, score) in book_scores.iter() {
        if let Some(metadata) = books_map.get(book_id).await {
            results.push((metadata.clone(), *score));
        }
    }

    results.sort_by(|a, b| b.1.cmp(&a.1));
    results
}
