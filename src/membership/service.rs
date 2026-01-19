//! Membership Service Implementation
//!
//! Manages the lifecycle of the local node and maintains the `members` list (the local view of the cluster).
//! Runs three concurrent background loops:
//! 1. **Gossip Loop**: Randomly probes other nodes to disseminate state.
//! 2. **Receive Loop**: Handles incoming UDP messages.
//! 3. **Failure Detection**: Monitors timestamps to transition nodes from Suspect to Dead.

use anyhow::Result;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;
use std::{net::SocketAddr, time::Duration};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tracing::info;

use super::types::{GossipMessage, Node, NodeId, NodeState};

// Configuration constants for the protocol timing.
const GOSSIP_INTERVAL: Duration = Duration::from_millis(500);
const FAILURE_DETECTION_INTERVAL: Duration = Duration::from_secs(2);
const SUSPECT_TIMEOUT: Duration = Duration::from_secs(5);
const DEAD_TIMEOUT: Duration = Duration::from_secs(10);

/// The main service struct responsible for cluster membership logic.
pub struct MembershipService {
    /// Information about the current running node.
    pub local_node: Node,
    /// Thread-safe map of all known members in the cluster.
    pub members: Arc<DashMap<NodeId, Node>>,
    /// UDP socket for sending and receiving gossip messages.
    socket: Arc<UdpSocket>,
    /// The local incarnation number, protected by a read-write lock.
    /// Incremented when this node needs to refute a suspicion about itself.
    incarnation: Arc<RwLock<u64>>,
}

impl MembershipService {
    /// Initializes the membership service.
    ///
    /// - Binds to the specified UDP address.
    /// - Initializes the local node state with incarnation 1.
    /// - If `seed_nodes` are provided, it immediately attempts to `Join` the existing cluster
    ///   by sending a `Join` message to each seed.
    pub async fn new(bind_addr: SocketAddr, seed_nodes: Vec<SocketAddr>) -> Result<Arc<Self>> {
        let socket = UdpSocket::bind(bind_addr).await?;
        let incarnation_counter = Arc::new(RwLock::new(1));
        let current_inc = *incarnation_counter.read().await;
        
        // Construct the local node definition.
        // Note: HTTP port is conventionally derived as gossip_port + 1000.
        let local_node = Node {
            id: NodeId::new(),
            gossip_addr: bind_addr,
            http_addr: SocketAddr::new(bind_addr.ip(), bind_addr.port() + 1000),
            state: NodeState::Alive,
            incarnation: current_inc,
            last_seen: Some(Instant::now()),
        };
        
        let members = Arc::new(DashMap::new());
        members.insert(local_node.id.clone(), local_node.clone());
        
        // Bootstrap phase: Contact seed nodes if any.
        if !seed_nodes.is_empty() {
            info!("Joining cluster via {} seed node(s)", seed_nodes.len());

            for seed_node in seed_nodes.iter() {
                let msg = GossipMessage::Join {
                    node: local_node.clone(),
                };

                let encoded = bincode::serialize(&msg)?;
                socket.send_to(&encoded, seed_node).await?;
                info!("Sent join request to {}", seed_node);
            }
        }

        Ok(Arc::new(Self {
            local_node,
            members,
            socket: Arc::new(socket),
            incarnation: incarnation_counter,
        }))
    }

    /// Starts the background tasks for the protocol.
    ///
    /// Spawns three independent Tokio tasks:
    /// 1. `gossip_loop`: Sends periodic heartbeats.
    /// 2. `receive_loop`: Listens for incoming UDP packets.
    /// 3. `failure_detection_loop`: Checks for timed-out nodes.
    pub async fn start(self: Arc<Self>) {
        tracing::info!("Starting membership service...");

        let _gossip_handle = {
            let service = self.clone();
            tokio::spawn(async move {
                service.gossip_loop().await;
            })
        };

        let _receive_handle = {
            let service = self.clone();
            tokio::spawn(async move {
                service.receive_loop().await;
            })
        };

        let _failure_detection_handle = {
            let service = self.clone();
            tokio::spawn(async move {
                service.failure_detection_loop().await;
            })
        };

        tracing::info!("All background tasks started");
    }

    /// Retrieves a copy of a member's state by its ID.
    pub fn get_member(&self, node_id: &NodeId) -> Option<Node> {
        self.members.get(node_id).map(|entry| entry.clone())
    }

    /// Returns a list of all members currently considered `Alive`.
    /// Useful for other services (like Storage or Executor) to know available peers.
    pub fn get_alive_members(&self) -> Vec<Node> {
        self.members
            .iter()
            .filter(|entry| entry.value().state == NodeState::Alive)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// The heartbeat of the protocol.
    ///
    /// Periodically selects a random "Alive" peer and sends a `Ping`.
    /// This randomized probing ensures that information (state changes) spreads epidemically
    /// through the cluster (gossip) with O(log N) convergence.
    async fn gossip_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(GOSSIP_INTERVAL);

        loop {
            interval.tick().await;

            // Filter for valid targets (not self, not dead)
            let alive_members: Vec<Node> = self
                .members
                .iter()
                .filter(|entry| {
                    entry.value().id != self.local_node.id
                        && entry.value().state == NodeState::Alive
                })
                .map(|entry| entry.value().clone())
                .collect();

            if alive_members.is_empty() {
                continue;
            }

            // Pick a random target to ping
            use rand::Rng;
            let idx = rand::thread_rng().gen_range(0..alive_members.len());
            let target = &alive_members[idx];

            let incarnation = *self.incarnation.read().await;
            let msg = GossipMessage::Ping {
                from: self.local_node.id.clone(),
                incarnation,
            };

            if let Ok(encoded) = bincode::serialize(&msg) {
                if let Err(e) = self.socket.send_to(&encoded, target.gossip_addr).await {
                    tracing::warn!("Failed to send ping to {:?}: {}", target.id, e);
                } else {
                    tracing::debug!("Sent ping to {:?}", target.id);
                }
            } else {
                tracing::error!("Failed to serialize GossipMessage::Ping");
            }
        }
    }

    /// Continuous loop that listens for incoming UDP messages.
    /// Deserializes the binary payload and dispatches to `handle_message`.
    async fn receive_loop(self: Arc<Self>) {
        let mut buf = vec![0u8; 65536]; // 64KB buffer

        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, src)) => match bincode::deserialize::<GossipMessage>(&buf[..len]) {
                    Ok(msg) => {
                        if let Err(e) = self.handle_message(msg, src).await {
                            tracing::error!("Error handling message from {}: {}", src, e);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to deserialize message from {}: {}", src, e);
                    }
                },
                Err(e) => {
                    tracing::error!("Failed to receive UDP packet: {}", e);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Dispatches incoming messages to their specific handlers.
    async fn handle_message(&self, msg: GossipMessage, src: SocketAddr) -> Result<()> {
        match msg {
            GossipMessage::Ping { from, incarnation } => {
                self.handle_ping(from, incarnation, src).await?;
            }

            GossipMessage::Ack {
                from,
                incarnation,
                members,
            } => {
                self.handle_ack(from, incarnation, members).await?;
            }

            GossipMessage::Join { node } => {
                self.handle_join(node).await?;
            }

            GossipMessage::Suspect {
                node_id,
                incarnation,
            } => {
                self.handle_suspect(node_id, incarnation).await?;
            }

            GossipMessage::Alive {
                node_id,
                incarnation,
            } => {
                self.handle_alive(node_id, incarnation).await?;
            }
        }

        Ok(())
    }

    /// Handles a `Ping` message.
    ///
    /// 1. Updates the sender's state in the local member list (incarnation check).
    /// 2. If the sender is unknown, adds it to the cluster view.
    /// 3. Responds immediately with an `Ack` containing a partial list of members.
    async fn handle_ping(
        &self,
        from: NodeId,
        from_incarnation: u64,
        src: SocketAddr,
    ) -> Result<()> {
        tracing::debug!("Received ping from {:?}", from);

        if let Some(mut member) = self.members.get_mut(&from) {
            member.last_seen = Some(Instant::now());

            // Update incarnation if the sender has a newer version
            if from_incarnation > member.incarnation {
                member.incarnation = from_incarnation;
            }
        } else {
            // New node discovery
            tracing::info!("Discovered new member via ping: {:?} at {}", from, src);

            let new_node = Node {
                id: from.clone(),
                gossip_addr: src,
                http_addr: SocketAddr::new(src.ip(), src.port() + 1000),
                state: NodeState::Alive,
                incarnation: from_incarnation,
                last_seen: Some(Instant::now()),
            };

            self.members.insert(new_node.id.clone(), new_node);
        }

        // Prepare Ack response
        let all_members: Vec<Node> = self
            .members
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        let my_incarnation = *self.incarnation.read().await;
        let reply = GossipMessage::Ack {
            from: self.local_node.id.clone(),
            incarnation: my_incarnation,
            members: all_members,
        };

        let encoded = bincode::serialize(&reply)?;
        self.socket.send_to(&encoded, src).await?;

        tracing::debug!("Sent ack to {:?} with {} members", from, self.members.len());

        Ok(())
    }

    /// Handles an `Ack` message.
    ///
    /// 1. Marks the sender as Alive and updates its timestamp.
    /// 2. Merges the piggybacked list of members (`members` vec) into the local view.
    async fn handle_ack(
        &self,
        from: NodeId,
        from_incarnation: u64,
        members: Vec<Node>,
    ) -> Result<()> {
        tracing::debug!(
            "Received ack from {:?} (inc={}) with {} members",
            from,
            from_incarnation,
            members.len()
        );
        tracing::debug!("MEMBERS: {:?}", members);

        if let Some(mut member) = self.members.get_mut(&from) {
            if member.state == NodeState::Dead {
                return Ok(());
            }

            member.last_seen = Some(Instant::now());

            if from_incarnation > member.incarnation {
                member.incarnation = from_incarnation;
            }
        }

        // Gossip dissemination: merge the received view into ours
        for member in members {
            self.merge_member(member).await;
        }

        Ok(())
    }

    /// Merges a single node update into the local member list.
    ///
    /// Applies conflict resolution rules based on incarnation numbers:
    /// - Higher incarnation number always wins.
    /// - If equal incarnation, specific state overrides may apply.
    async fn merge_member(&self, new_member: Node) {
        match self.members.get_mut(&new_member.id) {
            Some(mut existing) => {
                if existing.state == NodeState::Dead {
                    tracing::debug!("Ignoring update for dead node {:?}", existing.id);
                    return;
                }
                if new_member.incarnation > existing.incarnation {
                    tracing::debug!(
                        "Updating {:?}: inc {} -> {}",
                        new_member.id,
                        existing.incarnation,
                        new_member.incarnation,
                    );

                    existing.state = new_member.state;
                    existing.incarnation = new_member.incarnation;
                    existing.last_seen = Some(Instant::now());
                }
            }
            None => {
                if new_member.state == NodeState::Dead {
                    return;
                }
                tracing::info!(
                    "Doscovered new member: {:?} at {}",
                    new_member.id,
                    new_member.gossip_addr
                );

                let mut member_with_timestamp = new_member;
                member_with_timestamp.last_seen = Some(Instant::now());

                self.members
                    .insert(member_with_timestamp.id.clone(), member_with_timestamp);
            }
        }
    }

    /// Handles a report that a specific node is suspected to be down.
    ///
    /// **Self-Defense Mechanism**:
    /// If the local node receives a `Suspect` message regarding *itself*, it increments its
    /// own `incarnation` number and immediately broadcasts an `Alive` message.
    /// This effectively "refutes" the suspicion and forces other nodes to accept it is alive.
    async fn handle_suspect(&self, node_id: NodeId, incarnation: u64) -> Result<()> {
        let msg_to_broadcast = {
            match self.members.get_mut(&node_id) {
                Some(mut existing) => {
                    if existing.state == NodeState::Dead {
                        tracing::debug!("Ignoring Alive message for dead node {:?}", node_id);
                        return Ok(());
                    }
                    if incarnation > existing.incarnation {
                        if existing.id == self.local_node.id {
                            // SELF-DEFENSE: We are suspected, but we are alive!
                            tracing::info!(
                                "Self-defense: Node {:?} at {} - refuting suspiction",
                                node_id,
                                self.local_node.gossip_addr
                            );
                            let my_incarnation = {
                                let mut inc = self.incarnation.write().await;
                                *inc += 1; // Increment incarnation to override the suspicion
                                *inc
                            };

                            existing.incarnation = my_incarnation;
                            existing.state = NodeState::Alive;
                            existing.last_seen = Some(Instant::now());

                            // Broadcast Alive to clear our name
                            Some(GossipMessage::Alive {
                                node_id: node_id.clone(),
                                incarnation: my_incarnation,
                            })
                        } else {
                            // Valid suspicion about another node
                            tracing::info!(
                                "Node {:?} at {} suspected",
                                existing.id,
                                existing.gossip_addr
                            );
                            existing.state = NodeState::Suspect;
                            existing.incarnation = incarnation;
                            existing.last_seen = Some(Instant::now());

                            None
                        }
                    } else {
                        None // Old news, ignore
                    }
                }
                None => {
                    tracing::debug!("Suspected node {:?} doesn't exist", node_id);
                    None
                }
            }
        };

        if let Some(msg) = msg_to_broadcast {
            self.broadcast_message(msg).await;
        }
        Ok(())
    }

    /// Handles an `Alive` message.
    ///
    /// If the incarnation number is higher than what we know, we mark the node as Alive.
    /// This allows a node to recover from a `Suspect` state.
    async fn handle_alive(&self, node_id: NodeId, incarnation: u64) -> Result<()> {
        match self.members.get_mut(&node_id) {
            Some(mut existing) => {
                if existing.state == NodeState::Dead {
                    tracing::debug!("Ignoring Alive message for dead node {:?}", node_id);
                    return Ok(());
                }
                if incarnation > existing.incarnation {
                    tracing::info!(
                        "Node {:?} at {} is now Alive (inc={})",
                        existing.id,
                        existing.gossip_addr,
                        incarnation
                    );
                    existing.state = NodeState::Alive;
                    existing.incarnation = incarnation;
                    existing.last_seen = Some(Instant::now());
                } else if incarnation == existing.incarnation
                    && existing.state == NodeState::Suspect
                {
                    // Refutation successful
                    tracing::info!(
                        "Node {:?} at {} successfully refuted suspiction",
                        existing.id,
                        existing.gossip_addr,
                    );
                    existing.state = NodeState::Alive;
                    existing.incarnation = incarnation;
                    existing.last_seen = Some(Instant::now());
                }
            }
            None => {
                tracing::debug!("Alive message for unknown node {:?}", node_id);
            }
        }

        Ok(())
    }

    /// Handles a `Join` request from a new node.
    ///
    /// Adds the joining node to the local membership list.
    /// The existence of this new node will be propagated via Gossip (piggybacked on Acks).
    async fn handle_join(&self, mut node: Node) -> Result<()> {
        tracing::info!("Node {:?} joining cluster at {}", node.id, node.gossip_addr);

        node.last_seen = Some(Instant::now());

        self.members.insert(node.id.clone(), node.clone());

        tracing::info!("Cluster size now: {}", self.members.len());

        Ok(())
    }

    /// Background loop for failure detection.
    ///
    /// Monitors the `last_seen` timestamps of all members.
    /// - If a node hasn't been seen for `SUSPECT_TIMEOUT`, it is marked as `Suspect`.
    /// - If it remains `Suspect` for `DEAD_TIMEOUT`, it is marked as `Dead` and removed from consideration.
    async fn failure_detection_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(FAILURE_DETECTION_INTERVAL);

        loop {
            interval.tick().await;
            let now = Instant::now();

            let mut messages_to_broadcast = Vec::new();

            for mut entry in self.members.iter_mut() {
                let member = entry.value_mut();

                if member.id == self.local_node.id {
                    continue;
                }

                if member.state == NodeState::Dead {
                    continue;
                }

                if let Some(last_seen) = member.last_seen {
                    let elapsed = now.duration_since(last_seen);

                    match member.state {
                        NodeState::Alive => {
                            if elapsed > SUSPECT_TIMEOUT {
                                tracing::warn!(
                                    "Node {:?} suspected (no contact fo {:?})",
                                    member.id,
                                    elapsed
                                );

                                member.state = NodeState::Suspect;

                                let msg = GossipMessage::Suspect {
                                    node_id: member.id.clone(),
                                    incarnation: member.incarnation,
                                };

                                messages_to_broadcast.push(msg);
                            }
                        }

                        NodeState::Suspect => {
                            if elapsed > DEAD_TIMEOUT {
                                tracing::debug!(
                                    "Node {:?} declared DEAD (no contact for {:?})",
                                    member.id,
                                    elapsed
                                );

                                member.state = NodeState::Dead;
                            }
                        }

                        NodeState::Dead => {}
                    }
                } else {
                    member.last_seen = Some(now);
                }
            }

            // Disseminate any suspicion events generated in this tick
            for msg in messages_to_broadcast {
                self.broadcast_message(msg).await;
            }
        }
    }

    /// Helper to broadcast a message to all known alive members.
    /// Note: In a full SWIM implementation, this would be optimized.
    /// Here it does a naive iteration over the member list.
    async fn broadcast_message(&self, msg: GossipMessage) {
        if let Ok(encoded) = bincode::serialize(&msg) {
            for entry in self.members.iter() {
                let member = entry.value();

                if member.id == self.local_node.id {
                    continue;
                }

                if member.state == NodeState::Alive
                    && let Err(e) = self.socket.send_to(&encoded, member.gossip_addr).await
                {
                    tracing::warn!("Failed to broadcast to {:?}: {}", member.id, e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_membership_creation() {
        let bind_addr = "127.0.0.1:0".parse().unwrap();
        let seed_nodes = vec![];

        let service = MembershipService::new(bind_addr, seed_nodes)
            .await
            .expect("Failed to create service");

        assert_eq!(service.members.len(), 1);

        let members = service.get_alive_members();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].state, NodeState::Alive);

        println!("Test passed! Local node: {:?}", service.local_node.id);
    }
}