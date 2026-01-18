//! Distributed Task Executor Module
//!
//! This module implements a distributed, fault-tolerant task execution engine designed
//! to handle ingestion, indexing, and other asynchronous background jobs across the cluster.
//!
//! ## Core Components
//! - **DistributedQueue**: Manages task distribution, partitioning (sharding), and replication.
//! - **TaskExecutor**: A worker pool that pulls tasks from the local queue and executes them.
//! - **Registry**: Maps task names (strings) to actual Rust closures/handlers.
//! - **Protocol**: Defines the HTTP communication contract for task forwarding and replication.

pub mod types;
pub mod protocol;
pub mod queue;
pub mod handlers;
pub mod executor;
pub mod registry;

#[cfg(test)]
mod tests;
