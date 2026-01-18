//! Storage Module Tests
//!
//! Validates the data distribution logic and local storage mechanics.
//!
//! ## Test Scopes
//! - **Partitioner**: Ensures deterministic hashing and fair distribution of keys.
//! - **DistributedMap**: Verifies local storage operations (Put/Get) and data persistence.
//!
//! *Note: Network-dependent operations (replication, forwarding) are tested in integration tests.*

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

        // Same key -> same partition
        let p1 = partitioner.get_partition("book_100");
        let p2 = partitioner.get_partition("book_100");
        assert_eq!(p1, p2, "The same value should yield the same partition");
    }

    #[tokio::test]
    async fn test_partition_is_within_range() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership);

        // Test multiple keys
        for i in 0..1000 {
            let key = format!("test_key_{}", i);
            let partition = partitioner.get_partition(&key);
            assert!(
                partition < partitioner.num_partitions,
                "Partition {} should be < {}",
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

        // Check partition distribution (ensure not all keys go to one bucket)
        let mut partition_counts = std::collections::HashMap::new();

        for i in 0..10000 {
            let key = format!("book_{}", i);
            let partition = partitioner.get_partition(&key);
            *partition_counts.entry(partition).or_insert(0) += 1;
        }

        // With 256 partitions and 10000 keys, each should have ~39 keys.
        // We check if we have at least 100 used partitions (reasonable distribution).
        assert!(
            partition_counts.len() > 100,
            "Should have more than 100 distinct partitions used, got: {}",
            partition_counts.len()
        );
    }

    #[tokio::test]
    async fn test_get_owners_returns_primary_and_backup() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership);

        let owners = partitioner.get_owners(0);

        // With one node, we only get the primary (replication_factor is capped by node count)
        assert_eq!(owners.len(), 1);
    }

    #[tokio::test]
    async fn test_my_primary_partitions() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership);

        let my_partitions = partitioner.my_primary_partitions();

        // With one node, it owns all partitions
        assert_eq!(
            my_partitions.len() as u32,
            partitioner.num_partitions,
            "Single node should be primary for all partitions"
        );
    }

    // ============================================================
    // DISTRIBUTED MAP TESTS (Local Operations)
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

    // NOTE: Tests for put() and put_local() require a running HTTP server
    // because they attempt to replicate to backup nodes via HTTP.
    // These operations are covered in integration tests with a running cluster.
    // Unit tests here primarily focus on store_local() and get_local().

    #[tokio::test]
    async fn test_distributed_map_store_get_roundtrip() {
        // Test complete cycle: store_local -> get_local
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
    // VARIOUS KEY AND VALUE TYPES
    // ============================================================

    #[tokio::test]
    async fn test_distributed_map_with_vec_value() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();
        let partitioner = PartitionManager::new(membership.clone());

        // Same type as index_map in main.rs
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

        // Simulate adding books (like in index_book handler)
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