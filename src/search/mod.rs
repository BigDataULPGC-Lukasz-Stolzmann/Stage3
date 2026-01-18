//! Search Service Module
//!
//! The core component responsible for executing user queries against the distributed index.
//!
//! ## Responsibilities
//! - **Tokenization**: Parsing raw query strings into searchable tokens.
//! - **Ranking**: Scoring documents based on term matches.
//! - **Retrieval**: Fetching metadata for ranked document IDs.
//! - **API**: Exposing search capabilities via HTTP.

pub mod engine;
pub mod handlers;
pub mod tokenizer;
pub mod types;

#[cfg(test)]
mod tests;
