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

        // Wszystko powinno być lowercase
        assert!(tokens.contains("rust"));
        assert!(tokens.contains("programming"));
        assert!(tokens.contains("language"));

        // Uppercase nie powinno być
        assert!(!tokens.contains("RUST"));
        assert!(!tokens.contains("Programming"));
    }

    #[test]
    fn test_tokenize_text_filters_short_words() {
        let tokens = tokenize_text("I am a Rust programmer");

        // Słowa > 2 znaków
        assert!(tokens.contains("rust"));
        assert!(tokens.contains("programmer"));

        // Słowa <= 2 znaków są odfiltrowane
        assert!(!tokens.contains("i"));
        assert!(!tokens.contains("am"));
        assert!(!tokens.contains("a"));
    }

    #[test]
    fn test_tokenize_text_unique_tokens() {
        let tokens = tokenize_text("rust rust rust programming rust");

        // HashSet - powinien być tylko jeden "rust"
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

        // Nie powinno być znaków interpunkcyjnych
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
        assert!(tokens.is_empty(), "Wszystkie słowa <= 2 znaków");
    }

    #[test]
    fn test_tokenize_text_numbers_ignored() {
        let tokens = tokenize_text("Rust 2024 Programming 123");

        assert!(tokens.contains("rust"));
        assert!(tokens.contains("programming"));

        // Liczby nie są tokenami (regex: [a-zA-Z]+)
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

        // Vec zachowuje kolejność
        assert_eq!(tokens[0], "first");
        assert_eq!(tokens[1], "second");
        assert_eq!(tokens[2], "third");
    }

    #[test]
    fn test_tokenize_query_filters_short() {
        let tokens = tokenize_query("a is the rust");

        // Tylko "rust" ma > 2 znaki (oraz "the")
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

        // Vec może mieć duplikaty (w przeciwieństwie do tokenize_text)
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
        // Test dla paginacji - total_count > count
        let response = SearchResponse {
            query: "rust".to_string(),
            filters: HashMap::new(),
            total_count: 100, // 100 wyników łącznie
            count: 10,        // ale zwracamy tylko 10
            results: vec![],
        };

        assert!(response.total_count > response.count);
    }

    // ============================================================
    // SCORING LOGIC TESTS (symulacja engine.rs)
    // ============================================================

    #[test]
    fn test_scoring_logic_count_matching_tokens() {
        // Symulacja logiki z engine.rs
        let query_tokens = tokenize_query("rust programming language");

        // Symulacja indeksu: słowo -> lista book_ids
        let mut index: HashMap<String, Vec<String>> = HashMap::new();
        index.insert(
            "rust".to_string(),
            vec!["book-1".to_string(), "book-2".to_string()],
        );
        index.insert("programming".to_string(), vec!["book-1".to_string()]);
        index.insert("language".to_string(), vec!["book-1".to_string()]);

        // Logika scoringu
        let mut book_scores: HashMap<String, usize> = HashMap::new();
        for token in query_tokens.iter() {
            if let Some(book_ids) = index.get(token) {
                for book_id in book_ids {
                    *book_scores.entry(book_id.clone()).or_insert(0) += 1;
                }
            }
        }

        // book-1 ma wszystkie 3 tokeny
        assert_eq!(book_scores.get("book-1"), Some(&3));

        // book-2 ma tylko "rust"
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

        assert!(book_scores.is_empty(), "Żadna książka nie powinna pasować");
    }

    // ============================================================
    // EDGE CASES
    // ============================================================

    #[test]
    fn test_tokenize_text_special_characters() {
        let tokens = tokenize_text("C++ is not C# but Rust is great!");

        // Tylko słowa alfabetyczne
        assert!(tokens.contains("not"));
        assert!(tokens.contains("but"));
        assert!(tokens.contains("rust"));
        assert!(tokens.contains("great"));

        // C++ i C# nie przejdą przez regex [a-zA-Z]+
        assert!(!tokens.contains("c++"));
        assert!(!tokens.contains("c#"));
    }

    #[test]
    fn test_tokenize_unicode() {
        // Regex [a-zA-Z]+ nie obsługuje unicode
        let tokens = tokenize_text("Książka о программировании");

        // Tylko ASCII przechodzi przez regex [a-zA-Z]+
        // "Książka" dzieli się na "Ksi" (przed ą) i "ka" (po ą)
        // ale "ka" ma tylko 2 znaki, więc jest odfiltrowane

        // Sprawdzamy że nie ma pełnych słów z unicode
        assert!(!tokens.contains("książka"));
        assert!(!tokens.contains("программировании"));

        // Możliwe że tokeny są puste lub zawierają tylko fragmenty
        // To dokumentuje ograniczenie tokenizera dla nie-ASCII
        println!("Tokens from unicode text: {:?}", tokens);
    }
}
