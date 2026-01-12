# Day 2 Session - Partitioning (2026-01-11)

## ✅ Co zrobiliśmy dzisiaj:

### 1. Przeczytaliśmy całą dokumentację:
- ARCHITECTURE.md - SWIM protocol, podstawy
- DAY1_SUMMARY.md - co zostało zaimplementowane Day 1
- INTEGRATION_PLAN.md - plan na kolejne dni
- Kod membership service (522 linie)

### 2. Zrozumieliśmy teorię partycji:

**Problem bez partycji:**
```
hash(key) % num_nodes → Node X

Node umiera → num_nodes się zmienia
→ ~92% danych musi się przenieść! 💥
```

**Rozwiązanie z partycjami:**
```
Step 1: hash(key) % 256 → Partition Y (STAŁA!)
Step 2: Partition Y → Node X (zmienia się przy failures)

Node umiera → tylko jego partycje się przenoszą
→ ~25% danych (zamiast 92%)! ✅
```

**Kluczowa insight:**
- Partycja = WIADRO na wiele kluczy
- TAK, wiele książek może być w jednej partycji
- Przykład: Partition 42 = [book_100, book_356, book_872, ...]

### 3. Napisałeś pierwszą wersję PartitionManager:

**Struktura:**
```rust
pub struct PartitionManager {
    num_partitions: u32,        // 256 (Hazelcast ma 271)
    replication_factor: usize,  // 1 (primary + backup)
    membership: Arc<MembershipService>,
}
```

**4 metody:**
1. `new()` - stworzenie ✅
2. `get_partition(key)` - hash % 256 ✅
3. `get_owners(partition)` - ring distribution ❌ BUGI!
4. `my_primary_partitions()` - filter ❌ BUGI!
5. `my_backup_partitions()` - filter ❌ BUGI!

---

## 🐛 Błędy które popełniłeś (WAŻNE - popraw jutro!):

### Błąd 1: `sort_by()` zwraca `()`, nie `Vec`!

```rust
// ❌ ŹLE:
let sorted = alive_nodes.sort_by(|a, b| a.0.cmp(&b.0));
// sorted jest () (unit), nie Vec!

// ✅ DOBRZE:
let mut node_ids: Vec<NodeId> = alive_nodes
    .into_iter()
    .map(|node| node.id)  // Node → NodeId
    .collect();

node_ids.sort_by(|a, b| a.0.cmp(&b.0));  // Sortuje in-place, zwraca ()
// Teraz node_ids jest posortowany!
```

### Błąd 2: Cannot move out of `self.membership.local_node.id`

```rust
// ❌ ŹLE:
let my_id = self.membership.local_node.id;  // Próba move z &self

// ✅ DOBRZE:
let my_id = self.membership.local_node.id.clone();  // Clone!
```

### Błąd 3: `owners[1]` może panic jeśli tylko 1 node!

```rust
// ❌ ŹLE:
owners.len() > 1 && &owners[1] == my_id  // Bounds check, ale & zbędne

// ✅ DOBRZE:
owners.len() > 1 && owners[1] == my_id  // Sprawdź długość PRZED [1]
```

---

## 📚 Czego się nauczyłeś (Rust patterns):

### 1. Pattern matching w closure:
```rust
// Iterator zwraca &u32 (referencję):
(0..256)
    .filter(|&partition| {  // ← & destructure: &u32 → u32
        // partition jest teraz u32, nie &u32
    })
```

**Dlaczego &partition?**
- `.filter()` wymaga `FnMut(&Item) -> bool` (przyjmuje referencję)
- `|&partition|` to pattern match: destructure `&u32` → `u32`
- Możesz też: `|partition: &u32|` i używać `*partition`

### 2. Closures POŻYCZAJĄ, nie zjadają:
```rust
let my_id = /* ... */;

.filter(|&partition| {
    owners[0] == my_id  // ← BORROW (immutable reference)
})

// my_id nadal istnieje po filter()! ✅
```

**Kiedy closure ZJADA (move)?**
- `move |x| { ... }` (keyword `move`)
- `tokio::spawn(async move { ... })` (wymagane dla async)
- Zwracanie closure z funkcji

### 3. `sort_by()` sortuje in-place:
```rust
let mut vec = vec![3, 1, 2];
vec.sort_by(|a, b| a.cmp(b));  // Zwraca (), ale vec teraz [1, 2, 3]
```

---

## 🌍 Teoria: Dlaczego partycje to industry standard?

| System | Liczba partycji | Notatki |
|--------|----------------|---------|
| **Hazelcast** | 271 (default) | Konfigurowalne, prod często 1000+ |
| **Redis Cluster** | 16,384 | Hash slots |
| **Cassandra** | 256 (vnodes) | Virtual nodes |
| **Riak** | 64-256 | Configurable |
| **Amazon Dynamo** | Dynamic | Paper opisuje virtual nodes |

**Dlaczego liczby pierwsze? (271 vs 256)**
- 271 = liczba pierwsza → lepsze rozłożenie dla dowolnej liczby nodów
- 256 = 2^8 → szybszy modulo (bit shift)
- Dla 4 nodów: praktycznie bez różnicy

**Akademicko:**
> "Użyłem partition-based consistent hashing (Amazon Dynamo, Cassandra approach)
> z 256 partycjami dla minimalizacji data movement przy node failures."

Prowadzący: 🤯

---

## 🎯 Co dalej (jutro - Day 2 cd.):

### 1. NAJPIERW: Popraw PartitionManager (3 błędy wyżej)

**Plik:** `src/storage/partitioner.rs`

**Struktura folderów (jeśli nie zrobiłeś):**
```bash
mkdir -p src/storage
touch src/storage/mod.rs
touch src/storage/partitioner.rs
```

**src/storage/mod.rs:**
```rust
pub mod partitioner;
```

**src/lib.rs (dodaj):**
```rust
pub mod membership;
pub mod storage;  // ← DODAJ tę linię
```

### 2. Test:
```bash
cargo build
cargo test
```

### 3. Następnie: DistributedMap (HTTP + in-memory storage)

**Będzie zawierać:**
- `put(key, value)` - zapisz (forward jeśli nie jestem owner)
- `get(key)` - pobierz (fetch z ownera)
- `store_local()` - zapis w RAM (DashMap)
- HTTP communication między nodami

**To będzie fun! Będziesz wysyłać dane przez sieć!** 🚀

---

## 📂 Struktura projektu (obecna):

```
distributed-cluster/
├── Cargo.toml
├── ARCHITECTURE.md
├── DAY1_SUMMARY.md
├── INTEGRATION_PLAN.md
├── DAY2_SESSION.md          ← TEN PLIK (nowy!)
└── src/
    ├── main.rs
    ├── lib.rs
    ├── membership/
    │   ├── mod.rs
    │   ├── types.rs
    │   └── service.rs       (522 linie, SWIM protocol ✅)
    └── storage/             ← DO ZROBIENIA JUTRO
        ├── mod.rs
        └── partitioner.rs   (do poprawy!)
```

---

## 📝 Poprawiony kod (użyj jutro):

```rust
// src/storage/partitioner.rs

use crate::membership::{service::MembershipService, types::NodeId};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub struct PartitionManager {
    num_partitions: u32,
    replication_factor: usize,
    membership: Arc<MembershipService>,
}

impl PartitionManager {
    pub fn new(membership: Arc<MembershipService>) -> Self {
        Self {
            num_partitions: 256,
            replication_factor: 1,
            membership,
        }
    }

    pub fn get_partition(&self, key: &str) -> u32 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        (hash % self.num_partitions as u64) as u32
    }

    pub fn get_owners(&self, partition: u32) -> Vec<NodeId> {
        let alive_nodes = self.membership.get_alive_members();

        if alive_nodes.is_empty() {
            return vec![];
        }

        // POPRAWKA: Ekstraktuj NodeId i sortuj
        let mut node_ids: Vec<NodeId> = alive_nodes
            .into_iter()
            .map(|node| node.id)
            .collect();

        node_ids.sort_by(|a, b| a.0.cmp(&b.0));

        let primary_idx = (partition as usize) % node_ids.len();
        let backup_idx = (partition as usize + 1) % node_ids.len();

        vec![
            node_ids[primary_idx].clone(),
            node_ids[backup_idx].clone(),
        ]
    }

    pub fn my_primary_partitions(&self) -> Vec<u32> {
        let my_id = self.membership.local_node.id.clone();  // POPRAWKA: clone()

        (0..self.num_partitions)
            .filter(|&partition| {
                let owners = self.get_owners(partition);
                !owners.is_empty() && owners[0] == my_id
            })
            .collect()
    }

    pub fn my_backup_partitions(&self) -> Vec<u32> {
        let my_id = self.membership.local_node.id.clone();  // POPRAWKA: clone()

        (0..self.num_partitions)
            .filter(|&partition| {
                let owners = self.get_owners(partition);
                owners.len() > 1 && owners[1] == my_id  // POPRAWKA: bounds check
            })
            .collect()
    }
}
```

---

## 💡 Jak wpuścić dane do klastra (zapytałeś):

### Odpowiedź krótka:
```bash
# PUT na DOWOLNY node:
curl -X POST http://192.168.1.10:6000/put \
  -d '{"key": "book_100", "value": {...}}'

# Node automatycznie:
# 1. Oblicza partition
# 2. Znajduje ownera
# 3. Forward jeśli trzeba
# 4. Replicate do backup
```

**To zrobimy w DistributedMap jutro!**

---

## 🎓 Kluczowe insights z dzisiaj:

1. **Partycje = sprytne rozdrobnienie**
   - Zamiast 4 nody → 256 partycji
   - Minimalizuje data movement (25% vs 92%)

2. **Partycja = wiadro na wiele kluczy**
   - Partition 42 = [book_100, book_356, book_872, ...]

3. **Industry standard**
   - Dynamo, Cassandra, Hazelcast, Redis - WSZYSCY używają

4. **Rust patterns**
   - `sort_by()` in-place
   - Closures borrow by default
   - Pattern matching: `|&x|`

---

## ✅ Status projektu:

**Day 1 (DONE):**
- ✅ Membership service (SWIM gossip)
- ✅ Failure detection
- ✅ Auto-discovery
- ✅ Przetestowane na 2 komputerach

**Day 2 (W TRAKCIE):**
- ⏳ PartitionManager (napisany, ma bugi - popraw jutro!)
- ⏳ DistributedMap (TODO jutro)

**Day 3-7 (TODO):**
- Replication
- Task Executor
- HTTP API
- Integration z Stage2
- Testing na 4 maszynach

---

## 🚀 Jutro zaczynasz od:

1. Popraw 3 błędy w PartitionManager (kod wyżej)
2. `cargo build && cargo test`
3. Implementacja DistributedMap:
   - `put()` - HTTP forward
   - `get()` - HTTP fetch
   - DashMap storage w RAM

**Widzimy się jutro!** 🌙

---

*Saved: 2026-01-11 Evening*
*Next: DistributedMap implementation*
