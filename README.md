# Distributed Search Cluster

A horizontally-scalable distributed system for data storage, full-text search, and distributed task execution. Built in Rust using Tokio async runtime, this system provides fault-tolerant data replication, gossip-based cluster membership, and automatic workload distribution across nodes.

## Table of Contents

- [Project Description](#project-description)
- [Architecture Overview](#architecture-overview)
- [Prerequisites](#prerequisites)
- [Building and Running](#building-and-running)
  - [Local Development](#local-development)
  - [Nginx Load Balancer Setup](#nginx-load-balancer-setup)
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
- Docker (for running nginx load balancer)
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

### Nginx Load Balancer Setup

The application nodes run natively (not in Docker). Only the nginx load balancer runs in a Docker container.

1. **Start cluster nodes on your machines**:
```bash
# On machine 1 (seed node):
cargo run --release -- --bind 0.0.0.0:5000

# On machine 2:
cargo run --release -- --bind 0.0.0.0:5000 --seed <machine1-ip>:5000

# On machine 3:
cargo run --release -- --bind 0.0.0.0:5000 --seed <machine1-ip>:5000

# On machine 4:
cargo run --release -- --bind 0.0.0.0:5000 --seed <machine1-ip>:5000
```

2. **Configure `nginx.conf`** with your node addresses:
```nginx
upstream cluster {
    least_conn;
    server <machine1-ip>:6000;
    server <machine2-ip>:6000;
    server <machine3-ip>:6000;
    server <machine4-ip>:6000;
}
```

3. **Run nginx load balancer in Docker**:
```bash
docker run -d -p 80:80 -v $(pwd)/nginx.conf:/etc/nginx/nginx.conf:ro nginx:alpine
```

4. **Verify deployment**:
```bash
curl http://localhost/health/stats
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

Benchmarks should be run on a distributed multi-machine cluster to obtain accurate results. Running locally will not reflect real-world performance.

### Prerequisites for Benchmarks

1. Deploy a cluster across multiple machines (4 nodes recommended)
2. Ensure curl and jq are installed
3. Have network access to Project Gutenberg (for ingestion tests)

### Running Ingestion Stress Test

```bash
chmod +x ingest_100_test.sh
./ingest_100_test.sh
```

This script:
- Ingests 100 books sequentially from Project Gutenberg
- Applies 0.2s delay between requests to avoid rate limiting
- Measures total ingestion time

### Monitor Cluster Statistics

```bash
watch -n 1 'curl -s http://<load-balancer-ip>/health/stats | jq'
```

### Expected Benchmark Results

| Metric | Expected Range |
|--------|----------------|
| Ingestion rate | 5-10 docs/second (network dependent) |
| Search latency (p50) | < 50ms |
| Task completion | < 2s per document |
| Partition resync | < 5s for 1000 entries |

## Demonstration Video

YouTube Link: [https://youtu.be/x4k3mQ5bvsc?si=kSWDGz84vtQnTOzL]

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

### Command Line Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `--bind <addr:port>` | Yes | Gossip protocol bind address (HTTP API runs on port + 1000) |
| `--seed <addr:port>` | No* | Address of an existing cluster node to join. *Required when joining an existing cluster; omit only for the first (seed) node |

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
│   ├── Cargo.toml           # UI dependencies
│   └── src/
│       ├── main.rs          # UI server
│       └── ui.html          # Web interface
├── docker-compose.yml       # Nginx load balancer deployment
├── nginx.conf               # Load balancer config
├── Cargo.toml               # Rust dependencies
├── ingest_100_test.sh       # Ingestion stress test
├── test_cluster.sh          # Integration tests
└── test_executor.sh         # Executor tests
```

## License

This project was developed as part of a Big Data course.
