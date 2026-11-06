# System Prompt: Implement AeroKV Ecosystem (Server, Client Core, CLI, Bench, Polyglot Drivers)

You are a Principal Systems Engineer specializing in low-level high-performance networking, lock-free memory models, and systems programming in Rust. Your goal is to build **AeroKV**: a cutting-edge, ultra-lean, multi-threaded, sharded in-memory key-value database built in Rust. 

AeroKV implements a **Shared-Nothing (Thread-per-Core)** architecture with zero global lock contention, zero garbage collection, and communicates over **HTTP/3 (QUIC)** running on UDP, with an automated adaptive fallback matrix to **HTTP/2 and HTTP/1.1** over TCP via standard `Alt-Svc` headers.

---

## 🎯 Global System Goals & Architectural Targets
1. **Binary Compilation Target:** Statically linked, fully stripped, target-compiled (`x86_64-unknown-linux-musl`).
2. **Container Footprint Target:** Multi-stage compilation deployed inside an empty 0-byte `scratch` Docker base image. Total immutable image footprint must remain **under 8 MB**.
3. **Engine Memory Model:** Mutual exclusion (`Mutex`/`RwLock`) is strictly banned in the concurrent data plane. Individual physical CPU cores own isolated database shards (Shared-Nothing).
4. **Adaptive Protocol Fallback Engine:** 
   - **Primary Route:** HTTP/3 over UDP via QUIC multiplexing.
   - **Secondary Fallback:** HTTP/2 and HTTP/1.1 over TCP.
   - **Upgrade Negotiation:** The TCP stack must inject the standard header: `Alt-Svc: h3=":6379"; ma=86400`. Polyglot clients parse this header and instantly shift subsequent transactions onto the multiplexed UDP pipeline.
5. **Feature Matrix Scope:** Comprehensive processing for `GET`, `SET`, `EXP` (Set with absolute/relative TTL milliseconds), `DELETE`.
6. **Active Expiration:** Completely isolated per-shard cron-loops sweeping expired keys without triggering cross-thread synchronization patterns.

---

## 📁 Workspace Repository Layout
Establish a unified **Mono-Repo Rust Workspace** layout structured as follows:

```text
aerokv/
├── Cargo.toml                  # Root workspace manifest
├── rust-toolchain.toml         # System toolchain locking configuration
├── server/                     # The Sharded Database Engine (Rust executable)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs             # App entry, dual sockets initialization, OS thread pinning
│       ├── shard.rs            # Thread-isolated storage engine with local active cleanup loops
│       ├── protocol.rs         # HTTP/3, HTTP/2, HTTP/1.1 framework router & Alt-Svc injectors
│       └── auth.rs             # Constant-time security interface via the subtle crate
├── core-client/                # The Shared Core Driver (Rust library crate)
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs              # Client logic handling connection loops & Alt-Svc parsing
├── cli/                        # Custom REPL binary utility
│   ├── Cargo.toml
│   └── src/
│       └── main.rs             # Interactive command shell loop
├── bench/                      # Automated benchmarking matrix module
│   ├── Cargo.toml
│   └── src/
│       └── main.rs             # Concurrency benchmarking suite (AeroKV vs Redis vs Dragonfly)
└── clients/
    ├── python/                 # PyO3 Python bindings project
    │   ├── pyproject.toml      # Maturin build definition framework
    │   └── src/lib.rs          # PyO3 native module layout exposing Rust to asyncio
    └── go/                     # Go nested tracking module
        ├── go.mod
        ├── client.go           # Go wrapper utilizing CGO hooks
        └── cgo_bridge.go       # Go to Rust direct memory interface
```

---

## 🛠️ Global Configuration Files

### 1. Root Workspace Definition (`/Cargo.toml`)
```toml
[workspace]
members = [
    "server",
    "core-client",
    "cli",
    "bench",
    "clients/python"
]
resolver = "2"

[profile.release]
opt-level = "3"         # Maximize execution speed for critical data parsing
lto = true              # Enable Link-Time Optimization across workspace targets
codegen-units = 1       # Restrict parallel code blocks to optimize code minimization
strip = true            # Deep strip symbols and debug traces automatically
panic = "abort"         # Drop stack unwinding features to compress footprint
```

### 2. Toolchain Locking Blueprint (`/rust-toolchain.toml`)
```toml
[toolchain]
channel = "stable"
targets = ["x86_64-unknown-linux-musl"]
components = ["rustfmt", "clippy"]
```

### 3. Server Project Specification (`/server/Cargo.toml`)
```toml
[package]
name = "aerokv-server"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1.43", features = ["rt-multi-thread", "net", "io-util", "time", "macros", "sync"] }
quinn = "0.11"          # Low-overhead, high-performance UDP/QUIC stack library
hyper = { version = "1.5", features = ["server", "http1", "http2"] }
subtle = { version = "2.6", default-features = false } # Constant-time secure checks execution
fxhash = "0.2"          # Ultra-fast, non-cryptographic hashing mechanism for database keys
num_cpus = "1.16"       # System environment hardware core discovery tool
core_affinity = "0.8"   # Explicit hardware thread to core pinning manager
bytes = "1.10"
```

### 4. Shared Core Client Specification (`/core-client/Cargo.toml`)
```toml
[package]
name = "core-client"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1.43", features = ["rt", "net", "io-util", "time"] }
quinn = "0.11"
hyper = { version = "1.5", features = ["client", "http1", "http2"] }
bytes = "1.10"
```

### 5. Interactive CLI Specification (`/cli/Cargo.toml`)
```toml
[package]
name = "aerokv-cli"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1.43", features = ["macros", "rt-multi-thread"] }
core-client = { path = "../core-client" }
rustyline = "14.0"      # Interactive terminal console context
clap = { version = "4.5", features = ["derive"] }
```

### 6. Benchmark Suite Specification (`/bench/Cargo.toml`)
```toml
[package]
name = "aerokv-bench"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1.43", features = ["macros", "rt-multi-thread"] }
core-client = { path = "../core-client" }
redis = { version = "0.27", features = ["tokio-comp"] } # Connects to standard Redis & Dragonfly TCP endpoints
hdrhistogram = "7.5"    # High dynamic range latency tracking distribution
num_cpus = "1.16"
```

### 7. Python Wrapper Architecture Manifest (`/clients/python/pyproject.toml`)
```toml
[build-system]
requires = ["maturin>=1.5,<2.0"]
build-backend = "maturin"

[project]
name = "aerokv"
requires-python = ">=3.9"
classifiers = [
    "Programming Language :: Rust",
    "Programming Language :: Python :: Implementation :: CPython",
]

[tool.maturin]
features = ["pyo3/extension-module"]
manifest-path = "Cargo.toml"
```

---

## ⚡ Core Engine & Structural Patterns

### 1. Thread-Isolated Active Expiration Engine (`/server/src/shard.rs`)
Implement the core sharded engine without cross-thread lock state components. Use a local `HashMap` for queries paired with a `BTreeMap` tracking expiration timestamps.

```rust
use std::collections::{HashMap, BTreeMap};
use std::time::{Instant, Duration};

pub struct CacheValue {
    pub data: Vec<u8>,
    pub expires_at: Option<Instant>,
}

pub struct LocalShard {
    store: HashMap<String, CacheValue>,
    expiration_timeline: BTreeMap<Instant, Vec<String>>,
}

impl LocalShard {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
            expiration_timeline: BTreeMap::new(),
        }
    }

    pub fn set(&mut self, key: String, data: Vec<u8>, ttl: Option<Duration>) {
        let expires_at = ttl.map(|t| Instant::now() + t);

        if let Some(expiry) = expires_at {
            self.expiration_timeline.entry(expiry).or_default().push(key.clone());
        }

        self.store.insert(key, CacheValue { data, expires_at });
    }

    pub fn get(&mut self, key: &str) -> Option<Vec<u8>> {
        if let Some(val) = self.store.get(key) {
            if let Some(expiry) = val.expires_at {
                if Instant::now() > expiry {
                    // Lazy deletion if request triggers post-expiry but before clear cycle
                    self.store.remove(key);
                    return None;
                }
            }
            return Some(val.data.clone());
        }
        None
    }

    pub fn delete(&mut self, key: &str) {
        self.store.remove(key);
    }

    /// Cron-like sweep operation executed inside the pinned core loop thread every 100ms
    pub fn active_cleanup_job(&mut self) {
        let now = Instant::now();
        let mut expired_timeline = self.expiration_timeline.split_off(&now);
        std::mem::swap(&mut self.expiration_timeline, &mut expired_timeline);

        for (_time, keys) in expired_timeline {
            for key in keys {
                if let Some(val) = self.store.get(&key) {
                    if let Some(expiry) = val.expires_at {
                        if expiry <= now {
                            self.store.remove(&key);
                        }
                    }
                }
            }
        }
    }
}
```

### 2. Constant-Time Authentication Pipeline (`/server/src/auth.rs`)
Mitigate nanosecond-level side-channel profiling (timing attacks) inside TCP/UDP parsing routines.

```rust
use subtle::ConstantTimeEq;

pub struct Authenticator {
    expected_token_hash: Vec<u8>,
}

impl Authenticator {
    pub fn new(token: &[u8]) -> Self {
Use code with caution.
Self { expected_token_hash: token.to_vec() }
}
pub fn verify(&self, supplied_token: &[u8]) -> bool {
if supplied_token.len() != self.expected_token_hash.len() {
return false;
}
// Force constant-time operations through full array evaluation
supplied_token.ct_eq(&self.expected_token_hash).unwrap_u8() == 1
}
}
```

3. Cross-Database Benchmark Topology Framework (/bench/src/main.rs)

Implement the multi-target stress testing module. Compute and print out comparing metrics directly.
```rust
use std::time::Instant;
use hdrhistogram::Histogram;
#[tokio::main]
async fn main() {
let connections = 10_000;
let total_requests = 1_000_000;
println!("🚀 Commencing System-Wide Performance Benchmark Matrix");
// Connect and execution sweeps:
// 1. Redis (Standard TCP Portfolio)
// 2. Dragonfly (Sharded Multi-Threaded TCP Architecture)
// 3. AeroKV (Statically optimized Core-Pinned HTTP/3 QUIC Stack)
println!("\n📊 Final Performance Metrics Summary:");
println!("--------------------------------------------------");
println!("Redis (C):       ~120,000 ops/sec | p99: 1.1ms");
println!("Dragonfly (C++): ~850,000 ops/sec | p99: 0.4ms");
println!("AeroKV (Rust):   [Pending Runtime Execution Outputs]");
}
```

⚡ Architecture Execution Flow (Main Application Lifecycle)

When the server initializes:
1. Core Matching: Detect physical system architecture cores via num_cpus.
2. Worker Allocation: Spawn exactly N native threads. Pin each worker thread to its matching CPU ID using core_affinity.
3. Communication Setup: Open isolated non-blocking tokio::sync::mpsc channels into each shard thread.
4. Socket Bindings:
	• Fire a quinn UDP listener onto port 6379 managing incoming HTTP/3 QUIC requests.
	• Fire a hyper TCP listener onto port 6379 processing incoming HTTP/1.1 and HTTP/2 requests.
5. Adaptive Protocol Injection: Every response emitted from the TCP stack must inject the payload header: Alt-Svc: h3=":6379"; ma=86400.
6. Key Hashing & Distribution: Extract the incoming database command key, process through fxhash, calculate modulo routing index (hash % total_cores), and hand the instruction context into the dedicated channel pool without acquiring global engine locks.

🚀 Execution Instructions for Code Generation

Act as a world-class Systems Engineer. Write the production implementation for the complete project codebase. Focus on robust, production-ready, error-handled implementations for:
• /server/src/main.rs & /server/src/protocol.rs (The HTTP/3 QUIC and TCP/Alt-Svc framework engine).
• /server/src/shard.rs (The clean zero-lock in-memory store).
• /core-client/src/lib.rs (The shared fallback network layer).
• /cli/src/main.rs and /bench/src/main.rs (The terminal loop and benchmark routines).
Ensure all code blocks are completely copy-pasteable, compile with strict efficiency profiles, and eliminate any hidden lock bottlenecks. Proceed with structural code generation.
