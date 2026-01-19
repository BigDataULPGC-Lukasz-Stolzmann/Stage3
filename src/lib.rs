//! Distributed Search Engine Cluster Library
//!
//! This library crate defines the core modules that make up the distributed system.
//! It serves as the foundation for the binary executable (`main.rs`).
//!
//! ## Architecture Modules
//! The system is composed of five loosely coupled subsystems:
//!
//! - **`executor`**: The distributed task processing engine. It handles asynchronous jobs
//!   (like indexing) using a lease-based locking mechanism to ensure fault tolerance.
//! - **`ingestion`**: The data intake pipeline. Responsible for downloading content from
//!   external sources (Project Gutenberg), parsing metadata, and storing raw data.
//! - **`membership`**: The cluster coordination layer. Uses a UDP-based Gossip protocol
//!   (SWIM-like) to manage node discovery, failure detection, and cluster topology.
//! - **`search`**: The core information retrieval logic. Contains tokenizers, the scoring
//!   algorithm (TF), and query processing utilities.
//! - **`storage`**: The distributed state layer. Implements a sharded, replicated
//!   in-memory key-value store (`DistributedMap`) that handles data consistency.

pub mod executor;
pub mod ingestion;
pub mod membership;
pub mod search;
pub mod storage;