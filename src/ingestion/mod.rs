//! Ingestion Service Module
//!
//! Handles the acquisition, preprocessing, and storage of documents (eBooks) from external sources.
//!
//! ## Architecture Workflow
//! As described in the system architecture:
//! 1. **Download**: Fetches raw text directly from Project Gutenberg's cache.
//! 2. **Process**: Heuristically splits content into metadata headers and body text to strip legal boilerplate.
//! 3. **Storage**: Persists the `RawDocument` into the distributed **Datalake** (sharded & replicated).
//! 4. **Coordination**: Offloads CPU-intensive indexing by submitting an asynchronous task to the `DistributedQueue`.
//!
//! This design prevents the ingestion API from blocking during tokenization and ensures better
//! system responsiveness under load.

pub mod handlers;
pub mod types;