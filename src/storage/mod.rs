//! Distributed Storage Module
//!
//! Implements a sharded, replicated in-memory key-value store designed for high concurrency and fault tolerance.
//!
//! ## Core Architecture
//! The storage system is built around the "Partitioned Primary-Backup" model:
//! - **Partitioning**: The global keyspace is divided into fixed logical partitions (default: 256). Keys are mapped
//!   to partitions using a stable hash function.
//! - **Placement**: Partitions are assigned to nodes using a deterministic algorithm managed by the `PartitionManager`.
//!   This ensures every node calculates the same topology map without a central coordinator.
//! - **Replication**: Each partition has one **Primary** owner (responsible for coordinating writes) and $N$ **Backup**
//!   owners (responsible for durability).
//! - **Access**: The `DistributedMap` provides a unified interface (`put`, `get`) that transparently handles routing
//!   requests to the correct node (local or remote) and managing replication consistency.
//!
//! ## Submodules
//! - **`memory`**: The core `DistributedMap` implementation wrapping local `DashMap` storage.
//! - **`partitioner`**: Logic for consistent hashing and node assignment.
//! - **`protocol`**: Data Transfer Objects (DTOs) for inter-node storage operations.
//! - **`handlers`**: HTTP endpoints exposing storage operations to the cluster.

pub mod handlers;
pub mod memory;
pub mod partitioner;
pub mod protocol;

#[cfg(test)]
mod tests;