use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeState {
    Alive,
    Suspect,
    Dead,
}

/// Represents a single member in the cluster.
///
/// Contains identity, network addressing, and current lifecycle state.
/// The `incarnation` field is a logical clock used to order updates and resolve conflicts
/// (e.g., refuting a false "Suspect" claim).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub gossip_addr: SocketAddr,
    pub http_addr: SocketAddr,
    pub state: NodeState,
    pub incarnation: u64,

    #[serde(skip)]
    pub last_seen: Option<Instant>,
}

/// The wire protocol for inter-node communication.
///
/// - `Ping/Ack`: Used for liveness checks and state synchronization.
/// - `Join`: Sent by new nodes to seed nodes to enter the cluster.
/// - `Suspect/Alive`: Disseminates changes in node health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    Ping {
        from: NodeId,
        incarnation: u64,
    },

    Ack {
        from: NodeId,
        incarnation: u64,
        members: Vec<Node>,
    },

    Join {
        node: Node,
    },

    Suspect {
        node_id: NodeId,
        incarnation: u64,
    },

    Alive {
        node_id: NodeId,
        incarnation: u64,
    },
}
