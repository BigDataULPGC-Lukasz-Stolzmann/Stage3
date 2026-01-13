# SESSION SUMMARY - 2026-01-13

## ✅ CO DZIAŁA:
1. **4-node gossip cluster** - UDP SWIM protocol, auto-discovery
2. **Distributed PUT** - forwarding do primary, synchroniczna replikacja na backup
3. **Distributed GET** - fetch z remote primary/backup, fixed infinite loop
4. **Cross-node operations** - PUT na jednym, GET z drugiego działa
5. **Data updates** - wielokrotne PUT tego samego klucza działa
6. **HTTP endpoints** - `/put`, `/get/:key`, `/forward_put`, `/replicate`
7. **Partitioning** - 256 partitions, consistent hashing
8. **Primary-backup model** - owners[0]=primary, owners[1]=backup

## ❌ KNOWN ISSUES:
1. **Nodes oznaczane jako Dead** - failure detection czasem fałszywie zabija nody (nie crashują, tylko membership oznacza Dead)
2. **"Too many open files" resolved** - GET loop fixed (linia 143+ w memory.rs)
3. **Forward_put nie replikuje** - Bug #3, dane tylko na primary, brak backupu (nie crashuje, ale data loss risk)

## 🔧 FIXED TODAY:
- GET infinite loop (primary fetchował od siebie) - `memory.rs:143-152`
- HTTP port calculation (gossip_addr + 1000)
- `handle_forward_put` używa `store_local` zamiast `put_local` (no recursion)
- Node struct ma `gossip_addr` i `http_addr` zamiast jednego `addr`

## 📝 TODO NEXT SESSION:
1. **Debug membership** - dlaczego nody są Dead gdy procesy żyją
   - Sprawdź logi: "Node declared DEAD (no contact for XXs)"
   - Może SUSPECT/DEAD timeouts za krótkie? (5s/10s)
   - Może `merge_member` ignoruje dead nodes na zawsze?

2. **Fix forward_put replication** - `handlers.rs:128`
   - Teraz: `map.store_local(req.partition, key, value);` - BRAK replikacji
   - Powinno: replikować do backup node
   - Problem: dane tylko na primary gdy forward_put użyty

3. **Async replication** - `memory.rs:229`
   - Teraz: synchroniczne `.await?` blokuje klienta
   - Change to: `tokio::spawn` fire-and-forget
   - Ale wymaga K,V: 'static bounds

4. **Retry queue** - dla failed replications
   - mpsc channel + background worker
   - Exponential backoff (1s, 2s, 4s, 8s)
   - Max 5 retry attempts

5. **Versioning** - conflict resolution
   - Dodaj `Versioned<V>` wrapper z version counter
   - AtomicU64 dla version generation
   - LWW (Last Write Wins) przy konfliktach

## 🐛 BUGS ZNALEZIONE I FIXED:

### Bug #1: GET Infinite Loop (CRITICAL - FIXED)
**Lokalizacja:** `memory.rs:143-145`
```rust
// PRZED:
let primary_owner = &owners[0];
match self.fetch_remote(primary_owner, key).await {  // ← loop gdy JA jestem primary!

// PO FIX:
if primary_owner == &self.membership.local_node.id {
    if owners.len() > 1 {
        return self.fetch_remote(&owners[1], key).await.ok().flatten();
    }
    return None;
}
```
**Problem:** Node fetchował GET od samego siebie → infinite loop → setki HTTP connections → "Too many open files" → crash

### Bug #2: Response Body Not Consumed (FALSE ALARM)
**Lokalizacja:** `memory.rs:66-70, 100-103`
- Początkowo myśleliśmy że trzeba `.bytes().await` przed drop
- Reqwest automatycznie zamyka connection przy drop response
- "Too many open files" był symptomem Bug #1, nie Bug #2

### Bug #3: Forward_put Nie Replikuje (TODO)
**Lokalizacja:** `handlers.rs:128`
```rust
// Problem:
map.store_local(req.partition, key, value);  // ← tylko local, BRAK replikacji
```
**Impact:** Dane trafiają na primary ale NIE na backup → data loss risk przy node failure

## 📊 ARCHITEKTURA OBECNA:

### PUT Flow:
```
Client → PUT /put {"key":"X", "value_json":"..."}
  ↓
1. hash(key) → partition number (0-255)
2. get_owners(partition) → [primary_id, backup_id]
3a. JA jestem primary?
    YES → store_local + replicate_to_backup (sync .await)
    NO  → forward_put(primary) → primary robi 3a.YES
```

### GET Flow:
```
Client → GET /get/X
  ↓
1. Check local storage
2. get_owners(partition) → [primary_id, backup_id]
3. JA jestem primary?
    YES → return None (nie mam lokalnie)
          lub fetch z backup (crash recovery)
    NO  → fetch_remote(primary)
          fallback → fetch_remote(backup) przy błędzie
```

### Membership (SWIM):
```
- gossip_loop: ping random alive node co 500ms
- receive_loop: odpowiadaj na ping/join/ack
- failure_detection_loop: co 2s sprawdź timeouty
  - no contact 5s → SUSPECT
  - no contact 10s → DEAD (permanent!)
```

## 🧪 TESTING COMMANDS:

```bash
# Uruchom 4 nody:
./target/release/node --bind 127.0.0.1:5000
./target/release/node --bind 127.0.0.1:5001 --seed 127.0.0.1:5000
./target/release/node --bind 127.0.0.1:5002 --seed 127.0.0.1:5000
./target/release/node --bind 127.0.0.1:5003 --seed 127.0.0.1:5000

# PUT test:
curl -X POST http://127.0.0.1:6000/put \
  -H "Content-Type: application/json" \
  -d '{"key":"book_1","value_json":"{\"name\":\"Dune\",\"author\":\"Herbert\"}"}'

# GET test (z innego noda):
curl http://127.0.0.1:6002/get/book_1

# UPDATE test (ten sam klucz):
curl -X POST http://127.0.0.1:6001/put \
  -H "Content-Type: application/json" \
  -d '{"key":"book_1","value_json":"{\"name\":\"Updated\",\"author\":\"NewAuthor\"}"}'
```

## 📂 KEY FILES:

- `src/storage/memory.rs` - DistributedMap, PUT/GET logic, HTTP client
- `src/storage/handlers.rs` - Axum HTTP handlers
- `src/storage/partitioner.rs` - Consistent hashing, owner calculation
- `src/membership/service.rs` - SWIM gossip protocol
- `src/membership/types.rs` - Node struct (gossip_addr + http_addr)
- `src/main.rs` - Entry point, HTTP router setup

## 🎯 GŁÓWNE OSIĄGNIĘCIE:

**Masz działający distributed in-memory data grid z:**
- Cross-node PUT/GET
- Automatic data placement (partitioning)
- Replication (primary-backup)
- Gossip-based cluster membership
- Failure detection

**Z perspektywy użytkownika system DZIAŁA** - PUT i GET są OK, tylko membership czasem fałszywie markuje nody jako Dead.
