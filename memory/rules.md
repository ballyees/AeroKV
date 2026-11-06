# Rules & Engineering Standards: AeroKV

## 1. Code Style & Architecture
- Language: Rust 2021 Edition.
- Architecture: Shared-Nothing (Thread-per-Core). Zero global `Mutex` / `RwLock` locks in the data plane.
- Communications: Dual TCP protocols — high-throughput RESP binary engine (`:6380`) and standard HTTP/1.1 & HTTP/2 (`h2c`) REST engine (`:16379`).
- Code cleanliness: Zero tolerance for dead or unused code. All unused imports, variables, and commented-out code must be removed.
- Testing: Thorough unit and integration tests across sharding, active/lazy TTL sweeps, memory bounds, auth, and protocol switching.

## 2. Dependencies & Tooling
- Package Manager: `cargo`.
- Compiler Profile: Maximize speed (`opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "abort"`).

