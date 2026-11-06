# AeroKV ⚡

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg)](https://www.rust-lang.org/)
[![Throughput](https://img.shields.io/badge/Throughput->1.8M%20ops%2Fsec-brightgreen.svg)]()
[![C100k](https://img.shields.io/badge/Concurrency-100k%20Ready-blueviolet.svg)]()

**AeroKV** is an ultra-high-throughput, sharded in-memory key-value database built in Rust. It delivers **sub-millisecond latency** and scales past **1,800,000 ops/sec** by pairing a **Shared-Nothing (Thread-per-Core)** architecture with dual-protocol support:

1. **Native RESP2/RESP3 Engine (`:6380`):** 100% compatible with existing Redis/Dragonfly clients (`redis-cli`, Jedis, go-redis, redis-py).
2. **Dual-Stack HTTP/1.1 & HTTP/2 Engine (`:16379` / `:8080`):** High-speed REST interface with native HTTP/2 stream multiplexing and HTTP/1.1 keep-alive.

---

## 🚀 Performance Benchmarks

Measured on Apple Silicon (10-core hardware, `redis-benchmark` with $n=500{,}000$ operations):

### 1. Pipelined Throughput ($C=100$, $P=32$)

| Database | GET Throughput | SET Throughput | INCR Throughput | Median Latency ($p50$) |
| :--- | :--- | :--- | :--- | :--- |
| **AeroKV** ⚡ | **1,845,018 ops/sec** | **1,748,251 ops/sec** | **1,792,114 ops/sec** | **0.99 - 1.20 ms** |
| **Redis** (Native) | 1,712,328 ops/sec | 1,351,351 ops/sec | 1,655,629 ops/sec | 1.56 - 1.80 ms |
| **Dragonfly** (Docker) | 1,333,333 ops/sec | 1,285,347 ops/sec | 1,449,275 ops/sec | 1.73 - 1.83 ms |

### 2. High-Capacity Memory Efficiency (C100k Ready)
* **Adaptive Dynamic Buffering:** 100,000 idle client connections consume only **~800 MB RAM** instead of 12.8 GB.
* **Large Payloads Supported:** Seamlessly streams arbitrary payload sizes (**64 KB, 1 MB, 10 MB+**) at wire speed (~2.8 GB/sec memory throughput).

---

## 🏗️ Architecture

```
                               ┌────────────────────────────────────────────────┐
                               │                 AeroKV Engine                  │
                               └────────────────────────────────────────────────┘
                                                       │
               ┌───────────────────────────────────────┴───────────────────────────────────────┐
               ▼                                                                               ▼
     RESP2/RESP3 TCP Engine (:6380)                                               Dual-Stack HTTP Engine (:16379 / :8080)
  (redis-cli, go-redis, redis-py)                                                (HTTP/1.1 & HTTP/2 Cleartext h2c over TCP)
               │                                                                               │
               └───────────────────────────────────────┬───────────────────────────────────────┘
                                                       │  FxHash(key) % N
                                                       ▼
                      ┌─────────────────────────────────────────────────────────────────┐
                      │                 Shared-Nothing Lock-Free Shards                 │
                      ├──────────────────┬──────────────────┬───────────────────────────┤
                      │ Core 0: Shard 0  │ Core 1: Shard 1  │ ... Core N: Shard N       │
                      │ • FxHashMap      │ • FxHashMap      │     • FxHashMap           │
                      │ • TTL Timeline   │ • TTL Timeline   │     • TTL Timeline        │
                      │ • Sampled LRU    │ • Sampled LRU    │     • Sampled LRU         │
                      └──────────────────┴──────────────────┴───────────────────────────┘
```

* **Zero Lock Contention:** Individual CPU cores own their storage slice. No shared `Mutex` or `RwLock` in the data path.
* **Jemalloc Integration:** Zero heap arena contention across threads under heavy concurrency.
* **Cooperative Batch Draining:** Processes up to 128 commands per shard queue in a tight in-memory loop.

---

## 📦 Quick Start

### 1. Build and Run from Source

Requires **Rust 1.75+**:

```bash
# Clone the repository
git clone https://github.com/kittinanprompao/AeroKV.git
cd AeroKV

# Build optimized release binary
cargo build --release

# Start the server
./target/release/aerokv-server
```

Default listening ports:
* **RESP Protocol:** `0.0.0.0:6380`
* **HTTP/1.1 & HTTP/2 REST:** `0.0.0.0:16379`

### 2. Environment Variables

| Variable | Default | Description |
| :--- | :--- | :--- |
| `AEROKV_TOKEN` (or `AUTH_TOKEN`, `AEROKV_PASSWORD`) | *(empty)* | Optional authorization token / password (supports comma-separated list e.g. `"tok1,tok2,tok3"`) |
| `PORT` or `AEROKV_PORT` | `16379` (or `8080`) | Dual-stack HTTP/1.1 & HTTP/2 (TCP) port |
| `AEROKV_RESP_PORT` | `6380` | High-throughput RESP TCP port |
| `AEROKV_MAX_MEM_MB` | Auto-detected | Hard memory ceiling (MB). Auto-detects container cgroup v2/v1 or host RAM (80% allocation) when unset |
| `AEROKV_MEM_RATIO` | `0.80` | Memory allocation ratio (0.10 - 1.00) used when auto-detecting container limits |
| `AEROKV_CORES` | Hardware count | Number of pinned CPU shard worker threads |

---

## 🔒 Authorization / Multiple Key Rotation

AeroKV allows configuring **single or multiple comma-separated tokens** via `AEROKV_TOKEN`:

```bash
# Multi-token authorization (ideal for zero-downtime key rotation & multi-tenant microservices)
export AEROKV_TOKEN="mysecrettoken123,mysecrettoken1234,mysecrettoken12345"
```

Any client presenting **any** of the valid tokens will be granted access using constant-time comparison (`subtle::ConstantTimeEq`).

### 1. In RESP Protocol (`redis-cli`, Jedis, go-redis)
Clients authenticate via standard Redis `AUTH` command with any configured token:
```bash
# Using token 1
redis-cli -p 6380 -a mysecrettoken123 GET user:1001

# Using token 2
redis-cli -p 6380 -a mysecrettoken1234 GET user:1001
```

### 2. In HTTP REST API (`curl`, fetch)
Clients authenticate either via Bearer Token or custom header:
```bash
# Using token 1
curl -H "Authorization: Bearer mysecrettoken123" https://aerokv-url/v1/kv/mykey

# Using token 3
curl -H "X-AeroKV-Token: mysecrettoken12345" https://aerokv-url/v1/kv/mykey
```

---

## 🔌 Client Usage

### 1. Using Standard Redis Clients (`redis-cli`, Python, Go, Node.js)

Connect directly to port `6380`:

```bash
# Test connection
redis-cli -p 6380 PING
# Output: PONG

# Set and Get
redis-cli -p 6380 SET user:1001 "Alice" EX 60
redis-cli -p 6380 GET user:1001
# Output: "Alice"

# Atomic counter
redis-cli -p 6380 INCR counter
# Output: 1

# Check TTL
redis-cli -p 6380 TTL user:1001
# Output: 58
```

### 2. Using the Native AeroKV CLI

```bash
# Interactive REPL
cargo run --release -p aerokv-cli -- --port 16379

# One-shot command
cargo run --release -p aerokv-cli -- --port 16379 SET mykey "hello"
cargo run --release -p aerokv-cli -- --port 16379 GET mykey
```

### 3. Using HTTP REST API

```bash
# Set key
curl -X PUT http://localhost:16379/kv/mykey -d "custom-value"

# Get key
curl http://localhost:16379/kv/mykey

# Delete key
curl -X DELETE http://localhost:16379/kv/mykey

# Database statistics
curl http://localhost:16379/stats
```

### 4. Client Key Prefix Support (Namespaces & Multi-Tenancy)

AeroKV client libraries support key prefixes to isolate environments or tenants automatically:

**Rust (`core-client`):**
```rust
use core_client::AeroClient;

// Connect with initial prefix
let client = AeroClient::connect_with_prefix("127.0.0.1", 16379, Some("secret"), Some("tenant_a:")).await?;
client.set("user", Bytes::from("Alice"), None).await?; // Stored on server as "tenant_a:user"

// Or create a scoped child client sharing the exact same connection pool
let tenant_b = client.with_prefix("tenant_b:");
tenant_b.set("user", Bytes::from("Bob"), None).await?; // Stored on server as "tenant_b:user"
```

**Python (`clients/python`):**
```python
from aerokv import Client

client = await Client.connect("127.0.0.1", 16379, token="secret", prefix="tenant_a:")
await client.set("user", b"Alice")  # Stored on server as "tenant_a:user"

# Scoped child handle
tenant_b = client.with_prefix("tenant_b:")
await tenant_b.set("user", b"Bob")
```

**Go (`clients/go`):**
```go
client, err := aerokv.NewClientWithPrefix("127.0.0.1", 16379, "secret", "tenant_a:")
err = client.Set("user", []byte("Alice"), 0)
```

**Interactive CLI:**
```bash
cargo run --release -p aerokv-cli -- --port 16379 --prefix "tenant_a:"
```

---

## ☁️ Deployment

### Option A: Google Compute Engine / Bare-Metal VM (Recommended for 1.8M ops/sec)

Run the automated system tuning script:

```bash
sudo bash deploy/gce-setup.sh
sudo systemctl enable --now aerokv
```

This configures Linux kernel `sysctl` for C100k concurrency:
* `fs.file-max = 2097152`
* `net.core.somaxconn = 65535`
* `net.ipv4.tcp_tw_reuse = 1`

### Option B: Google Cloud Run (HTTP Serverless Cache)

Use the dedicated [`Dockerfile.cloudrun`](file:///Users/kittinanprompao/Desktop/gh/AeroKV/Dockerfile.cloudrun):

```bash
# 1. Build & Push to Google Cloud Artifact Registry / GCR
gcloud builds submit --tag gcr.io/$PROJECT_ID/aerokv:latest -f Dockerfile.cloudrun .

# 2. Deploy with single-instance pinning (prevents split-brain cache)
gcloud run deploy aerokv \
  --image gcr.io/$PROJECT_ID/aerokv:latest \
  --platform managed \
  --region us-central1 \
  --allow-unauthenticated \
  --memory 2Gi \
  --cpu 2 \
  --min-instances 1 \
  --max-instances 1 \
  --no-cpu-throttling \
  --set-env-vars="PORT=8080,AEROKV_MAX_MEM_MB=1536"
```

---

## 🧪 Testing & Benchmarks

```bash
# Run unit & end-to-end integration tests across all crates
cargo test --workspace

# Run full multi-protocol benchmark suite (RESP P=1, RESP P=16, Redis, Dragonfly, HTTP/1.1, HTTP/2)
cargo run --release -p aerokv-bench -- --protocol all --total 100000 --concurrency 50

# Benchmark RESP engine only (automatically probes Redis on :6379 and Dragonfly on :6381 if running)
cargo run --release -p aerokv-bench -- --protocol resp --total 100000 --concurrency 50

# Benchmark HTTP engine only (HTTP/1.1 TCP keep-alive and HTTP/2 cleartext h2c)
cargo run --release -p aerokv-bench -- --protocol http --total 100000 --concurrency 50

# Run standard Redis benchmark tool against AeroKV RESP port
redis-benchmark -p 6380 -t set,get,incr -n 500000 -c 100 -P 32 -q
```

---

## 📜 License

Licensed under the [MIT License](LICENSE).

