# Integration Plan - Stage2 → Distributed Cluster

## Obecna architektura Stage2-BigData:

```
Stage2-BigData/
├── services/
│   ├── ingestion-service/    (Port 7001)
│   │   └── Downloads books from Gutenberg
│   │   └── Stores in /app/datalake
│   │
│   ├── indexing-service/     (Port 7002)
│   │   └── Reads from datalake
│   │   └── Stores in Redis/PostgreSQL
│   │
│   ├── search-service/       (Port 7003)
│   │   └── Queries Redis/PostgreSQL
│   │
│   └── control-module/
│       └── Orchestrates: ingest → index → verify
```

**Backend trait (już masz):**
```rust
// Stage2-BigData/services/indexing-service/src/models/storage.rs

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn store_metadata(&self, book_id: u32, metadata: BookMetadata) -> Result<()>;
    async fn add_word(&self, word: &str, book_id: u32) -> Result<()>;
    // ...
}

pub enum Backend {
    Redis(RedisBackend),
    Postgres(PostgresBackend),
}
```

---

## Nowa architektura (Distributed):

```
distributed-cluster/
├── src/
│   ├── membership/          ✅ Day 1 (DONE)
│   │   ├── types.rs
│   │   └── service.rs
│   │
│   ├── storage/             ⏳ Day 2-3
│   │   ├── mod.rs
│   │   ├── partitioner.rs       # Consistent hashing
│   │   ├── distributed_map.rs   # In-memory storage
│   │   └── backend.rs           # StorageBackend impl
│   │
│   ├── executor/            ⏳ Day 4-5
│   │   ├── mod.rs
│   │   ├── task.rs              # Task types
│   │   └── executor.rs          # Distributed executor
│   │
│   ├── api/                 ⏳ Day 6
│   │   ├── mod.rs
│   │   ├── handlers.rs          # HTTP handlers
│   │   └── routes.rs
│   │
│   ├── ingestion/           ⏳ Day 6 (Reuse Stage2 logic)
│   │   └── downloader.rs        # Copy from Stage2
│   │
│   └── indexing/            ⏳ Day 6 (Reuse Stage2 logic)
│       └── tokenizer.rs         # Copy from Stage2
│
└── main.rs                  # All-in-one node
```

---

## Day 2-3: Distributed Storage

### Partitioner (Consistent Hashing):

```rust
// src/storage/partitioner.rs

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub struct PartitionManager {
    pub num_partitions: u32,  // 256
    pub replication_factor: usize,  // 1 (primary + backup)
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

    /// Książka ID → Partition number (0-255)
    pub fn get_partition(&self, key: &str) -> u32 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() % self.num_partitions as u64) as u32
    }

    /// Partition → [Primary Owner, Backup Owner]
    pub fn get_owners(&self, partition: u32) -> Vec<NodeId> {
        let alive_members = self.membership.get_alive_members();

        if alive_members.is_empty() {
            return vec![];
        }

        // Sortuj members po NodeId (consistent ordering)
        let mut sorted: Vec<_> = alive_members.into_iter()
            .map(|m| m.id)
            .collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));

        // Wybierz owners używając ring
        let primary_idx = (partition as usize) % sorted.len();
        let backup_idx = (partition as usize + 1) % sorted.len();

        vec![
            sorted[primary_idx].clone(),
            sorted[backup_idx].clone(),
        ]
    }

    /// Wszystkie partycje dla których jestem primary owner
    pub fn my_primary_partitions(&self) -> Vec<u32> {
        let my_id = &self.membership.local_node.id;

        (0..self.num_partitions)
            .filter(|&partition| {
                let owners = self.get_owners(partition);
                !owners.is_empty() && &owners[0] == my_id
            })
            .collect()
    }

    /// Wszystkie partycje dla których jestem backup owner
    pub fn my_backup_partitions(&self) -> Vec<u32> {
        let my_id = &self.membership.local_node.id;

        (0..self.num_partitions)
            .filter(|&partition| {
                let owners = self.get_owners(partition);
                owners.len() > 1 && &owners[1] == my_id
            })
            .collect()
    }
}
```

### DistributedMap (In-Memory Storage):

```rust
// src/storage/distributed_map.rs

use dashmap::DashMap;
use serde::{Serialize, de::DeserializeOwned};

pub struct DistributedMap<K, V>
where
    K: Hash + Eq + Clone + ToString,
    V: Clone + Serialize + DeserializeOwned,
{
    /// Local storage: Partition → (Key → Value)
    local_data: Arc<DashMap<u32, DashMap<K, V>>>,

    partitioner: Arc<PartitionManager>,
    membership: Arc<MembershipService>,
}

impl<K, V> DistributedMap<K, V>
where
    K: Hash + Eq + Clone + ToString + Send + Sync + 'static,
    V: Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    pub fn new(
        partitioner: Arc<PartitionManager>,
        membership: Arc<MembershipService>,
    ) -> Self {
        Self {
            local_data: Arc::new(DashMap::new()),
            partitioner,
            membership,
        }
    }

    /// PUT: Store key-value pair
    pub async fn put(&self, key: K, value: V) -> Result<()> {
        let partition = self.partitioner.get_partition(&key.to_string());
        let owners = self.partitioner.get_owners(partition);

        if owners.is_empty() {
            return Err(anyhow::anyhow!("No alive members"));
        }

        let my_id = &self.membership.local_node.id;

        // Czy jestem primary owner?
        if &owners[0] == my_id {
            // Store locally (primary)
            self.store_local(partition, key.clone(), value.clone()).await;

            // Replicate to backup (async, best-effort)
            if owners.len() > 1 {
                let backup_node = &owners[1];
                self.replicate_to(backup_node, partition, key, value).await.ok();
            }
        } else {
            // Forward to primary owner
            self.forward_put(&owners[0], partition, key, value).await?;
        }

        Ok(())
    }

    /// GET: Retrieve value by key
    pub async fn get(&self, key: &K) -> Option<V> {
        let partition = self.partitioner.get_partition(&key.to_string());

        // Check local storage first
        if let Some(part_data) = self.local_data.get(&partition) {
            if let Some(value) = part_data.get(key) {
                return Some(value.clone());
            }
        }

        // Not local, forward to owner
        let owners = self.partitioner.get_owners(partition);
        if owners.is_empty() {
            return None;
        }

        self.fetch_from_remote(&owners[0], key).await.ok()
    }

    /// Store data locally
    async fn store_local(&self, partition: u32, key: K, value: V) {
        self.local_data
            .entry(partition)
            .or_insert_with(DashMap::new)
            .insert(key, value);
    }

    /// Replicate to backup node (HTTP POST)
    async fn replicate_to(
        &self,
        target: &NodeId,
        partition: u32,
        key: K,
        value: V,
    ) -> Result<()> {
        let node = self.membership.members.get(target)
            .ok_or_else(|| anyhow::anyhow!("Node not found"))?;

        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "partition": partition,
            "key": key.to_string(),
            "value": value,
        });

        client
            .post(format!("http://{}/replicate", node.addr))
            .json(&payload)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await?;

        Ok(())
    }

    /// Forward PUT to primary owner
    async fn forward_put(
        &self,
        target: &NodeId,
        partition: u32,
        key: K,
        value: V,
    ) -> Result<()> {
        let node = self.membership.members.get(target)
            .ok_or_else(|| anyhow::anyhow!("Node not found"))?;

        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "partition": partition,
            "key": key.to_string(),
            "value": value,
        });

        client
            .post(format!("http://{}/put", node.addr))
            .json(&payload)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await?;

        Ok(())
    }

    /// Fetch from remote node
    async fn fetch_from_remote(&self, target: &NodeId, key: &K) -> Result<V> {
        let node = self.membership.members.get(target)
            .ok_or_else(|| anyhow::anyhow!("Node not found"))?;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://{}/get/{}", node.addr, key.to_string()))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await?;

        let value: V = response.json().await?;
        Ok(value)
    }
}
```

---

## Day 4-5: Task Executor

```rust
// src/executor/task.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Task {
    IngestBook { book_id: u32 },
    IndexBook { book_id: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl TaskId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}
```

```rust
// src/executor/executor.rs

use std::collections::VecDeque;
use tokio::sync::Mutex;

pub struct TaskExecutor {
    local_queue: Arc<Mutex<VecDeque<Task>>>,
    membership: Arc<MembershipService>,
    storage: Arc<DistributedMap<u32, BookMetadata>>,
    word_index: Arc<DistributedMap<String, HashSet<u32>>>,
}

impl TaskExecutor {
    /// Submit task - automatic load balancing
    pub async fn submit(&self, task: Task) -> Result<TaskId> {
        let task_id = TaskId::new();

        // Select least-loaded node
        let target = self.select_target_node().await;

        if target == self.membership.local_node.id {
            // Execute locally
            self.local_queue.lock().await.push_back(task);
        } else {
            // Forward to remote node
            self.forward_task(&target, task).await?;
        }

        Ok(task_id)
    }

    /// Select target based on load (simple: round-robin)
    async fn select_target_node(&self) -> NodeId {
        let alive = self.membership.get_alive_members();

        if alive.is_empty() {
            return self.membership.local_node.id.clone();
        }

        // Simple: random selection (można poprawić: queue length, CPU load)
        let idx = rand::random::<usize>() % alive.len();
        alive[idx].id.clone()
    }

    /// Start worker pool (N workers per node)
    pub async fn start_workers(self: Arc<Self>, num_workers: usize) {
        for worker_id in 0..num_workers {
            let executor = self.clone();

            tokio::spawn(async move {
                loop {
                    // Pop task from queue
                    let task = {
                        let mut queue = executor.local_queue.lock().await;
                        queue.pop_front()
                    };

                    if let Some(task) = task {
                        tracing::info!("Worker {} executing task: {:?}", worker_id, task);

                        if let Err(e) = executor.execute_task(task).await {
                            tracing::error!("Task failed: {}", e);
                            // TODO: Retry logic
                        }
                    } else {
                        // No tasks, sleep
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            });
        }
    }

    /// Execute task locally
    async fn execute_task(&self, task: Task) -> Result<()> {
        match task {
            Task::IngestBook { book_id } => {
                // REUSE Stage2 logic!
                let book = download_book(book_id).await?;

                // Store w distributed map
                self.storage.put(book_id, book.metadata).await?;

                tracing::info!("Ingested book {}", book_id);
            }

            Task::IndexBook { book_id } => {
                // REUSE Stage2 logic!
                let (metadata, words) = extract_and_tokenize(book_id).await?;

                // Store metadata
                self.storage.put(book_id, metadata).await?;

                // Store word index (distributed)
                for word in words {
                    // Get current set for this word
                    let mut book_ids = self.word_index.get(&word).await
                        .unwrap_or_default();

                    book_ids.insert(book_id);

                    // Put updated set
                    self.word_index.put(word, book_ids).await?;
                }

                tracing::info!("Indexed book {}", book_id);
            }
        }

        Ok(())
    }
}
```

---

## Day 6: HTTP API (Single Binary)

```rust
// main.rs (All-in-one distributed node)

use axum::{
    routing::{get, post},
    Router, Extension, Json,
    extract::{Path, Query},
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();

    let args = parse_args();

    // 1. Start membership (gossip, failure detection)
    let membership = MembershipService::new(args.bind_addr, args.seed_nodes).await?;
    tokio::spawn({
        let m = membership.clone();
        async move { m.start().await }
    });

    // 2. Start distributed storage
    let partitioner = Arc::new(PartitionManager::new(membership.clone()));
    let books = Arc::new(DistributedMap::<u32, BookMetadata>::new(
        partitioner.clone(),
        membership.clone(),
    ));
    let word_index = Arc::new(DistributedMap::<String, HashSet<u32>>::new(
        partitioner.clone(),
        membership.clone(),
    ));

    // 3. Start task executor
    let executor = Arc::new(TaskExecutor::new(
        membership.clone(),
        books.clone(),
        word_index.clone(),
    ));
    tokio::spawn({
        let e = executor.clone();
        async move { e.start_workers(4).await }
    });

    // 4. Start HTTP API
    let app = Router::new()
        // Public API
        .route("/ingest/:book_id", post(handle_ingest))
        .route("/search", get(handle_search))

        // Internal API (inter-node communication)
        .route("/put", post(handle_put))
        .route("/get/:key", get(handle_get))
        .route("/replicate", post(handle_replicate))

        // Cluster management
        .route("/cluster/members", get(handle_members))
        .route("/cluster/stats", get(handle_stats))

        // Extensions (shared state)
        .layer(Extension(membership))
        .layer(Extension(executor))
        .layer(Extension(books))
        .layer(Extension(word_index));

    let http_port = args.bind_addr.port() + 1000;  // 5000 → 6000
    let http_addr = SocketAddr::from(([0, 0, 0, 0], http_port));

    tracing::info!("HTTP API listening on {}", http_addr);

    axum::Server::bind(&http_addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

// HTTP Handlers

async fn handle_ingest(
    Path(book_id): Path<u32>,
    Extension(executor): Extension<Arc<TaskExecutor>>,
) -> Result<Json<TaskId>, StatusCode> {
    let task_id = executor.submit(Task::IngestBook { book_id })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(task_id))
}

async fn handle_search(
    Query(params): Query<SearchParams>,
    Extension(word_index): Extension<Arc<DistributedMap<String, HashSet<u32>>>>,
    Extension(books): Extension<Arc<DistributedMap<u32, BookMetadata>>>,
) -> Result<Json<Vec<BookMetadata>>, StatusCode> {
    // Tokenize query
    let words = tokenize(&params.q);

    // Fetch book IDs for each word (parallel)
    let mut book_id_sets = vec![];
    for word in words {
        if let Some(ids) = word_index.get(&word).await {
            book_id_sets.push(ids);
        }
    }

    // Intersection
    let common_ids = intersect_all(book_id_sets);

    // Fetch metadata (parallel)
    let mut results = vec![];
    for book_id in common_ids {
        if let Some(book) = books.get(&book_id).await {
            results.push(book);
        }
    }

    Ok(Json(results))
}

async fn handle_members(
    Extension(membership): Extension<Arc<MembershipService>>,
) -> Json<Vec<Node>> {
    let members = membership.get_alive_members();
    Json(members)
}
```

---

## Deployment (4 nodes):

```bash
# Każda maszyna ma ten sam binary!

# Machine 1 (192.168.1.10):
./distributed-cluster --bind 192.168.1.10:5000

# Machine 2 (192.168.1.11):
./distributed-cluster --bind 192.168.1.11:5000 --seed 192.168.1.10:5000

# Machine 3 (192.168.1.12):
./distributed-cluster --bind 192.168.1.12:5000 --seed 192.168.1.10:5000

# Machine 4 (192.168.1.13):
./distributed-cluster --bind 192.168.1.13:5000 --seed 192.168.1.10:5000
```

**Cluster tworzy się automatycznie przez gossip!**

---

## Usage (Client):

```bash
# Ingest books (można na dowolny node!)
curl -X POST http://192.168.1.10:6000/ingest/100
curl -X POST http://192.168.1.11:6000/ingest/101
curl -X POST http://192.168.1.12:6000/ingest/102

# Tasks są load-balanced na wszystkie nodes!

# Search (też na dowolny node!)
curl "http://192.168.1.10:6000/search?q=adventure"

# Response:
[
  {
    "book_id": 100,
    "title": "Treasure Island",
    "author": "Robert Louis Stevenson"
  },
  ...
]

# Cluster info
curl http://192.168.1.10:6000/cluster/members

# Response:
[
  { "id": "abc-123", "addr": "192.168.1.10:5000", "state": "Alive" },
  { "id": "def-456", "addr": "192.168.1.11:5000", "state": "Alive" },
  { "id": "ghi-789", "addr": "192.168.1.12:5000", "state": "Alive" },
  { "id": "jkl-012", "addr": "192.168.1.13:5000", "state": "Alive" }
]
```

---

## Performance (Expected):

### Stage2 (Centralized):
```
100 books × 90s = 9000s (2.5 hours)
Sequential processing
```

### Distributed (4 nodes):
```
100 books / 4 nodes = 25 books/node
25 books × 90s = 2250s (37 minutes)

Speedup: 4x (theoretical)
         3.3x (realistic with overhead)
```

### Fault Tolerance:
```
T0: 4 nodes, processing 100 books
T1: Node2 crashes (25 books in progress)
T2: Failure detected (~10s)
T3: Tasks redistributed to Node1,3,4
T4: Processing continues with 3 nodes
    Total time: +10-15% overhead

Result: ✅ No data loss, no manual intervention!
```

---

## Porównanie z Hazelcast (Java):

| Feature | Hazelcast | Nasz system |
|---------|-----------|-------------|
| In-memory storage | ✅ IMap | ✅ DistributedMap |
| Partitioning | ✅ 271 parts | ✅ 256 parts |
| Replication | ✅ Sync/Async | ✅ Async |
| Auto-discovery | ✅ Multicast | ✅ Gossip (UDP) |
| Failure detection | ✅ Heartbeat | ✅ SWIM |
| Task execution | ✅ IExecutorService | ✅ TaskExecutor |
| Language | Java | Rust |
| Memory safety | ❌ GC pauses | ✅ No GC |
| Performance | Good | Better (Rust) |
| Maturity | 15 years | 7 days 😄 |

---

## Next Steps:

1. **Today:** Test 2-node cluster localhost
2. **Tomorrow:** Implement partitioner + DistributedMap
3. **Day 3-4:** Replication + failure recovery
4. **Day 5:** Task executor + load balancing
5. **Day 6:** HTTP API + integration
6. **Day 7:** Testing on 4 machines + demo

**Finish line: Fully functional distributed search engine!** 🚀
