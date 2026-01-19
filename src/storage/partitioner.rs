//! Partition Manager
//!
//! Responsible for mapping keys to partitions and assigning partitions to specific nodes
//! in the cluster. It ensures a deterministic distribution of data.
//!
//! ## Mechanism
//! - **Hashing**: Keys are hashed to a `u32` and mapped to a partition ID using modulo arithmetic.
//! - **Placement**: Partitions are assigned to nodes based on a sorted list of active members.
//!   This ensures that all nodes calculate the exact same ownership map independently (Shared-Nothing).

use crate::membership::{service::MembershipService, types::NodeId};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Manages the topology of data storage.
pub struct PartitionManager {
    /// Total number of logical partitions (shards) in the cluster.
    pub num_partitions: u32,
    /// Desired number of copies for each datum (Primary + Backups).
    replication_factor: usize,
    /// Reference to the membership service to track alive nodes.
    membership: Arc<MembershipService>,
}

impl PartitionManager {
    /// Creates a new PartitionManager with default settings (256 partitions, Replication Factor = 2).
    pub fn new(membership: Arc<MembershipService>) -> Arc<Self> {
        Self::new_with_replication(membership, 2)
    }

    /// Creates a new PartitionManager with custom replication settings.
    pub fn new_with_replication(
        membership: Arc<MembershipService>,
        replication_factor: usize,
    ) -> Arc<Self> {
        Arc::new(Self {
            num_partitions: 256,
            replication_factor: replication_factor.max(1),
            membership,
        })
    }

    /// Maps a string key to a specific partition ID.
    ///
    /// Uses the standard Rust `DefaultHasher` to ensure consistent mapping across restarts.
    pub fn get_partition(&self, key: &str) -> u32 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish() as u32;
        hash % self.num_partitions
    }

    /// Calculates the list of nodes responsible for a specific partition.
    ///
    /// The algorithm provides stability: if a node fails, the next node in the sorted list
    /// takes over responsibility.
    ///
    /// # Return Value
    /// A vector of `NodeId`s where:
    /// - **Index 0**: Primary Owner (handles writes).
    /// - **Indices 1+**: Backup Owners (passive replicas).
    pub fn get_owners(&self, partition: u32) -> Vec<NodeId> {
        let alive_nodes = self.membership.get_alive_members();
        if alive_nodes.is_empty() {
            return vec![];
        }
        
        // CRITICAL: Sort by ID to ensure identical results on all nodes regardless of discovery order.
        let mut node_ids: Vec<NodeId> = alive_nodes.into_iter().map(|node| node.id).collect();
        node_ids.sort_by(|a, b| a.0.cmp(&b.0));

        // Use consistent hashing logic (partition % node_count) to find the primary
        let primary_idx = (partition as usize) % node_ids.len();
        
        // Select neighbors in the ring as replicas
        let replica_count = self.replication_factor.min(node_ids.len());

        (0..replica_count)
            .map(|offset| node_ids[(primary_idx + offset) % node_ids.len()].clone())
            .collect()
    }

    /// Identifies all partitions for which the local node acts as the Primary.
    ///
    /// Used by the synchronization loop to verify that this node holds the correct data
    /// for its assigned responsibilities.
    pub fn my_primary_partitions(&self) -> Vec<u32> {
        let my_id = &self.membership.local_node.id;

        (0..self.num_partitions)
            .filter(|&partition| {
                let owners = self.get_owners(partition);
                !owners.is_empty() && &owners[0] == my_id
            })
            .collect()
    }

    /// Identifies all partitions for which the local node acts as a Backup.
    ///
    /// Used by the synchronization loop to pull data from primaries to ensure redundancy.
    pub fn my_backup_partitions(&self) -> Vec<u32> {
        let my_id = &self.membership.local_node.id;

        (0..self.num_partitions)
            .filter(|&partition| {
                let owners = self.get_owners(partition);
                owners.iter().skip(1).any(|owner| owner == my_id)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[tokio::test]
    async fn test_partition_deterministic() {
        let bind_addr: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        let membership = MembershipService::new(bind_addr, vec![]).await.unwrap();

        let mamanger = PartitionManager::new(membership);

        let p1 = mamanger.get_partition("book_100");
        let p2 = mamanger.get_partition("book_100");
        assert_eq!(p1, p2);

        assert!(p1 < 256);

        println!("book_100 -> partition {}", p1);
    }
}