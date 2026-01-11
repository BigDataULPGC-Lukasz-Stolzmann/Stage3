use crate::membership::{service::MembershipService, types::NodeId};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub struct PartitionManager {
    num_partitions: u32,
    replication_factor: usize,
    membership: Arc<MembershipService>,
}

impl PartitionManager {
    pub fn new(membership: Arc<MembershipService>) -> Self {
        Self {
            num_partitions: 256,
            replication_factor: 1,
            membership,
        }
    }

    pub fn get_partition(&self, key: &str) -> u32 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish() as u32;
        hash % self.num_partitions
    }

    pub fn get_owners(&self, partition: u32) -> Vec<NodeId> {
        let alive_nodes = self.membership.get_alive_members();
        if alive_nodes.is_empty() {
            return vec![];
        }
        let mut node_ids: Vec<NodeId> = alive_nodes.into_iter().map(|node| node.id).collect();
        node_ids.sort_by(|a, b| a.0.cmp(&b.0));
        let primary_idx = (partition as usize) % node_ids.len();
        let backup_idx = (partition as usize + 1) % node_ids.len();
        vec![node_ids[primary_idx].clone(), node_ids[backup_idx].clone()]
    }

    pub fn my_primary_partitions(&self) -> Vec<u32> {
        let my_id = &self.membership.local_node.id;

        (0..self.num_partitions)
            .filter(|&partition| {
                let owners = self.get_owners(partition);
                !owners.is_empty() && &owners[0] == my_id
            })
            .collect()
    }

    pub fn my_backup_partitions(&self) -> Vec<u32> {
        let my_id = &self.membership.local_node.id;

        (0..self.num_partitions)
            .filter(|&partition| {
                let owners = self.get_owners(partition);
                owners.len() > 1 && &owners[1] == my_id
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
