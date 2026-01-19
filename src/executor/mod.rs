//! Distributed Task Executor Module
//!
//! This module implements a distributed, fault-tolerant task execution engine designed
//! to handle ingestion, indexing, and other asynchronous background jobs across the cluster.
//!
//! ## Architecture Overview
//! The executor follows a **Pull-based** model with **Lease** management:
//! 1. **Submission**: Tasks are submitted to the `DistributedQueue`. The system determines the
//!    partition owner (Primary) using consistent hashing.
//! 2. **Persistence**: The Primary node stores the task and replicates it to Backup nodes
//!    for fault tolerance (Primary-Backup Replication).
//! 3. **Execution**: Worker threads on the Primary node poll their local partitions for `Pending` tasks.
//! 4. **Leasing**: To execute a task, a worker "claims" it by setting a lease expiration.
//!    If the worker dies, the lease expires, allowing another worker to retry the task (at-least-once semantics).
//!
//! ## Submodules
//! - **`queue`**: Core distributed data structure managing task state, sharding, and replication.
//! - **`executor`**: Manages the worker thread pool and the execution lifecycle (claim -> run -> complete).
//! - **`registry`**: Maps string identifiers (e.g., "index_document") to executable Rust code.
//! - **`protocol`**: Defines the HTTP API contracts for inter-node communication.

pub mod types;
pub mod protocol;
pub mod queue;
pub mod handlers;
pub mod executor;
pub mod registry;

#[cfg(test)]
mod tests;