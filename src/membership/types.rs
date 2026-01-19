use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Instant;

/// Unique identifier for a node in the cluster.
/// Wrapper around a UUID string to ensure global uniqueness across restarts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct NodeId(pub String);

impl NodeId {
    /// Generates a new random UUID v4-based NodeId.
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// Represents the lifecycle state of a node from the perspective of the local failure detector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeState {
    /// The node is healthy and responsive.
    Alive,
    /// The node has missed the heartbeat window and is suspected to be down.
    /// It can transition back to `Alive` if it refutes the suspicion, or to `Dead` if it times out.
    Suspect,
    /// The node is confirmed failed and is effectively removed from the cluster view.
    Dead,
}

/// Represents a single member in the cluster.
///
/// Contains identity, network addressing, and current lifecycle state.
/// The `incarnation` field is a logical clock used to order updates and resolve conflicts
/// (e.g., refuting a false "Suspect" claim).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Unique ID of the node.
    pub id: NodeId,
    /// UDP address used for the Gossip protocol.
    pub gossip_addr: SocketAddr,
    /// TCP address used for the HTTP API (external and internal communication).
    pub http_addr: SocketAddr,
    /// Current health state (Alive/Suspect/Dead).
    pub state: NodeState,
    /// Logical clock for versioning the node's state. Higher numbers take precedence.
    pub incarnation: u64,

    /// Local timestamp of when this node was last heard from.
    /// Not serialized over the network; used only by the local failure detector.
    #[serde(skip)]
    pub last_seen: Option<Instant>,
}

/// The wire protocol for inter-node communication via UDP.
///
/// Implements the messages required for the SWIM-style membership protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    /// A direct health check sent to a peer.
    /// Also carries the sender's current incarnation number for state dissemination.
    Ping {
        from: NodeId,
        incarnation: u64,
    },

    /// Response to a Ping, confirming the target is alive.
    /// Can optionally carry a partial list of other members to speed up convergence (gossip).
    Ack {
        from: NodeId,
        incarnation: u64,
        members: Vec<Node>,
    },

    /// Request sent by a new node to a seed node to enter the cluster.
    Join {
        node: Node,
    },

    /// Message broadcasting that a specific node is suspected to be dead.
    /// This triggers the "Suspect" state transition on receiving nodes.
    Suspect {
        node_id: NodeId,
        incarnation: u64,
    },

    /// Message broadcasting that a node is alive.
    /// Often used to refute a "Suspect" message (Self-Defense).
    Alive {
        node_id: NodeId,
        incarnation: u64,
    },
}