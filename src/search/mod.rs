//! Search Service Module
//!
//! The core component responsible for executing user queries against the distributed index.
//!
//! ## Overview
//! This module implements the Information Retrieval (IR) pipeline for the search engine.
//! It bridges the HTTP API layer with the underlying distributed storage systems
//! (Inverted Index and Metadata Store).
//!
//! ## Responsibilities
//! - **Tokenization**: Parsing raw query strings and document text into normalized, searchable tokens.
//! - **Ranking**: Scoring documents based on term matches using a relevance algorithm.
//! - **Retrieval**: Hydrating ranked document IDs with full metadata (Title, Author, etc.).
//! - **API**: Exposing search capabilities via RESTful HTTP endpoints.
//!
//! ## Submodules
//! - **`engine`**: Contains the core ranking and retrieval logic.
//! - **`handlers`**: HTTP request handlers for the Axum web server.
//! - **`tokenizer`**: Text processing utilities (normalization, stemming, filtering).
//! - **`types`**: Data Transfer Objects (DTOs) for API communication.

pub mod engine;
pub mod handlers;
pub mod tokenizer;
pub mod types;

#[cfg(test)]
mod tests;