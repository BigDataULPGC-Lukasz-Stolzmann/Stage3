# Distributed Search Cluster

A horizontally-scalable distributed system for data storage, full-text search, and distributed task execution. Built in Rust using Tokio async runtime, this system provides fault-tolerant data replication, gossip-based cluster membership, and automatic workload distribution across nodes.

## Table of Contents

- [Project Description](#project-description)
- [Architecture Overview](#architecture-overview)
- [Prerequisites](#prerequisites)
- [Building and Running](#building-and-running)
  - [Local Development](#local-development)
  - [Docker Compose Deployment](#docker-compose-deployment)
  - [Kubernetes Deployment](#kubernetes-deployment)
- [API Reference](#api-reference)
- [Benchmark Procedures](#benchmark-procedures)
- [Reproducing Benchmarks](#reproducing-benchmarks)
- [Demonstration Video](#demonstration-video)

## Project Description

This project implements a distributed search cluster capable of:

- **Distributed Data Storage**: Hash-partitioned key-value storage with configurable replication factor across 256 partitions
- **Full-Text Search**: Inverted index-based search over document collections with TF scoring
- **Cluster Membership**: UDP gossip protocol with failure detection (suspect/dead states) and automatic node discovery
- **Task Execution**: Distributed task queue with lease-based claiming and multi-worker processing
- **Data Ingestion**: Automated ingestion pipeline for Project Gutenberg books with metadata extraction

The system is designed to scale horizontally by adding nodes to the cluster. Each node automatically discovers peers through the gossip protocol and participates in data partitioning and replication.

## Architecture Overview

```
                                    +----------------+
                                    |     Nginx      |
                                    | Load Balancer  |
                                    +-------+--------+
                                            |
                    +-----------------------+-----------------------+
                    |                                               |
            +-------v--------+                             +--------v-------+
            |    Node 1      |<--- Gossip Protocol --->   |    Node 2      |
            |  (HTTP :6000)  |                             |  (HTTP :6000)  |
            |  (UDP  :5000)  |                             |  (UDP  :5001)  |
            +----------------+                             +----------------+
                    |                                               |
    +---------------+---------------+               +---------------+---------------+
    |               |               |               |               |               |
+---v---+       +---v---+       +---v---+       +---v---+       +---v---+       +---v---+
| Books |       |Datalake|      | Index |       | Books |       |Datalake|      | Index |
| Store |       | Store  |      | Store |       | Store |       | Store  |      | Store |
+-------+       +--------+      +-------+       +-------+       +--------+      +-------+
```

### Core Components

| Component | Description |
|-----------|-------------|
| Membership Service | Gossip-based cluster membership with failure detection |
| Storage Layer | Distributed key-value store with partitioning and replication |
| Search Engine | Full-text search with tokenization and relevance scoring |
| Task Executor | Distributed task queue with worker pool |
| Ingestion Handler | Document ingestion pipeline for Project Gutenberg |

## Prerequisites

- Rust 1.75+ (2024 edition)
- Docker and Docker Compose (for containerized deployment)
- kubectl and a Kubernetes cluster (for Kubernetes deployment)
- curl and jq (for testing)

## Building and Running

### Local Development

1. **Build the project**:
```bash
cargo build --release
```

2. **Start the first node (seed node)**:
```bash
cargo run --release -- --bind 127.0.0.1:5000
```
The HTTP API will be available at `127.0.0.1:6000` (gossip port + 1000).

3. **Start additional nodes**:
```bash
# Node 2
cargo run --release -- --bind 127.0.0.1:5001 --seed 127.0.0.1:5000

# Node 3
cargo run --release -- --bind 127.0.0.1:5002 --seed 127.0.0.1:5000
```

4. **Verify cluster health**:
```bash
curl http://127.0.0.1:6000/health/stats
```

### Docker Compose Deployment

1. **Configure node addresses** in `docker-compose.yml`:
```yaml
services:
  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
    depends_on:
      - node1
      - node2
```

2. **Update `nginx.conf`** with your node addresses:
```nginx
upstream cluster {
    least_conn;
    server node1:6000;
    server node2:6000;
}
```

3. **Build and start the cluster**:
```bash
docker-compose build
docker-compose up -d
```

4. **Verify deployment**:
```bash
curl http://localhost/health/stats
```

5. **Scale the cluster**:
```bash
docker-compose up -d --scale node=4
```

6. **Stop the cluster**:
```bash
docker-compose down
```

### Kubernetes Deployment

1. **Create a ConfigMap for Nginx**:
```bash
kubectl create configmap nginx-config --from-file=nginx.conf
```

2. **Deploy the cluster using the provided manifests**:
```bash
kubectl apply -f k8s/
```

3. **Alternatively, deploy manually**:

Create a StatefulSet for cluster nodes:
```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: search-cluster
spec:
  serviceName: search-cluster
  replicas: 3
  selector:
    matchLabels:
      app: search-cluster
  template:
    metadata:
      labels:
        app: search-cluster
    spec:
      containers:
      - name: node
        image: search-cluster:latest
        ports:
        - containerPort: 5000
          name: gossip
        - containerPort: 6000
          name: http
        args:
        - "--bind"
        - "0.0.0.0:5000"
        - "--seed"
        - "search-cluster-0.search-cluster:5000"
```

4. **Expose the service**:
```bash
kubectl expose statefulset search-cluster --port=6000 --type=LoadBalancer
```

5. **Verify pods are running**:
```bash
kubectl get pods -l app=search-cluster
kubectl logs search-cluster-0
```

## API Reference

### Health and Status
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health/routes` | GET | List available API routes |
| `/health/stats` | GET | Node statistics (memory, CPU, partitions) |

### Ingestion
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/ingest/{book_id}` | POST | Ingest book from Project Gutenberg |
| `/ingest/status/{book_id}` | GET | Check ingestion status |

### Search
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/search?q={query}&limit={n}` | GET | Search books with pagination |
| `/books` | POST | Create book metadata |

### Task Execution
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/task/submit` | POST | Submit a task |
| `/task/status/{id}` | GET | Get task execution status |

## Benchmark Procedures

The project includes several test and benchmark scripts to evaluate system performance.

### Test Scripts Overview

| Script | Purpose |
|--------|---------|
| `test_cluster.sh` | Comprehensive integration tests for storage and executor |
| `test_executor.sh` | Task execution specific tests |
| `ingest_100_test.sh` | Stress test with 100 book ingestions |

### Metrics Collected

1. **Ingestion Throughput**: Documents ingested per second
2. **Search Latency**: Query response time (p50, p95, p99)
3. **Task Execution Time**: Time from submission to completion
4. **Cluster Synchronization**: Partition resync timing
5. **Memory Usage**: Per-node memory consumption
6. **CPU Utilization**: Worker thread efficiency

## Reproducing Benchmarks

### Prerequisites for Benchmarks

1. Start a cluster with at least 2 nodes
2. Ensure curl and jq are installed
3. Have network access to Project Gutenberg (for ingestion tests)

### Running Integration Tests

```bash
# Start two nodes first
# Terminal 1:
cargo run --release -- --bind 127.0.0.1:5000

# Terminal 2:
cargo run --release -- --bind 127.0.0.1:5001 --seed 127.0.0.1:5000

# Terminal 3: Run tests
chmod +x test_cluster.sh
./test_cluster.sh
```

The test script validates:
- Storage PUT/GET operations
- Cross-node data reads
- Data overwrites
- Task submission and lifecycle
- Concurrent task execution
- Error handling for unknown handlers

### Running Executor Tests

```bash
chmod +x test_executor.sh
./test_executor.sh
```

Tests include:
- Task submission verification
- Cross-node task distribution
- Burst task submission (5 concurrent tasks)
- Unknown handler error handling

### Running Ingestion Stress Test

```bash
chmod +x ingest_100_test.sh
./ingest_100_test.sh
```

This script:
- Ingests 100 books sequentially from Project Gutenberg
- Applies 0.2s delay between requests to avoid rate limiting
- Measures total ingestion time

### Manual Benchmark Commands

**Measure search latency**:
```bash
# Ingest sample data first
for i in $(seq 1 20); do
  curl -X POST "http://localhost:6000/ingest/$i"
  sleep 1
done

# Run search benchmark
for i in $(seq 1 100); do
  time curl -s "http://localhost:6000/search?q=adventure&limit=10" > /dev/null
done
```

**Measure concurrent ingestion**:
```bash
# Parallel ingestion using GNU parallel
seq 100 200 | parallel -j 10 "curl -X POST http://localhost:6000/ingest/{}"
```

**Monitor cluster statistics**:
```bash
watch -n 1 'curl -s http://localhost:6000/health/stats | jq'
```

### Expected Benchmark Results

| Metric | Expected Range |
|--------|----------------|
| Ingestion rate | 5-10 docs/second (network dependent) |
| Search latency (p50) | < 50ms |
| Task completion | < 2s per document |
| Partition resync | < 5s for 1000 entries |

## Demonstration Video

YouTube Link: [https://youtu.be/x4k3mQ5bvsc?si=w3i-12cmRHhGQIup]

The demonstration video covers:
1. Cluster deployment and node discovery
2. Data ingestion from Project Gutenberg
3. Full-text search functionality
4. Fault tolerance demonstration (node failure and recovery)
5. Task execution and monitoring
6. Load balancing with Nginx

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `REPLICATION_FACTOR` | 2 | Number of data replicas |
| `MAX_BODY_BYTES` | 20MB | Maximum HTTP request body size |

### Command Line Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `--bind <addr:port>` | Yes | Gossip protocol bind address |
| `--seed <addr:port>` | No | Seed node address for joining cluster |

## Project Structure

```
.
├── src/
│   ├── main.rs              # Application entry point
│   ├── lib.rs               # Module declarations
│   ├── membership/          # Gossip-based cluster membership
│   │   ├── service.rs       # Membership service implementation
│   │   ├── types.rs         # Node and message types
│   │   └── tests.rs         # Membership tests
│   ├── storage/             # Distributed storage layer
│   │   ├── memory.rs        # Distributed key-value store
│   │   ├── partitioner.rs   # Partition management
│   │   ├── protocol.rs      # Storage protocol
│   │   ├── handlers.rs      # HTTP handlers
│   │   └── tests.rs         # Storage tests
│   ├── search/              # Search engine
│   │   ├── engine.rs        # Search ranking logic
│   │   ├── tokenizer.rs     # Text tokenization
│   │   ├── handlers.rs      # Search HTTP handlers
│   │   ├── types.rs         # Search types
│   │   └── tests.rs         # Search tests
│   ├── executor/            # Task execution
│   │   ├── executor.rs      # Worker pool
│   │   ├── queue.rs         # Distributed queue
│   │   ├── registry.rs      # Handler registry
│   │   ├── protocol.rs      # Task protocol
│   │   ├── handlers.rs      # Task HTTP handlers
│   │   ├── types.rs         # Task types
│   │   └── tests.rs         # Executor tests
│   └── ingestion/           # Data ingestion
│       ├── handlers.rs      # Ingestion handlers
│       └── types.rs         # Ingestion types
├── ui/                      # Web UI (optional)
│   ├── src/
│   │   └── main.rs          # UI server
│   └── src/ui.html          # Web interface
├── docker-compose.yml       # Docker deployment
├── nginx.conf               # Load balancer config
├── Cargo.toml               # Rust dependencies
├── test_cluster.sh          # Integration tests
├── test_executor.sh         # Executor tests
└── ingest_100_test.sh       # Stress test
```

## License

This project was developed as part of a Big Data systems course.
