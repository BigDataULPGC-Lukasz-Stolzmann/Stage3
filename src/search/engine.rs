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
/// This function performs the fundamental "Map-Reduce" style operation for search:
/// 1. **Tokenize**: Splits the user query into normalized keywords.
/// 2. **Lookup**: Fetches the list of document IDs (postings list) for each token from the distributed `index_map`.
/// 3. **Score**: Calculates a relevance score. Currently implements a coordination-level match,
///    counting how many distinct query terms appear in each document.
/// 4. **Hydrate**: Fetches full metadata (title, author) for the matching document IDs from the `books_map`.
/// 5. **Sort**: Orders results by score (descending) to present the most relevant documents first.
///
/// # Arguments
/// * `query` - The raw search string from the user.
/// * `index_map` - Access to the distributed Inverted Index.
/// * `books_map` - Access to the distributed Metadata Store.
pub async fn search(
    query: &str,
    index_map: Arc<DistributedMap<String, Vec<String>>>,
    books_map: Arc<DistributedMap<String, BookMetadata>>,
) -> Vec<(BookMetadata, usize)> {
    let query_tokens = tokenize_query(query);

    // Scoring Phase: Count occurrences of document IDs across all query tokens.
    // If a document ID appears in the lists for "rust" and "programming", it gets a score of 2.
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

    // Hydration Phase: Resolve Book IDs to full metadata.
    // This involves network calls if the metadata partition is remote.
    let mut results: Vec<(BookMetadata, usize)> = Vec::new();
    for (book_id, score) in book_scores.iter() {
        if let Some(metadata) = books_map.get(book_id).await {
            results.push((metadata.clone(), *score));
        }
    }

    // Ranking Phase: Sort by score descending.
    results.sort_by(|a, b| b.1.cmp(&a.1));
    results
}