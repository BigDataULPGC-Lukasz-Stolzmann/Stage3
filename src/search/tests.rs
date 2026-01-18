//! Search Module Tests
//!
//! Validates the search pipeline, including text processing, metadata handling, and ranking logic.
//!
//! ## Test Scopes
//! - **Tokenizer**: Ensures text is correctly split, normalized, and filtered.
//! - **Scoring**: Verifies that documents matching more query terms are ranked higher.
//! - **Serialization**: Checks JSON compatibility for API types.

#[cfg(test)]
mod tests {
    use crate::search::tokenizer::{tokenize_query, tokenize_text};
    use crate::search::types::{BookMetadata, SearchResponse, SearchResultItem};
    use std::collections::HashMap;

    // ============================================================
    // TOKENIZER TESTS - tokenize_text
    // ============================================================

    #[test]
    fn test_tokenize_text_basic() {
        let tokens = tokenize_text("Hello World");

        assert!(tokens.contains("hello"));
        assert!(tokens.contains("world"));
    }

    #[test]
    fn test_tokenize_text_lowercase() {
        let tokens = tokenize_text("RUST Programming LANGUAGE");

        // Everything should be lowercase
        assert!(tokens.contains("rust"));
        assert!(tokens.contains("programming"));
        assert!(tokens.contains("language"));

        // Uppercase should not exist
        assert!(!tokens.contains("RUST"));
        assert!(!tokens.contains("Programming"));
    }

    #[test]
    fn test_tokenize_text_filters_short_words() {
        let tokens = tokenize_text("I am a Rust programmer");

        // Words > 2 chars
        assert!(tokens.contains("rust"));
        assert!(tokens.contains("programmer"));

        // Words <= 2 chars are filtered out
        assert!(!tokens.contains("i"));
        assert!(!tokens.contains("am"));
        assert!(!tokens.contains("a"));
    }

    #[test]
    fn test_tokenize_text_unique_tokens() {
        let tokens = tokenize_text("rust rust rust programming rust");

        // HashSet - should contain only one "rust"
        let rust_count = tokens.iter().filter(|t| *t == "rust").count();
        assert_eq!(rust_count, 1);
    }

    #[test]
    fn test_tokenize_text_removes_punctuation() {
        let tokens = tokenize_text("Hello, World! How are you?");

        assert!(tokens.contains("hello"));
        assert!(tokens.contains("world"));
        assert!(tokens.contains("how"));
        assert!(tokens.contains("are"));
        assert!(tokens.contains("you"));

        // Punctuation should be removed
        assert!(!tokens.contains("hello,"));
        assert!(!tokens.contains("world!"));
    }

    #[test]
    fn test_tokenize_text_empty_string() {
        let tokens = tokenize_text("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_text_only_short_words() {
        let tokens = tokenize_text("I am a be to");
        assert!(tokens.is_empty(), "All words are <= 2 chars");
    }

    #[test]
    fn test_tokenize_text_numbers_ignored() {
        let tokens = tokenize_text("Rust 2024 Programming 123");

        assert!(tokens.contains("rust"));
        assert!(tokens.contains("programming"));

        // Numbers are not tokens (regex: [a-zA-Z]+)
        assert!(!tokens.contains("2024"));
        assert!(!tokens.contains("123"));
    }

    // ============================================================
    // TOKENIZER TESTS - tokenize_query
    // ============================================================

    #[test]
    fn test_tokenize_query_basic() {
        let tokens = tokenize_query("rust programming");

        assert_eq!(tokens.len(), 2);
        assert!(tokens.contains(&"rust".to_string()));
        assert!(tokens.contains(&"programming".to_string()));
    }

    #[test]
    fn test_tokenize_query_preserves_order() {
        let tokens = tokenize_query("first second third");

        // Vec preserves order
        assert_eq!(tokens[0], "first");
        assert_eq!(tokens[1], "second");
        assert_eq!(tokens[2], "third");
    }

    #[test]
    fn test_tokenize_query_filters_short() {
        let tokens = tokenize_query("a is the rust");

        // Only "rust" (and "the") are > 2 chars
        assert!(tokens.contains(&"rust".to_string()));
        assert!(tokens.contains(&"the".to_string()));
        assert!(!tokens.contains(&"a".to_string()));
        assert!(!tokens.contains(&"is".to_string()));
    }

    #[test]
    fn test_tokenize_query_trims_punctuation() {
        let tokens = tokenize_query("hello, world!");

        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
    }

    #[test]
    fn test_tokenize_query_empty() {
        let tokens = tokenize_query("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_query_allows_duplicates() {
        let tokens = tokenize_query("rust rust rust");

        // Vec can contain duplicates (unlike tokenize_text HashSet)
        assert_eq!(tokens.len(), 3);
    }

    // ============================================================
    // TYPES TESTS - BookMetadata
    // ============================================================

    #[test]
    fn test_book_metadata_serialization() {
        let book = BookMetadata {
            book_id: "book-123".to_string(),
            title: "The Rust Programming Language".to_string(),
            author: "Steve Klabnik".to_string(),
            language: "en".to_string(),
            year: Some(2019),
            word_count: 150000,
            unique_words: 8500,
        };

        let json = serde_json::to_string(&book).expect("Serialization failed");
        let restored: BookMetadata = serde_json::from_str(&json).expect("Deserialization failed");

        assert_eq!(restored.book_id, book.book_id);
        assert_eq!(restored.title, book.title);
        assert_eq!(restored.author, book.author);
        assert_eq!(restored.year, Some(2019));
        assert_eq!(restored.word_count, 150000);
    }

    #[test]
    fn test_book_metadata_optional_year() {
        let book = BookMetadata {
            book_id: "ancient-book".to_string(),
            title: "Unknown Manuscript".to_string(),
            author: "Anonymous".to_string(),
            language: "la".to_string(),
            year: None,
            word_count: 0,
            unique_words: 0,
        };

        let json = serde_json::to_string(&book).unwrap();
        let restored: BookMetadata = serde_json::from_str(&json).unwrap();

        assert!(restored.year.is_none());
    }

    // ============================================================
    // TYPES TESTS - SearchResultItem
    // ============================================================

    #[test]
    fn test_search_result_item_serialization() {
        let item = SearchResultItem {
            book_id: "result-001".to_string(),
            title: "Matching Book".to_string(),
            author: "Some Author".to_string(),
            score: 42,
        };

        let json = serde_json::to_string(&item).unwrap();
        let restored: SearchResultItem = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.score, 42);
        assert_eq!(restored.book_id, "result-001");
    }

    // ============================================================
    // TYPES TESTS - SearchResponse
    // ============================================================

    #[test]
    fn test_search_response_serialization() {
        let response = SearchResponse {
            query: "rust programming".to_string(),
            filters: HashMap::new(),
            total_count: 10,
            count: 2,
            results: vec![
                SearchResultItem {
                    book_id: "book-1".to_string(),
                    title: "Rust Book".to_string(),
                    author: "Author 1".to_string(),
                    score: 10,
                },
                SearchResultItem {
                    book_id: "book-2".to_string(),
                    title: "Programming Guide".to_string(),
                    author: "Author 2".to_string(),
                    score: 5,
                },
            ],
        };

        let json = serde_json::to_string(&response).unwrap();
        let restored: SearchResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.query, "rust programming");
        assert_eq!(restored.total_count, 10);
        assert_eq!(restored.count, 2);
        assert_eq!(restored.results.len(), 2);
        assert_eq!(restored.results[0].score, 10);
    }

    #[test]
    fn test_search_response_empty_results() {
        let response = SearchResponse {
            query: "nonexistent query".to_string(),
            filters: HashMap::new(),
            total_count: 0,
            count: 0,
            results: vec![],
        };

        let json = serde_json::to_string(&response).unwrap();
        let restored: SearchResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.total_count, 0);
        assert_eq!(restored.count, 0);
        assert!(restored.results.is_empty());
    }

    #[test]
    fn test_search_response_with_filters() {
        let mut filters = HashMap::new();
        filters.insert("language".to_string(), "en".to_string());
        filters.insert("year".to_string(), "2020".to_string());

        let response = SearchResponse {
            query: "test".to_string(),
            filters,
            total_count: 100,
            count: 0,
            results: vec![],
        };

        let json = serde_json::to_string(&response).unwrap();
        let restored: SearchResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.filters.get("language"), Some(&"en".to_string()));
        assert_eq!(restored.filters.get("year"), Some(&"2020".to_string()));
        assert_eq!(restored.total_count, 100);
    }

    #[test]
    fn test_search_response_pagination() {
        // Pagination test - total_count > count
        let response = SearchResponse {
            query: "rust".to_string(),
            filters: HashMap::new(),
            total_count: 100, // 100 results total
            count: 10,        // but only returning 10
            results: vec![],
        };

        assert!(response.total_count > response.count);
    }

    // ============================================================
    // SCORING LOGIC TESTS (simulating engine.rs)
    // ============================================================

    #[test]
    fn test_scoring_logic_count_matching_tokens() {
        // Simulating logic from engine.rs
        let query_tokens = tokenize_query("rust programming language");

        // Simulated index: word -> list of book_ids
        let mut index: HashMap<String, Vec<String>> = HashMap::new();
        index.insert(
            "rust".to_string(),
            vec!["book-1".to_string(), "book-2".to_string()],
        );
        index.insert("programming".to_string(), vec!["book-1".to_string()]);
        index.insert("language".to_string(), vec!["book-1".to_string()]);

        // Scoring logic
        let mut book_scores: HashMap<String, usize> = HashMap::new();
        for token in query_tokens.iter() {
            if let Some(book_ids) = index.get(token) {
                for book_id in book_ids {
                    *book_scores.entry(book_id.clone()).or_insert(0) += 1;
                }
            }
        }

        // book-1 matches all 3 tokens
        assert_eq!(book_scores.get("book-1"), Some(&3));

        // book-2 matches only "rust"
        assert_eq!(book_scores.get("book-2"), Some(&1));
    }

    #[test]
    fn test_scoring_no_matches() {
        let query_tokens = tokenize_query("xyz abc");

        let mut index: HashMap<String, Vec<String>> = HashMap::new();
        index.insert("rust".to_string(), vec!["book-1".to_string()]);

        let mut book_scores: HashMap<String, usize> = HashMap::new();
        for token in query_tokens.iter() {
            if let Some(book_ids) = index.get(token) {
                for book_id in book_ids {
                    *book_scores.entry(book_id.clone()).or_insert(0) += 1;
                }
            }
        }

        assert!(book_scores.is_empty(), "No books should match");
    }

    // ============================================================
    // EDGE CASES
    // ============================================================

    #[test]
    fn test_tokenize_text_special_characters() {
        let tokens = tokenize_text("C++ is not C# but Rust is great!");

        // Only alphabetic words
        assert!(tokens.contains("not"));
        assert!(tokens.contains("but"));
        assert!(tokens.contains("rust"));
        assert!(tokens.contains("great"));

        // C++ and C# fail the regex [a-zA-Z]+
        assert!(!tokens.contains("c++"));
        assert!(!tokens.contains("c#"));
    }

    #[test]
    fn test_tokenize_unicode() {
        // Regex [a-zA-Z]+ does not support unicode
        let tokens = tokenize_text("Książka о программировании");

        // Only ASCII passes the [a-zA-Z]+ regex
        // "Książka" splits into "Ksi" (before ą) and "ka" (after ą)
        // but "ka" has only 2 chars, so it's filtered

        // Check that full unicode words are NOT present
        assert!(!tokens.contains("książka"));
        assert!(!tokens.contains("программировании"));

        // Documenting limitation for non-ASCII text
        println!("Tokens from unicode text: {:?}", tokens);
    }
}