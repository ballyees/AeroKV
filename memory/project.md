# Project Architectural Blueprint: AeroKV

## 1. Project Overview & Business Domain
AeroKV is an ultra-lean, multi-threaded, sharded in-memory key-value database built in Rust with thread-per-core isolation, lock-free data planes, and dual TCP engines: ultra-high-throughput RESP and high-concurrency HTTP/1.1 & HTTP/2 (`h2c`).

## 2. Tech Architecture & Runtime Stack
- Language: Rust 2021 Edition
- Primary Stack: `tokio`, `hyper 1.5`, `hyper-util`, `h2`, `bytes`, `fxhash`, `core_affinity`, `num_cpus`, `subtle`, `tikv-jemallocator`.
- Target: Statically linked `x86_64-unknown-linux-musl`, deployed inside scratch Docker container (< 8MB).

## 3. Directory Layout & Module Ownership
- `server/`: Pinned core threads, HTTP/1.1 & HTTP/2 TCP listener, high-speed RESP TCP listener, LocalShard engine, constant-time auth, cgroups auto-memory detection.
- `core-client/`: Rust client with pure HTTP connection pooling and C-ABI FFI.
- `cli/`: Terminal REPL using `rustyline` and `clap`.
- `bench/`: Latency and throughput benchmarking suite using `hdrhistogram`, testing RESP, HTTP/1.1, HTTP/2, and Redis.
- `clients/python/`: PyO3 asyncio driver.
- `clients/go/`: CGO wrapper for Go.

## 4. Key Data Models & Schema Mappings
- Key-Value entries: `Bytes` key, `CacheValue { data: Bytes, expires_at: Option<Instant>, last_accessed_tick: u64 }`.
- Active Expiration: `BTreeMap<Instant, Vec<Bytes>>`.
- Eviction: Sampled-LRU.

## 5. System Features & Core Mechanics
- Commands: `GET`, `SET`, `EXP`, `SETNX`, `DELETE`, `INCR`, `DECR`, `TTL`, `EXISTS`, `MGET`, `MSET`, `PING`, `STATS`, `FLUSH`.
- Active Cleanup: Per-shard loop sweeping expired keys every 100ms.
- Communication: Dual TCP listeners for RESP (6380) and HTTP/1.1 & HTTP/2 (16379/8080).

## 6. Authentication, Authorization & Security
- `subtle::ConstantTimeEq` constant-time token comparison.

## 7. REST API Endpoints Reference
- `GET /kv/{key}`, `PUT /kv/{key}`, `DELETE /kv/{key}`, `POST /kv/{key}/incr`, `GET /kv/{key}/ttl`, `GET /kv/{key}/exists`, `POST /mget`, `POST /mset`, `GET /ping`, `GET /stats`, `POST /flush`.

## 8. WebSocket & Asynchronous Event Protocol Spec
- Pure TCP with binary streams (HTTP/2) and pipelined multiplexing (RESP).

## 9. Third-Party Integrations & External Services
- None (self-contained in-memory database).

## 10. Restricted & Auto-Generated Files (Do Not Edit)
- None.

## 11. Architectural Invariants & ADRs
- No `Mutex` or `RwLock` across the data plane.
- Communications to shards use bounded `mpsc` and `oneshot` channels.

## 12. Testing Strategy & Mocking Guidelines
- Unit testing with `cargo test`.
- Real network integration testing for RESP and HTTP protocols.

## 13. Running Local Infrastructure
- Port 6380 for RESP TCP, Port 16379 for HTTP/1.1 & HTTP/2 TCP.

## 14. Containerized & Production Cloud Deployments
- Multi-stage Dockerfile to scratch base image (< 8MB).

## 15. Known Technical Debt & Fragile Areas
- None (greenfield project).

