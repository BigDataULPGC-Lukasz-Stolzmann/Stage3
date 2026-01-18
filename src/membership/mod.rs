//! Membership & Discovery Module
//!
//! Implements a Gossip-based membership protocol (inspired by SWIM) to manage the cluster topology.
//! Nodes use this service to discover each other, detect failures, and disseminate cluster state updates.
//!
//! ## Core Mechanisms
//! - **Gossip Protocol**: Nodes periodically exchange status updates via UDP to maintain a consistent view of the cluster.
//! - **Failure Detection**: Uses a "Suspect" -> "Dead" transition model with timeouts to handle node crashes gracefully.
//! - **Incarnation Numbers**: Solves conflict resolution when node state (Alive/Suspect) is disputed.

pub mod service;
pub mod types;

#[cfg(test)]
mod tests;
