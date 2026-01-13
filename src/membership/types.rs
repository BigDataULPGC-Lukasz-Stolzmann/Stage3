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
