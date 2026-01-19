//! Membership & Discovery Module
//!
//! Implements a Gossip-based membership protocol (inspired by SWIM) to manage the cluster topology.
//! Nodes use this service to discover each other, detect failures, and disseminate cluster state updates.
//!
//! ## Core Mechanisms
//! - **Gossip Protocol**: Nodes periodically exchange status updates via UDP to maintain a consistent view of the cluster.
//!   Information spreads epidemically through the system with O(log N) convergence.
//! - **Failure Detection**: Uses a "Suspect" -> "Dead" transition model with timeouts to handle node crashes gracefully
//!   and avoid false positives due to transient network issues.
//! - **Incarnation Numbers**: Solves conflict resolution when node state (Alive/Suspect) is disputed.
//!   This acts as a logical clock to determine the most recent version of a node's state.

pub mod service;
pub mod types;

#[cfg(test)]
mod tests;