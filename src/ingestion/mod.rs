//! Ingestion Service Module
//!
//! Handles the acquisition, preprocessing, and storage of documents (eBook) from external sources.
//!
//! ## Workflow
//! 1. **Download**: Fetches raw text from Project Gutenberg.
//! 2. **Process**: Splits content into metadata headers and body text.
//! 3. **Storage**: Saves the raw document into the distributed "Datalake".
//! 4. **Coordination**: Submits an indexing task to the `DistributedQueue` for further processing.

pub mod handlers;
pub mod types;
