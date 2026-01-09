# Day 1 Summary - Distributed Cluster Implementation

## Kontekst
**Data:** 2026-01-09
**Cel:** Implementacja Hazelcast-like distributed system w Rust (projekt na zaliczenie)
**Deadline:** 7 dni
**Deployment:** 4 fizyczne maszyny w LAN

---

## Co zostało zaimplementowane dzisiaj:

### ✅ 1. Membership Service (SWIM Gossip Protocol)

**Lokalizacja:** `src/membership/`

**Komponenty:**
- **types.rs** - Struktury danych:
  - `NodeId` - UUID noda
  - `NodeState` - Alive/Suspect/Dead
  - `Node` - Informacje o nodzie (ID, addr, incarnation, last_seen)
  - `GossipMessage` - Ping, Ack, Join, Suspect, Alive

- **service.rs** - Core logic:
  - `MembershipService::new()` - Inicjalizacja z seed nodes
  - `gossip_loop()` - Wysyła ping co 500ms do losowego noda
  - `receive_loop()` - Odbiera UDP messages i handleuje
  - `failure_detection_loop()` - Wykrywa timeouty (5s suspect, 10s dead)
  - `handle_ping()` - Odpowiada ACK z member list
  - `handle_ack()` - Merguje member list (gossip propagation)
  - `handle_join()` - Dodaje nowy node do klastra
  - `merge_member()` - Conflict resolution używając incarnation
  - `broadcast_message()` - Wysyła do wszystkich alive nodes

**Kluczowe koncepty wyjaśnione:**

1. **Arc<Self> vs &self:**
   - Arc = shared ownership, cheap clone (tylko pointer)
   - Wszystkie Arc wskazują TEN SAM obiekt w pamięci
   - Clone zwiększa tylko reference count (atomic)

2. **DashMap - Concurrent HashMap:**
   - Interior mutability - lock'i wewnątrz
   - Fine-grained locking (per-shard)
   - Nie trzeba ręcznego `Mutex<HashMap>`
   - Automatyczne locki w `.insert()`, `.get_mut()`

3. **Incarnation number:**
   - Counter zwiększany po restarcie noda
   - Używany do conflict resolution
   - Większa incarnation = nowsza informacja
   - Zapobiega "Node is DEAD" po restarcie

4. **Why tokio::spawn() w main:**
   - `service.start().await` blokowałoby na zawsze
   - `spawn()` idzie w tle, main może obsłużyć Ctrl+C
   - Osobne taski dla core i stats (różne intervale)

5. **UDP Buffer 65536 bytes:**
   - Limit UDP = 65507 bytes payload
   - Packet > buffer → truncate (NIE panic)
   - Nasze packety ~1-5KB (bezpieczne)

6. **Copy vs Clone:**
   - `SocketAddr` jest Copy (automatic bitwise copy)
   - `String`, `Arc`, `Node` tylko Clone (explicit)
   - Copy = tanie, Clone = może być drogie

---

## Struktura projektu:

```
distributed-cluster/
├── Cargo.toml
├── ARCHITECTURE.md          # Kompletny design doc
├── DAY1_SUMMARY.md          # Ten plik
└── src/
    ├── main.rs              # Entry point z CLI parsing
    ├── lib.rs               # Library exports
    └── membership/
        ├── mod.rs           # Module exports
        ├── types.rs         # Data structures
        └── service.rs       # Core membership logic (413 lines)
```

---

## Dependencies (Cargo.toml):

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
bincode = "1"          # Binary serialization (szybsze niż JSON)
uuid = { version = "1", features = ["v4"] }
dashmap = "5"          # Concurrent HashMap
tracing = "0.1"        # Logging
tracing-subscriber = "0.3"
anyhow = "1"           # Error handling
rand = "0.8"           # Random node selection
```

---

## Testy wykonane:

### Test 1: Unit test
```bash
cargo test
# Output: test membership::service::tests::test_membership_creation ... ok
```

### Test 2: Kompilacja
```bash
cargo build --release
# Status: ✅ Kompiluje się (tylko warnings o unused variables)
```

### Test 3: Uruchomienie localhost (TODO - następny krok)
```bash
# Terminal 1:
./target/release/node --bind 127.0.0.1:5000

# Terminal 2:
./target/release/node --bind 127.0.0.1:5001 --seed 127.0.0.1:5000

# Expected: 2 nodes widzą się, gossip działa
```

---

## Kluczowe problemy rozwiązane:

### Problem 1: "Zawsze 1 alive node"
**Przyczyna:** `handle_join()` był pusty (tylko `Ok(())`)
**Rozwiązanie:** Implementacja dodawania noda do members:
```rust
async fn handle_join(&self, mut node: Node) -> Result<()> {
    node.last_seen = Some(Instant::now());
    self.members.insert(node.id.clone(), node.clone());
    tracing::info!("Cluster size now: {}", self.members.len());
    Ok(())
}
```

### Problem 2: "my_incarnation nie używana"
**Przyczyna:** Ack nie miał incarnation field
**Rozwiązanie:** Dodano incarnation do GossipMessage::Ack

### Problem 3: Borrow checker errors
**Nauka:**
- `Arc<DashMap>` daje interior mutability
- `.get_mut()` lepsze niż `.get()` + `.insert()`
- `#[serde(skip)]` dla Instant (nie-Serialize)

---

## Jak to działa (Flow):

### Startup Flow:
```
1. Node1 startuje:
   - new() → Tworzy local_node, binduje UDP socket
   - start() → Spawns 3 tasks (gossip, receive, failure_detection)
   - Cluster size: 1

2. Node2 startuje z --seed Node1:
   - new() → Wysyła JOIN do Node1
   - Node1 otrzymuje JOIN → handle_join() → dodaje Node2
   - Node1 pinguje Node2 → ACK z member list
   - Node2 merguje list → widzi Node1
   - Cluster size: 2 (obie strony wiedzą)
```

### Gossip Protocol:
```
Co 500ms (gossip_loop):
  1. Wybierz losowy alive node
  2. Wyślij PING { from, incarnation }
  3. Target odpowiada ACK { from, incarnation, members }
  4. Merge member list (gossip propagation)

Informacja rozprzestrzenia się epidemicznie:
  Node1 zna [N1, N2]
  Node3 zna [N3]

  N1 → PING → N3
  N3 → ACK([N3]) → N1
  N1 merge: [N1, N2, N3] ✓

  N3 → PING → N1
  N1 → ACK([N1, N2, N3]) → N3
  N3 merge: [N1, N2, N3] ✓
```

### Failure Detection:
```
Co 2s (failure_detection_loop):
  For każdy member (poza sobą):
    elapsed = now - last_seen

    If Alive && elapsed > 5s:
      → Suspect
      → Broadcast Suspect message

    If Suspect && elapsed > 10s:
      → Dead
      → Log "Node declared DEAD"
```

---

## Co dalej (Day 2-7):

### Day 2: Consistent Hashing & Partitioning
- 256 partitions
- Partition assignment algorithm
- Rebalancing on topology change

### Day 3: Distributed Storage (DistributedMap)
- In-memory DashMap per partition
- PUT: determine partition → store or forward
- GET: check local → forward if needed

### Day 4: Replication
- Primary + Backup (replication_factor=1)
- Async replication
- Partition handoff on failure

### Day 5: Task Executor
- Distributed work queue
- Load balancing (least-loaded routing)
- Worker pool per node

### Day 6: Integration z Stage2-BigData
- Replace Redis backend z HazelcastBackend
- Reuse ingestion/indexing logic
- HTTP API (Axum)

### Day 7: Dashboard & Demo
- Web UI showing cluster state
- Live metrics (throughput, latency)
- Demo script dla prowadzącego

---

## Deployment na LAN (4 maszyny):

### Znaleźć IP:
```bash
ip addr show | grep "inet "
# Output: 192.168.1.10 ← Twoje IP
```

### Firewall:
```bash
sudo ufw allow 5000/udp
```

### Uruchomienie:

**Komputer 1 (192.168.1.10) - Seed:**
```bash
./target/release/node --bind 192.168.1.10:5000
```

**Komputer 2 (192.168.1.11):**
```bash
./target/release/node --bind 192.168.1.11:5000 --seed 192.168.1.10:5000
```

**Komputer 3 (192.168.1.12):**
```bash
./target/release/node --bind 192.168.1.12:5000 --seed 192.168.1.10:5000
```

**Komputer 4 (192.168.1.13):**
```bash
./target/release/node --bind 192.168.1.13:5000 --seed 192.168.1.10:5000
```

**WAŻNE:** Użyj prawdziwych IP (192.168.x.x), NIE localhost (127.0.0.1)!

---

## Pytania użytkownika i odpowiedzi:

### Q: "Czy Arc clone kopiuje dane?"
**A:** NIE! Arc::clone() kopiuje tylko pointer (8 bajtów) i zwiększa reference count. Wszystkie Arc wskazują TEN SAM obiekt w pamięci. To jest cały sens Arc - cheap sharing!

### Q: "Po co kilka tasków w main?"
**A:**
- Task 1 (core) - gossip/receive/failure_detection - MUSI działać
- Task 2 (stats) - monitoring co 5s - nice to have
- Osobne bo różne intervale i separation of concerns
- spawn() zamiast await() żeby Ctrl+C działał

### Q: "Dlaczego nie używamy incarnation w handle_ping?"
**A:** Production version powinien update'ować incarnation pinera. Na razie simplifikacja - update tylko przez ACK. Dodamy Day 2.

### Q: "Dlaczego buffer 65536 bytes?"
**A:** Limit UDP payload = 65507 bytes. Nasze packety ~1-5KB. 64KB = bezpieczny margines.

### Q: "Co jeśli packet > buffer?"
**A:** UDP obcina (truncate), deserializacja fail, NIE panic. W praktyce nie problem bo nasze packety małe.

### Q: "Czemu nie Mutex a DashMap?"
**A:** DashMap ma interior mutability - automatic locking wewnątrz. Fine-grained (per-shard) vs coarse-grained (whole map). Lepszy concurrent access.

### Q: "Czy to zadziała na 2 kompach w LAN?"
**A:** TAK! Bez zmian w kodzie, tylko użyj prawdziwych IP zamiast 127.0.0.1 i otwórz firewall.

---

## Performance Expectations:

### Localhost:
- Gossip latency: <1ms
- Member discovery: <2s
- Failure detection: 10-15s

### LAN (4 machines):
- Gossip latency: 1-5ms (zależy od switcha)
- Member discovery: 2-5s
- Failure detection: 10-20s
- Throughput: 1000+ messages/sec

### Scalability:
- 3-10 nodes: optimal
- 10-50 nodes: działa dobrze
- 50+ nodes: gossip overhead rośnie (O(N²) messages)

---

## Metryki sukcesu (demo):

1. ✅ 4 nodes startują i widzą się nawzajem
2. ✅ Kill jeden node → reszta wykrywa failure w <15s
3. ✅ Restart node → auto-rejoin w <5s
4. ✅ Gossip propaguje informacje w <10s
5. ✅ Dashboard pokazuje live cluster state

---

## Komenda do ponownego uruchomienia kontekstu:

Jeśli zamkniesz Claude i wrócisz:
```bash
# Czytaj te pliki w kolejności:
1. /home/uka/Documents/big_data_uni/distributed-cluster/ARCHITECTURE.md
2. /home/uka/Documents/big_data_uni/distributed-cluster/DAY1_SUMMARY.md
3. Kod w src/membership/

# Następnie kontynuuj od Day 2
```

---

## Status na koniec Day 1:

✅ **COMPLETED:**
- Membership service (SWIM gossip)
- Failure detection (Alive → Suspect → Dead)
- Auto-discovery (JOIN protocol)
- Conflict resolution (incarnation)
- CLI interface (--bind, --seed)
- Graceful shutdown (Ctrl+C)

⏳ **TODO (Day 2-7):**
- Partitioning (consistent hashing)
- Distributed storage (in-memory map)
- Replication (primary + backup)
- Task executor (distributed work queue)
- Integration z Stage2 (search engine)
- HTTP API (Axum)
- Web dashboard
- Testing na 4 maszynach

**Linie kodu:** ~650 (service.rs: 413, main.rs: 87, types.rs: 59)

**Czas pracy:** ~6 godzin (nauka + implementacja)

**Następny krok:** Test 2-node localhost, potem Day 2 (partitioning)

---

*Saved: 2026-01-09 Evening*
*Ready for Day 2 tomorrow! 🚀*
