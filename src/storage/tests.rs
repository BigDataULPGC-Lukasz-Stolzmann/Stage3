#[cfg(test)]
mod tests {
    use crate::membership::service::MembershipService;
    use crate::storage::memory::DistributedMap;
    use crate::storage::partitioner::PartitionManager;
    use serde::{Deserialize, Serialize};
    use std::net::SocketAddr;

    // Test data structure
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestBook {
        id: String,
        title: String,
        author: String,
    }

    // ============================================================
    // PARTITIONER TESTS
    // ============================================================

    #[tokio::test]
    async fn test_partition_is_deterministic() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership);

        // Ten sam klucz -> ta sama partycja
        let p1 = partitioner.get_partition("book_100");
        let p2 = partitioner.get_partition("book_100");
        assert_eq!(p1, p2, "Ta sama wartość powinna dać tę samą partycję");
    }

    #[tokio::test]
    async fn test_partition_is_within_range() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership);

        // Testuj wiele kluczy
        for i in 0..1000 {
            let key = format!("test_key_{}", i);
            let partition = partitioner.get_partition(&key);
            assert!(
                partition < partitioner.num_partitions,
                "Partycja {} powinna być < {}",
                partition,
                partitioner.num_partitions
            );
        }
    }

    #[tokio::test]
    async fn test_partition_distribution() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership);

        // Sprawdź rozkład partycji (czy nie wszystkie trafiają do jednej)
        let mut partition_counts = std::collections::HashMap::new();

        for i in 0..10000 {
            let key = format!("book_{}", i);
            let partition = partitioner.get_partition(&key);
            *partition_counts.entry(partition).or_insert(0) += 1;
        }

        // Przy 256 partycjach i 10000 kluczach, każda powinna mieć ~39 kluczy
        // Sprawdzamy czy mamy przynajmniej 100 różnych partycji (rozsądny rozkład)
        assert!(
            partition_counts.len() > 100,
            "Powinno być więcej niż 100 różnych partycji, jest: {}",
            partition_counts.len()
        );
    }

    #[tokio::test]
    async fn test_get_owners_returns_primary_and_backup() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership);

        let owners = partitioner.get_owners(0);

        // Z jednym nodem dostajemy tylko primary (replication_factor ogranicza sie do liczby nodow)
        assert_eq!(owners.len(), 1);
    }

    #[tokio::test]
    async fn test_my_primary_partitions() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership);

        let my_partitions = partitioner.my_primary_partitions();

        // Z jednym nodem, wszystkie partycje należą do nas
        assert_eq!(
            my_partitions.len() as u32,
            partitioner.num_partitions,
            "Pojedynczy node powinien być primary dla wszystkich partycji"
        );
    }

    // ============================================================
    // DISTRIBUTED MAP TESTS (lokalne operacje)
    // ============================================================

    #[tokio::test]
    async fn test_distributed_map_store_and_get_local() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership.clone());

        let map: DistributedMap<String, TestBook> =
            DistributedMap::new(membership, partitioner.clone());

        let book = TestBook {
            id: "book-001".to_string(),
            title: "Rust Programming".to_string(),
            author: "Steve".to_string(),
        };

        // Store locally
        let partition = partitioner.get_partition("book-001");
        map.store_local(partition, "book-001".to_string(), book.clone());

        // Get locally
        let retrieved = map.get_local(&"book-001".to_string());

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), book);
    }

    #[tokio::test]
    async fn test_distributed_map_get_nonexistent_key() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership.clone());

        let map: DistributedMap<String, TestBook> = DistributedMap::new(membership, partitioner);

        let result = map.get_local(&"nonexistent".to_string());
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_distributed_map_overwrite_value() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership.clone());

        let map: DistributedMap<String, TestBook> =
            DistributedMap::new(membership, partitioner.clone());

        let book1 = TestBook {
            id: "book-001".to_string(),
            title: "Original Title".to_string(),
            author: "Author 1".to_string(),
        };

        let book2 = TestBook {
            id: "book-001".to_string(),
            title: "Updated Title".to_string(),
            author: "Author 2".to_string(),
        };

        let partition = partitioner.get_partition("book-001");

        // Store first version
        map.store_local(partition, "book-001".to_string(), book1);

        // Overwrite with second version
        map.store_local(partition, "book-001".to_string(), book2.clone());

        // Should get updated version
        let retrieved = map.get_local(&"book-001".to_string());
        assert_eq!(retrieved.unwrap().title, "Updated Title");
    }

    #[tokio::test]
    async fn test_distributed_map_multiple_keys() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership.clone());

        let map: DistributedMap<String, TestBook> =
            DistributedMap::new(membership, partitioner.clone());

        // Store multiple books
        for i in 0..100 {
            let key = format!("book-{:03}", i);
            let book = TestBook {
                id: key.clone(),
                title: format!("Title {}", i),
                author: format!("Author {}", i),
            };

            let partition = partitioner.get_partition(&key);
            map.store_local(partition, key, book);
        }

        // Verify all can be retrieved
        for i in 0..100 {
            let key = format!("book-{:03}", i);
            let retrieved = map.get_local(&key);
            assert!(retrieved.is_some(), "Book {} should exist", key);
            assert_eq!(retrieved.unwrap().title, format!("Title {}", i));
        }
    }

    // UWAGA: Testy dla put() i put_local() wymagają działającego HTTP serwera
    // bo próbują replikować do backup node przez HTTP.
    // Te operacje są testowane w integration testach z działającym klastrem.
    // Unit testy używają tylko store_local() i get_local().

    #[tokio::test]
    async fn test_distributed_map_store_get_roundtrip() {
        // Test kompletnego cyklu store_local -> get_local
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership.clone());

        let map: DistributedMap<String, TestBook> =
            DistributedMap::new(membership, partitioner.clone());

        let book = TestBook {
            id: "roundtrip-book".to_string(),
            title: "Roundtrip Test".to_string(),
            author: "Test Author".to_string(),
        };

        let partition = partitioner.get_partition("roundtrip-book");
        map.store_local(partition, "roundtrip-book".to_string(), book.clone());

        let retrieved = map.get_local(&"roundtrip-book".to_string());
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), book);
    }

    // ============================================================
    // RÓŻNE TYPY KLUCZY I WARTOŚCI
    // ============================================================

    #[tokio::test]
    async fn test_distributed_map_with_vec_value() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership.clone());

        // Taki sam typ jak index_map w main.rs
        let map: DistributedMap<String, Vec<String>> =
            DistributedMap::new(membership, partitioner.clone());

        let book_ids = vec![
            "book-001".to_string(),
            "book-002".to_string(),
            "book-003".to_string(),
        ];

        let partition = partitioner.get_partition("rust");
        map.store_local(partition, "rust".to_string(), book_ids.clone());

        let retrieved = map.get_local(&"rust".to_string());
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), book_ids);
    }

    #[tokio::test]
    async fn test_distributed_map_append_to_vec() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership.clone());

        let map: DistributedMap<String, Vec<String>> =
            DistributedMap::new(membership, partitioner.clone());

        let partition = partitioner.get_partition("programming");

        // Initial empty
        map.store_local(partition, "programming".to_string(), vec![]);

        // Symulacja dodawania książek (jak w index_book handler)
        let mut books = map
            .get_local(&"programming".to_string())
            .unwrap_or_default();
        books.push("book-001".to_string());
        map.store_local(partition, "programming".to_string(), books);

        let mut books = map
            .get_local(&"programming".to_string())
            .unwrap_or_default();
        books.push("book-002".to_string());
        map.store_local(partition, "programming".to_string(), books);

        // Verify
        let final_books = map.get_local(&"programming".to_string()).unwrap();
        assert_eq!(final_books.len(), 2);
        assert!(final_books.contains(&"book-001".to_string()));
        assert!(final_books.contains(&"book-002".to_string()));
    }
}
