# Distributed Cluster Architecture - Learning Journal

## Context
Projekt na zaliczenie przedmiotu Big Data - implementacja Hazelcast-like distributed in-memory data grid w Rust.

**Deadline:** 7 dni od 2026-01-09
**Zespół:** 4 osoby (4 fizyczne maszyny w LAN)
**Cel:** Pokazać fault tolerance, load balancing, auto-recovery, distributed computing

---

## Hazelcast Features - Co implementujemy

### ✅ Core Features (zrobimy w 7 dni):
- Distributed Map (in-memory storage rozproszone między nody)
- Partitioning (256 partitions, consistent hashing)
- Replication (async, eventual consistency, 1 backup)
- Auto-discovery (UDP multicast w LAN)
- Failure detection (SWIM gossip protocol)
- Auto-recovery (node restart → automatic rejoin)
- Distributed Executor (task distribution, load balancing)
- Scalability (działa na 3, 4, 5, 10+ nodów)

### ⚠️ Advanced Features (nie robimy):
- Transactions
- Distributed locks
- Distributed queue
- WAN replication
- Split-brain protection (zakładamy reliable LAN)

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│           4 Nodes - Pure In-Memory Cluster              │
│               (No Redis, No NFS, No DB)                 │
└─────────────────────────────────────────────────────────┘

Node 1 (192.168.1.10)        Node 2 (192.168.1.11)
┌──────────────────────┐     ┌──────────────────────┐
│  Membership Service  │◄───►│  Membership Service  │
│  [N1, N2, N3, N4]    │     │  [N1, N2, N3, N4]    │
├──────────────────────┤     ├──────────────────────┤
│  In-Memory Storage   │     │  In-Memory Storage   │
│  ┌────────────────┐  │     │  ┌────────────────┐  │
│  │ Part 0: P      │  │     │  │ Part 0: B      │  │
│  │ Part 1: P      │  │     │  │ Part 1: B      │  │
│  │ Part 4: B      │  │     │  │ Part 4: P      │  │
│  │ Part 5: B      │  │     │  │ Part 5: P      │  │
│  └────────────────┘  │     │  └────────────────┘  │
│  P=Primary B=Backup  │     │  P=Primary B=Backup  │
├──────────────────────┤     ├──────────────────────┤
│  Worker Pool (4)     │     │  Worker Pool (4)     │
│  Task Queue          │     │  Task Queue          │
└──────────────────────┘     └──────────────────────┘
```

---

## Day 1: SWIM Gossip Protocol & Membership

### Problem: Cluster Membership
Jak każdy node wie o wszystkich innych bez central server?

**Złe rozwiązania:**
- ❌ Centralny serwer (single point of failure)
- ❌ Each pings all (O(N²) messages)
- ❌ Broadcasting (network congestion)

**Dobre rozwiązanie: Gossip Protocol**
- Każdy node co 500ms wysyła wiadomość do JEDNEGO losowego noda
- O(N) messages/sec
- Eventual consistency
- Informacja rozprzestrzenia się epidemicznie

### SWIM Protocol (Scalable Weakly-consistent Infection-style Membership)

**Failure Detection:**

```
1. Direct ping:
   Node1 → PING → Node2 (timeout 2s)

2. Indirect ping (jeśli no response):
   Node1 → PING-REQ(target=Node2) → Node3
   Node3 → PING → Node2
   Node2 → ACK → Node3
   Node3 → ACK → Node1

3. Suspect state:
   Node1 broadcasts: "Node2 SUSPECT"

4. Self-defense:
   Node2 (jeśli słyszy): "I'm ALIVE!" + zwiększa incarnation

5. Confirmation:
   Po 10s suspect bez ALIVE → Node2 DEAD
```

**Dlaczego to działa:**
- ✅ Wykrywa prawdziwe failures (nie network blips)
- ✅ False positive rate < 0.01%
- ✅ Detection time: 5-10 sekund
- ✅ Skaluje na 1000+ nodów

### Incarnation Number
**Problem:** Stare wiadomości "Node2 DEAD" krążą po restarcie Node2

**Rozwiązanie:**
- Node po restarcie zwiększa `incarnation` counter
- Reguła: najnowsza wiadomość = największa incarnation
- "Node2 DEAD (inc=5)" < "Node2 ALIVE (inc=6)" → ignoruj starą

---

## Tech Stack

```toml
tokio = { version = "1", features = ["full"] }  # Async runtime
serde = { version = "1", features = ["derive"] } # Serialization
bincode = "1"          # Binary serialization (szybszy niż JSON)
uuid = { version = "1", features = ["v4"] }     # Unique IDs
dashmap = "5"          # Concurrent HashMap (lock-free)
tracing = "0.1"        # Logging
anyhow = "1"           # Error handling
```

### Dlaczego bincode zamiast JSON?
- 50-70% mniejszy payload
- 3-5x szybszy
- Type-safe
- W distributed systems: performance > readability

### Dlaczego DashMap zamiast Mutex<HashMap>?
- Lock-free concurrent access
- Multiple readers równocześnie
- Fine-grained locking (per-shard)
- Mutex<HashMap> = bottleneck przy concurrent access

---

## Project Structure

```
distributed-cluster/
├── Cargo.toml
├── ARCHITECTURE.md          # This file
└── src/
    ├── main.rs              # Entry point
    ├── lib.rs               # Library exports
    └── membership/
        ├── mod.rs           # Public API
        ├── types.rs         # Data structures
        ├── service.rs       # MembershipService
        └── protocol.rs      # SWIM implementation
```

---

## Data Structures

### NodeId
```rust
pub struct NodeId(pub String);  // UUID
```
- Unikalny identyfikator noda
- Hash trait → używany jako klucz w DashMap

### NodeState
```rust
pub enum NodeState {
    Alive,    // Działa
    Suspect,  // Może nie działa (indirect ping failed)
    Dead,     // Nie działa (confirmed)
}
```

### Node
```rust
pub struct Node {
    pub id: NodeId,
    pub addr: SocketAddr,      // 192.168.1.10:5000
    pub state: NodeState,
    pub incarnation: u64,      // Conflict resolution
    #[serde(skip)]
    pub last_seen: Option<Instant>,  // Lokalny timestamp
}
```

**UWAGA:** `last_seen` ma `#[serde(skip)]` bo:
- `Instant` nie jest Serialize (machine-specific)
- Używamy tylko lokalnie do failure detection
- Nie przesyłamy przez sieć

### GossipMessage
```rust
pub enum GossipMessage {
    Ping { from: NodeId, incarnation: u64 },
    Ack { from: NodeId, members: Vec<Node> },
    Join { node: Node },
    Suspect { node_id: NodeId, incarnation: u64 },
    Alive { node_id: NodeId, incarnation: u64 },
}
```

---

## MembershipService Design

### Thread Safety
- `Arc<DashMap<NodeId, Node>>` - współdzielona mapa memberów
- `Arc<UdpSocket>` - socket współdzielony między tasks
- `Arc<RwLock<u64>>` - incarnation counter (write rare, read often)

### Background Tasks (tokio::spawn)
1. **gossip_loop** - co 500ms wysyła ping do losowego noda
2. **receive_loop** - nasłuchuje UDP messages, handleuje
3. **failure_detection_loop** - co 2s sprawdza timeouts

---

## Common Rust Issues & Solutions

### Problem 1: Self-referential structures
```rust
// ❌ Nie zadziała:
pub struct MembershipService {
    pub local_node: Node,
    pub members: DashMap<NodeId, Node>,  // zawiera local_node!
}
```

**Rozwiązanie:** Clone local_node do members:
```rust
let members = DashMap::new();
members.insert(local_node.id.clone(), local_node.clone());
```

### Problem 2: Instant is not Serialize
```rust
// ❌ Nie można przesłać przez sieć:
pub struct Node {
    pub last_seen: Instant,  // machine-specific!
}
```

**Rozwiązanie:** Skip w serialization:
```rust
#[serde(skip)]
pub last_seen: Option<Instant>,
```

### Problem 3: Moving out of Arc
```rust
// ❌ Nie można:
let service = Arc::new(MembershipService { ... });
tokio::spawn(service.gossip_loop());  // move occurs!
```

**Rozwiązanie:** Clone Arc (tani, tylko pointer):
```rust
let service_clone = service.clone();
tokio::spawn(async move {
    service_clone.gossip_loop().await;
});
```

---

## Testing Strategy

### Unit Tests
- Serialization/deserialization messages
- Partition assignment logic
- Consistent hashing

### Integration Tests
- 2 nodes: join, gossip, detect alive
- 3 nodes: failure detection, backup promotion
- 4 nodes: load balancing, rebalancing

### Demo Tests (na uczelni)
1. **Cluster formation:** 4 nodes startują, widzą się nawzajem
2. **Parallel processing:** 20 książek → 4x speedup
3. **Fault tolerance:** kill node2 → tasks redistrybuowane
4. **Auto-recovery:** restart node2 → automatic rejoin

---

## Timeline

- **Day 1-2:** Membership + SWIM protocol
- **Day 3-4:** Distributed storage + replication
- **Day 5:** Task executor + load balancing
- **Day 6:** Testing na 2 maszynach
- **Day 7:** Dashboard + demo prep

---

## Questions to Answer During Implementation

1. Dlaczego NodeId ma Hash trait? → Używany jako klucz w DashMap
2. Dlaczego last_seen ma #[serde(skip)]? → Instant nie jest Serialize
3. Co jeśli dwóch nodów wygeneruje ten sam UUID? → Prawdopodobieństwo ~10^-38
4. Dlaczego DashMap a nie Mutex<HashMap>? → Lock-free, lepszy concurrent access
5. Dlaczego bincode a nie JSON? → 3-5x szybszy, mniejszy payload

---

## References & Learning Resources

- [SWIM Paper](https://www.cs.cornell.edu/projects/Quicksilver/public_pdfs/SWIM.pdf)
- [Hazelcast IMDG Documentation](https://docs.hazelcast.com/imdg/latest/)
- [Tokio Documentation](https://tokio.rs/)
- [DashMap GitHub](https://github.com/xacrimon/dashmap)

---

*Last updated: 2026-01-09*
*Author: Learning Rust + Distributed Systems*
