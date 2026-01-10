use anyhow::Result;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;
use std::{net::SocketAddr, time::Duration};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tracing::info;

use super::types::{GossipMessage, Node, NodeId, NodeState};

const GOSSIP_INTERVAL: Duration = Duration::from_millis(500);
const FAILURE_DETECTION_INTERVAL: Duration = Duration::from_secs(2);
const SUSPECT_TIMEOUT: Duration = Duration::from_secs(5);
const DEAD_TIMEOUT: Duration = Duration::from_secs(10);

pub struct MembershipService {
    pub local_node: Node,
    pub members: Arc<DashMap<NodeId, Node>>,
    socket: Arc<UdpSocket>,
    incarnation: Arc<RwLock<u64>>,
}

impl MembershipService {
    pub async fn new(bind_addr: SocketAddr, seed_nodes: Vec<SocketAddr>) -> Result<Arc<Self>> {
        let socket = UdpSocket::bind(bind_addr).await?;
        let incarnation_counter = Arc::new(RwLock::new(1));
        let current_inc = *incarnation_counter.read().await;
        let local_node = Node {
            id: NodeId::new(),
            addr: bind_addr,
            state: NodeState::Alive,
            incarnation: current_inc,
            last_seen: Some(Instant::now()),
        };
        let members = Arc::new(DashMap::new());
        members.insert(local_node.id.clone(), local_node.clone());
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

    pub fn get_alive_members(&self) -> Vec<Node> {
        self.members
            .iter()
            .filter(|entry| entry.value().state == NodeState::Alive)
            .map(|entry| entry.value().clone())
            .collect()
    }

    async fn gossip_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(GOSSIP_INTERVAL);

        loop {
            interval.tick().await;

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

            use rand::Rng;
            let idx = rand::thread_rng().gen_range(0..alive_members.len());
            let target = &alive_members[idx];

            let incarnation = *self.incarnation.read().await;
            let msg = GossipMessage::Ping {
                from: self.local_node.id.clone(),
                incarnation,
            };

            if let Ok(encoded) = bincode::serialize(&msg) {
                if let Err(e) = self.socket.send_to(&encoded, target.addr).await {
                    tracing::warn!("Failed to send ping to {:?}: {}", target.id, e);
                } else {
                    tracing::debug!("Sent ping to {:?}", target.id);
                }
            } else {
                tracing::error!("Failed to serialize GossipMessage::Ping");
            }
        }
    }

    async fn receive_loop(self: Arc<Self>) {
        let mut buf = vec![0u8; 65536];

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

    async fn handle_ping(
        &self,
        from: NodeId,
        from_incarnation: u64,
        src: SocketAddr,
    ) -> Result<()> {
        tracing::debug!("Received ping from {:?}", from);

        if let Some(mut member) = self.members.get_mut(&from) {
            member.last_seen = Some(Instant::now());

            if from_incarnation > member.incarnation {
                member.incarnation = from_incarnation;
            }
        } else {
            tracing::info!("Discovered new member via piang: {:?} at {}", from, src);

            let new_node = Node {
                id: from.clone(),
                addr: src,
                state: NodeState::Alive,
                incarnation: from_incarnation,
                last_seen: Some(Instant::now()),
            };

            self.members.insert(new_node.id.clone(), new_node);
        }

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

        if let Some(mut member) = self.members.get_mut(&from)
            && from_incarnation > member.incarnation
        {
            member.incarnation = from_incarnation;
            member.last_seen = Some(Instant::now());
        }

        for member in members {
            self.merge_member(member).await;
        }

        Ok(())
    }

    async fn merge_member(&self, new_member: Node) {
        match self.members.get_mut(&new_member.id) {
            Some(mut existing) => {
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
                } else if new_member.incarnation == existing.incarnation
                    && new_member.state == NodeState::Alive
                    && existing.state == NodeState::Suspect
                {
                    tracing::info!("{:?} refuted suspiction", new_member.id);
                    existing.state = NodeState::Alive;
                    existing.last_seen = Some(Instant::now());
                }
            }
            None => {
                tracing::info!(
                    "Doscovered new member: {:?} at {}",
                    new_member.id,
                    new_member.addr
                );

                let mut member_with_timestamp = new_member;
                member_with_timestamp.last_seen = Some(Instant::now());

                self.members
                    .insert(member_with_timestamp.id.clone(), member_with_timestamp);
            }
        }
    }

    async fn handle_suspect(&self, node_id: NodeId, incarnation: u64) -> Result<()> {
        match self.members.get_mut(&node_id) {
            Some(mut existing) => {
                if incarnation > existing.incarnation {
                    if node_id == self.local_node.id {
                        tracing::info!("Node {:?} at {} alive", existing.id, existing.addr);
                        let my_incarnation = {
                            let mut inc = self.incarnation.write().await;
                            *inc += 1;
                            *inc
                        };

                        let msg = GossipMessage::Alive {
                            node_id: node_id.clone(),
                            incarnation: my_incarnation,
                        };

                        self.broadcast_message(msg).await;

                        existing.incarnation = my_incarnation;
                        existing.state = NodeState::Alive;
                        existing.last_seen = Some(Instant::now());
                    } else {
                        tracing::info!("Node {:?} at {} suspected", existing.id, existing.addr);
                        existing.state = NodeState::Suspect;
                        existing.incarnation = incarnation;
                        existing.last_seen = Some(Instant::now());
                    }
                }
            }
            None => {
                tracing::debug!("Suspected node {:?} doesn't exist", node_id);
            }
        }

        Ok(())
    }

    async fn handle_alive(&self, node_id: NodeId, incarnation: u64) -> Result<()> {
        match self.members.get_mut(&node_id) {
            Some(mut existing) => {
                if incarnation > existing.incarnation {
                    tracing::info!(
                        "Node {:?} at {} is now Alive (inc={})",
                        existing.id,
                        existing.addr,
                        incarnation
                    );
                    existing.state = NodeState::Alive;
                    existing.incarnation = incarnation;
                    existing.last_seen = Some(Instant::now());
                } else if incarnation == existing.incarnation
                    && existing.state == NodeState::Suspect
                {
                    tracing::info!(
                        "Node {:?} at {} successfully refuted suspiction",
                        existing.id,
                        existing.addr,
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

    async fn handle_join(&self, mut node: Node) -> Result<()> {
        tracing::info!("Node {:?} joining cluster at {}", node.id, node.addr);

        node.last_seen = Some(Instant::now());

        self.members.insert(node.id.clone(), node.clone());

        tracing::info!("Cluster size now: {}", self.members.len());

        Ok(())
    }

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

                                tracing::info!(
                                    "Cluster size now: {} alive nodes",
                                    self.get_alive_members().len()
                                );
                            }
                        }

                        NodeState::Dead => {
                            tracing::debug!(
                                "Node {:?} DEAD (no concact for {:?})",
                                member.id,
                                elapsed
                            );
                        }
                    }
                } else {
                    member.last_seen = Some(now);
                }
            }

            for msg in messages_to_broadcast {
                self.broadcast_message(msg).await;
            }
        }
    }

    async fn broadcast_message(&self, msg: GossipMessage) {
        if let Ok(encoded) = bincode::serialize(&msg) {
            for entry in self.members.iter() {
                let member = entry.value();

                if member.id == self.local_node.id {
                    continue;
                }

                if member.state == NodeState::Alive
                    && let Err(e) = self.socket.send_to(&encoded, member.addr).await
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
