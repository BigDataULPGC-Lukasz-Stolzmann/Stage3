//! Distributed Storage Module
//!
//! Implements a sharded, replicated in-memory key-value store.
//!
//! ## Core Concepts
//! - **Partitioning**: Data is divided into fixed partitions (shards) based on key hashing.
//! - **Placement**: `PartitionManager` assigns partitions to nodes (Primary + Backups).
//! - **Replication**: Writes are coordinated by the Primary node and pushed to Backups for fault tolerance.
//! - **Access**: `DistributedMap` acts as a smart client, handling routing (local vs remote) transparently.

pub mod handlers;
pub mod memory;
pub mod partitioner;
pub mod protocol;

#[cfg(test)]
mod tests;
