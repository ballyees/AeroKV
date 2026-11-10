# Operating Workflow

1. **Precedence, scope, and token efficiency:** Follow explicit user requirements before this workflow. Work only in this workspace. Before executing work, perform strict scope definition by explicitly stating the Goal, In-Scope boundaries, and Out-of-Scope limitations. Eliminate conversational filler, status commentary, and text preambles; execute the next required development tool or terminal command immediately once aligned. Write concise, accurate, professional output; state assumptions and tradeoffs, choose the simplest sufficient solution, and request clarification when ambiguity blocks a safe decision.
   * **Token & Context Optimization (Maximize Performance):** Never dump large files, full logs, or verbose stdout into context. Use targeted line-range reads, ripgrep/AST filters, or executable scripts in `memory/script/` that emit compact, structured summaries (exit codes, JSON metrics, or `file:line` references). Offload repetitive reasoning, parsing, and batch operations to scripts rather than LLM token generation.
2. **Context, memory, and skill registry:** At session initialization (first chat turn), load memory sequentially: `memory/rules.md` → `memory/tools.md` → `memory/refs.md` → select and read the **specific past `session.md`** referenced in `refs.md` relevant to the active task → `memory/project.md` → **only inspect files and modules directly relevant to the in-scope task**. Do not read the entire codebase during routine tasks. Detect the ISO date; create and read the daily session log using the Session Template below if absent.
   * **On-Demand Deep Research (`memory/research/` — Lazy Loaded, Not Required at Init):** Do **not** load `memory/research/*` automatically at session startup to preserve context tokens. Read `memory/research/libs.md` or `memory/research/{topic}.md` **only when relevant** to the active task (e.g., evaluating libraries, CVEs, architectural tradeoffs, or domain algorithms). When reading a research file, inspect the top `TL;DR` summary lines first before reading the full document. Whenever you conduct deep research on a domain/algorithm/architecture/security model, persist the synthesis to `memory/research/{topic}.md`, and whenever you evaluate packages, CVEs, or third-party dependencies, persist the evaluation to `memory/research/libs.md` using the templates below.
   * **On-Demand Root Cause Analysis (`memory/rca/{topic}.md` — Lazy Loaded, Triggered on Hard Debugging/RCA/Security Incidents):** Do **not** load `memory/rca/*` automatically at session startup. When the user requests finding a root cause, investigating an anomaly/vulnerability, or debugging a hard/extremely hard bug (e.g., race conditions, memory/resource leaks, tail-latency spikes, state corruption, or non-deterministic failures), check `memory/refs.md` for prior related RCAs (reading the top `TL;DR` lines first) and record the full forensic investigation in `memory/rca/{topic}.md` using the **Root Cause Analysis Template** below.
   * **Missing `memory/project.md`:** If `memory/project.md` is missing, perform a one-time comprehensive codebase analysis to generate it using the **Project Architecture Template** below (covering all 15 architectural and operational sections).
   * **Missing `memory/rules.md`:** If `memory/rules.md` is missing, initialize and populate it with code style, coding conventions, project conventions, basic guidelines, quality criteria, and coding standards.
   * **Missing `memory/tools.md`, `memory/research/`, `memory/rca/`, & Script Directories:** If `memory/tools.md` is missing, initialize it using the **Tool & Skill Library Template** below. Ensure `memory/script/` (persistent automation), `memory/scratchpad/` (temporary experiments), `memory/research/` (with `memory/research/libs.md`), and `memory/rca/` exist, and verify `memory/scratchpad/` is listed in `.gitignore`.
   * **Cascading Multi-Repo Updates:** In multi-repo or nested workspace layouts (e.g., `root/project1/project_frontend/`), when modifying code within specific sub-projects, **you must update memory in all relative project and component memory directories** (e.g., if working in `project1` and modifying `project1/project_frontend`, update both `project1/memory/` AND `project1/project_frontend/memory/`). Append progress, changes, blockers, and task status across all touched memory locations after codebase changes. Update `memory/refs.md` after daily-log, `memory/research/`, or `memory/rca/` changes, `memory/tools.md` when adding/updating reusable scripts, and `memory/project.md` after high-level architectural changes.
3. **Scratchpad vs. Reusable Script Lifecycle (`memory/scratchpad/` & `memory/script/`):**
   * **Consult `memory/tools.md` First:** Before writing any ad-hoc command or script, check `memory/tools.md` to reuse existing automation in `memory/script/`.
   * **`memory/scratchpad/` (Ephemeral Sandbox — Git-Ignored):** Write all temporary scripts (Python, Shell, TypeScript/Bun), one-off data probes, hypothesis tests, bug/anomaly/exploit reproducers, fault-injection harnesses, and draft proofs strictly inside `memory/scratchpad/`. Ensure `memory/scratchpad/` is present in `.gitignore` so temporary artifacts never pollute version control.
   * **`memory/script/` (Persistent Pipelines & Token-Saving Tools):** Store reusable pipelines, benchmark harnesses, validation runners, CVE/security auditors, regression reproducers, code generators, and automation tools in `memory/script/`. Design every script to accept CLI arguments and output compact, token-efficient summaries. Whenever a script in `memory/scratchpad/` proves reusable, clean it up, move it to `memory/script/`, and register it in `memory/tools.md`.
4. **Pre-plan alignment gate (Observability, Benchmark, Validation):** Before creating any plan or executing tools, files, commands, or code changes, **you must explicitly ask and align with the user every single time without exception** on the following three pillars to ensure absolute correctness, identical definitions, and complete mutual understanding. **Never skip or assume this alignment under any circumstances, even if prior context exists in the prompt or `memory/`:**
   * **Observability:** How system state, execution flow, security events, and failures will be observed and inspected (e.g., structured logs, metrics, traces, debug outputs, health endpoints, audit trails, or UI/DOM state inspection).
   * **Benchmark (Normal + Non-Normal / Adversarial Cases & Performance Budgets):** How performance, resource ceilings, and resilience baselines vs. target thresholds will be measured. **Benchmarks must explicitly cover non-normal conditions, not just average/happy paths:**
     * **Tail Latency:** Measure `p95`, `p99`, and `p99.9` latency distributions alongside `p50` to expose jitter, GC pauses, and queuing stalls.
     * **Anomalies:** Test behavior under unexpected spikes, clock skew, partial network/IO failures, corrupted/malformed payloads, and out-of-order events.
     * **Edge Cases:** Test boundary values (`0`, `1`, `MAX_INT`, empty collections, deep nesting, high-cardinality keys, Unicode/binary extremes).
     * **Worst-Case Complexity, Saturation & Abuse:** Test pathological or adversarial inputs that trigger worst-case algorithmic time/space complexity (e.g., hash collisions, ReDoS regex backtracking, unindexed scans, payload amplification), lock contention, cold starts, and memory/CPU saturation.
   * **Validation (Correctness, Security & Maintainability):** The exact acceptance criteria, deterministic commands, formal invariants, security/CVE checks, and pass/fail thresholds required to prove correctness and safety across both normal and non-normal regimes.
5. **Plan, formal reasoning, root cause analysis, and implement:** After aligning on Observability, Benchmark, and Validation, record a verification plan with objective, edge cases, security/performance impact, and module impact. Prioritize high-risk mitigation: identify and resolve the highest-risk unknowns, core logic dependencies, security threat vectors, or architectural uncertainties before working on trivial tasks (consulting or generating `memory/research/{topic}.md` for domain/algorithmic investigation, or `memory/rca/{topic}.md` for root-cause debugging).
   * **Abstraction, Hypothesis, Lean Proof & Testing Protocol:** When requested by the user, when debugging hard root causes (`memory/rca/{topic}.md`), or when modifying complex algorithms, security/auth boundaries, concurrency, state machines, or core business invariants, construct and verify:
     1. **Abstraction:** Distill the target logic or failure mechanism into a minimal, implementation-agnostic model defining types, state space, trust boundaries, inputs/outputs, and transition rules.
     2. **Hypothesis:** State explicit, falsifiable claims, pre-conditions, post-conditions, failure triggers, time/space complexity bounds, and safety/liveness/security invariants.
     3. **Lean Proof:** Formulate a concise, step-by-step formal logical/mathematical proof (or executable Lean 4 specification/proof in `memory/scratchpad/` when applicable) demonstrating that the invariants hold (or proving why the bug/vulnerability occurred and why the fix eliminates the reachable failure state) across base cases, state transitions, and worst-case boundary conditions.
     4. **Testing:** Translate the abstraction boundaries, hypothesis claims, and proof obligations directly into automated unit, property-based, security/fuzzing, regression, adversarial benchmark, and edge-case tests (executing temporary hypothesis/reproducer harnesses in `memory/scratchpad/` and persistent test/benchmark suites via `memory/script/`).
   * For core-business-logic changes, obtain user confirmation through the Walkthrough Artifact. Implement sequentially; preserve local style; avoid unrequested features, abstractions, refactors, and cleanup. Remove only unused code caused by the change.
6. **Engineering, performance, security, CVE, and maintenance standards:** Adhere primarily to project conventions defined in `memory/rules.md`. If `memory/rules.md` is not provided or lacks specific rules, enforce the following standards:
   * **Clean & Maintainable Architecture:** Keep code simple, modular, strictly typed, and easily readable by humans while ensuring high technical rigor. Reserve entry points for orchestration and isolate business logic. Prevent circular dependencies, keep cyclomatic/cognitive complexity low, and ensure backward compatibility or explicit migration paths for schema/API changes.
   * **Performance Engineering:** Enforce bounded memory, queues, caches, and payload sizes (zero unbounded growth/OOM risks). Eliminate N+1 queries, blocking I/O on event loops, and redundant allocations/serializations on hot paths. Prefer $O(1)$ or $O(\log n)$ lookups over linear scans in critical loops.
   * **Security & Vulnerability Hardening (Zero-Trust / OWASP / CWE):** Validate and sanitize all external inputs at trust boundaries using strict schemas. Prevent SQL/NoSQL/Command Injection (use parameterized queries/safe APIs), XSS, CSRF, SSRF, Path Traversal, Prototype Pollution, Insecure Deserialization, and ReDoS. Enforce least-privilege RBAC/ABAC checks on every protected route/action. Never hardcode, log, or expose secrets, tokens, or PII in code, terminal output, or memory files.
   * **CVE, Dependency & Supply-Chain Hygiene:** Before installing, upgrading, or replacing any dependency, check `memory/research/libs.md` first, inspect existing built-in alternatives, verify maintainer reputation, license compatibility, maintenance activity, transitive dependency bloat, and CVE history (`bun audit` / `uv run pip-audit`). Zero tolerance for unmitigated Critical or High CVEs. Record the evaluation in `memory/research/libs.md`, pin exact versions, verify lockfile integrity, and use `bun` (FE/Node) or `uv` (Python).
   * **Mandatory Unused Code Deletion Standard (Frontend & Backend):** Zero tolerance for dead or unused code. All unused imports, variables, parameters, functions, dead methods, unreachable branches, and commented-out dead code blocks **MUST have their lines of code permanently deleted** (never commented out, never hidden behind suppressions). Enforce via `bun run clean:unused` on frontend and `uv run ruff check --fix --select F401` on backend. Prefer React TypeScript functional components with hooks, Node.js TypeScript services, and Tailwind; use NaiveUI, then DaisyUI, before Tailwind, and global color styling when required. Meet PEP 8 and ESLint.
7. **Safety and verification:** Practice empirical verification: never assume code works or is secure; every implementation step must be verified by executing the aligned **Observability**, **Benchmark** (including Tail Latency, Anomalies, Edge Cases, and Worst Cases), and **Validation** checks via actual terminal commands, compile checks, linting, security/CVE audits, and passing unit/property tests. Practice rigorous edge-case and adversarial testing: before advancing to the next workflow step, explicitly ask "What anomaly, tail-latency spike, security exploit, edge case, or worst-case input could prove this Hypothesis or Lean Proof wrong?" and actively test it in `memory/scratchpad/`. For root-cause or vulnerability fixes (`memory/rca/{topic}.md`), verify that the deterministic reproducer fails before the fix and passes after the fix. Never force-delete, wipe databases, or clean Git without explicit instruction. Check ports and prevent orphaned servers. For UI changes, inspect the page and capture a screenshot. Document failures, fix them, and re-run checks. For major changes, provide a Walkthrough Artifact with modified files, compile output, security/CVE status, normal vs. worst-case benchmark comparisons, and test results.

---

### Root Cause Analysis (RCA) & Vulnerability Forensics Template (`memory/rca/{topic}.md`)
*Read and write on-demand when requested to find a root cause, investigate a security vulnerability/CVE, or debug hard/extremely hard issues (do not auto-load at session start). Keep the `TL;DR` Forensic Header in the first 15 lines for low-token inspection:*

```markdown
# Root Cause Analysis: {Topic / Bug / CVE Identifier}
* **Date:** {YYYY-MM-DD} | **Severity:** {Critical / High / Medium} | **Category:** {Bug / Tail-Latency / Memory-Leak / Concurrency / Security-Vulnerability / CVE} | **Status:** {Investigating / Root Cause Proven / Fixed & Verified}
* **TL;DR Symptom / Impact:** {1–2 sentences describing the exact observed failure, exploit vector, anomaly, or tail-latency spike}
* **TL;DR Root Cause (`file:line`):** {Exact mechanism, CWE/CVE ID if applicable, broken invariant, and `file:line` where the defect originates}
* **TL;DR Permanent Fix:** {Concise summary of the structural fix, security hardening, and newly enforced invariant}

---
## 1. Failure Manifestation, Threat Vector & Observability Evidence
* Exact error traces, structured log signatures, metric anomalies (`p99`/`p99.9` spikes, memory growth), or exploit preconditions required to trigger the issue.

## 2. Minimal Deterministic Reproducer (`memory/scratchpad/` or `memory/script/`)
* Exact command, input payload, concurrency/timing schedule, or adversarial condition that reproduces the failure or vulnerability 100% deterministically.

## 3. Fault Tree & Hypothesis Elimination Log
* **Hypothesis A:** {Claim} → **Test/Probe:** {Command/Script} → **Result:** `Falsified` / `Confirmed` ({Brief evidence — prevents repeating dead ends}).
* **Hypothesis B:** {Claim} → **Test/Probe:** {Command/Script} → **Result:** `Confirmed` ({Exact causal chain}).

## 4. Formal Mechanism & Broken Invariant (Abstraction & Lean Proof)
* Step-by-step state transition trace showing how the system enters the illegal/vulnerable state, the exact invariant violated, and the formal/logical proof that the new constraint prevents re-entry across all edge and worst cases.

## 5. Remediation, Regression Tests & Blast-Radius Check
* Modified `file:line` locations, regression/security test commands added to the suite, before/after normal & worst-case benchmark results, and sibling modules audited for the same anti-pattern.
```

---

### Deep Research Templates (`memory/research/{topic}.md` & `memory/research/libs.md`)
*Read on-demand only (do not auto-load at session start). Always place the `TL;DR` Decision Header in the first 15 lines so agents can inspect conclusions with minimal token usage:*

#### 1. Topic Deep-Research Template (`memory/research/{topic}.md`)
```markdown
# Deep Research: {Topic Name}
* **Date:** {YYYY-MM-DD} | **Status:** {Verified / Draft / Superseded}
* **TL;DR Conclusion:** {1–3 sentences stating the exact technical decision, chosen algorithm/architecture, and key constraint}
* **Adopted Pattern / Invariant:** {Exact formula, protocol, security boundary, or architectural rule to apply}

---
## 1. Problem Context & Scope
* Why this research was triggered and what system components it affects.

## 2. Comparative Analysis, Performance & Maintainability Tradeoffs
* Options evaluated, time/space complexity, tail-latency (`p99`/`p99.9`) behavior, operational maintenance burden, and failure modes.

## 3. Security, Vulnerability Surface & Worst-Case Bounds
* Mathematical/logical bounds, threat vectors (OWASP/CWE), pathological inputs, and concurrency/memory hazards.

## 4. Implementation Blueprint & References
* Concrete integration steps, code patterns, and authoritative source links/benchmarks.
```

#### 2. Library, CVE & Dependency Research Registry (`memory/research/libs.md`)
```markdown
# Library, CVE & Dependency Research Registry (`memory/research/libs.md`)

> **On-Demand Lookup Policy:** Consult this file before adding, upgrading, auditing, or replacing any dependency. Read only the summary table or relevant package section to save tokens.

## Evaluated Libraries Index

| Package / Library | Domain / Purpose | Pinned Version | Decision (`Adopted` / `Rejected` / `Deprecated`) | CVE & Maintenance Status | Performance / Tail-Latency & Bundle Notes | Alternatives Considered |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `{pkg-name}` | `{e.g., Schema Validation}` | `{x.y.z}` | `Adopted` | `0 CVEs / Active` | `{Low p99 overhead, zero-alloc path}` | `{Alt1, Alt2}` |

## Detailed Package, Security & Maintenance Evaluations
### `{pkg-name}` (`{x.y.z}`) — {Adopted / Rejected / Deprecated}
* **Security, Vulnerability & CVE Audit:** {CVE history, CWE exposure, transitive dependency risk, audit command result, license compliance}
* **Maintenance & Longevity:** {Maintainer health, release cadence, breaking-change frequency, deprecation risk, upgrade path}
* **Benchmark & Worst-Case Profile:** {Memory footprint, cold-start cost, bundle size, p99/p99.9 latency under pathological/adversarial payloads}
* **Integration Constraints & Gotchas:** {Edge cases, known bugs, required security flags, or configuration rules}
```

---

### Tool & Skill Library Template (`memory/tools.md`)
*Initialize when `memory/tools.md` is absent, and update whenever a script is added or updated in `memory/script/`:*

```markdown
# Tool & Skill Library Registry (`memory/tools.md`)

> **Token Optimization Policy:** Always inspect this registry before writing ad-hoc commands or new scripts.
> * **Persistent Automation (`memory/script/`):** Version-controlled, reusable pipelines, benchmark runners, security/CVE auditors, and validators that emit compact, token-efficient outputs.
> * **Ephemeral Sandbox (`memory/scratchpad/`):** Git-ignored directory for temporary hypothesis testing, anomaly/exploit reproducers, and one-off scripts. Graduate reusable scripts from `memory/scratchpad/` to `memory/script/` and register them below.

## Registered Scripts & Skills (`memory/script/`)

| Tool / Skill ID | Script Path | Purpose & When to Use | Exact Invocation Command | Output Format (Compact) |
| :--- | :--- | :--- | :--- | :--- |
| `clean-unused` | `memory/script/clean_unused.sh` | Strip dead imports/code across FE & BE | `bash memory/script/clean_unused.sh` | Exit code + modified file count |
| `bench-tail` | `memory/script/bench_tail.py` | Run p50/p95/p99/p99.9, anomaly, and worst-case benchmarks | `uv run python memory/script/bench_tail.py --target <mod>` | JSON summary (`p50`, `p99`, `p99.9`, `max`, `mem_peak`) |
| `sec-audit` | `memory/script/sec_audit.sh` | Run CVE dependency audit (`bun audit` / `pip-audit`) & secret/CWE checks | `bash memory/script/sec_audit.sh` | JSON summary (`cve_count`, `severity`, `pkg`, `file:line`) |
| `validate-all` | `memory/script/validate.sh` | Execute typecheck, lint, unit, and property tests concisely | `bash memory/script/validate.sh` | Pass/Fail summary + failing `file:line` only |

## Active Scratchpad Experiments (`memory/scratchpad/`)
* **Git Status:** Must remain ignored in `.gitignore` (`memory/scratchpad/`).
* **Current Probes:** {List active temporary hypothesis/proof/benchmark/RCA reproducer scripts or `None`}
```

---

### Project Architecture Template (`memory/project.md`)
*Generate when `memory/project.md` is absent by performing a one-time architectural codebase scan:*

```markdown
# Project Architectural Blueprint: {Project Name}

## 1. Project Overview & Business Domain
* Core mission, problem solved, user domain, key stakeholders, and high-level boundaries.

## 2. Tech Architecture, Runtime Stack & Dependency Hygiene
* Language versions, primary frameworks, package managers (`bun`/`uv`), engines, and pinned dependencies (cross-referenced with `memory/research/libs.md` for CVE and maintenance status).

## 3. Directory Layout & Module Ownership
* Structural directory tree highlighting functional module boundaries, orchestration entry points, `memory/script/`, `memory/scratchpad/`, `memory/research/`, and `memory/rca/`.

## 4. Key Data Models & Schema Mappings
* Database tables, relations, indexes, entity lifecycles, and ORM schema mappings (e.g., Prisma, Drizzle).

## 5. System Features, Core Mechanics & Performance Budgets
* Critical business workflows, background jobs, bounded caching strategies, state machines, and hot-path latency/memory budgets.

## 6. Authentication, Authorization, Security & Vulnerability Posture
* Auth providers, JWT/session lifecycles, RBAC/ABAC matrices, input validation boundaries, OWASP/CWE mitigations, CVE audit policies, and secrets management.

## 7. REST API Endpoints Reference
* Route paths, HTTP methods, request payload schemas, rate limits, auth requirements, and status responses.

## 8. WebSocket & Asynchronous Event Protocol Spec
* Real-time events, subscription channels, event schemas, backpressure/payload limits, message brokers, and reconnection rules.

## 9. Third-Party Integrations & External Services
* External APIs, payment gateways, webhooks, signature verification, notification services, and external dependencies.

## 10. Restricted & Auto-Generated Files (Do Not Edit)
* Code artifacts that must not be modified directly (e.g., generated types, migrations, build outputs).

## 11. Architectural Invariants & ADRs (Key Decisions)
* Critical decisions made, non-negotiable boundaries, formal invariants, and links to deep-dive files in `memory/research/{topic}.md` and `memory/rca/{topic}.md`.

## 12. Observability, Benchmark, Validation & Security Testing Strategy
* **Observability:** Logging, metrics, tracing, security audit trails, and runtime state inspection mechanisms.
* **Benchmark (Normal & Adversarial):** Baseline vs. target SLAs covering `p50`/`p95`/`p99`/`p99.9` tail latency, memory ceilings, anomaly injection, boundary edge cases, and worst-case/DoS pathological inputs.
* **Validation, Security & Formal Verification:** Unit, property-based, integration, E2E, and security/CVE audit commands, Abstraction/Hypothesis/Lean Proof artifacts, test database isolation, and mock fixture setup.

## 13. Running Local Infrastructure
* Port mappings, database containers, environment variable setup, and seeding commands.

## 14. Containerized & Production Cloud Deployments
* Dockerfile specifications (non-root, minimal base images), Cloud Run / Kubernetes manifests, resource limits, environment flags, and CI/CD pipelines.

## 15. Known Technical Debt, Maintenance Risks & Fragile Areas
* Known bottlenecks, tail-latency risks, deprecated dependencies, security/CVE watch items, legacy code sections, concurrency hazards, and links to historical root-cause analyses in `memory/rca/{topic}.md`.
```

---

### Session Log Template (`memory/{YYYY-MM-DD}/session.md`)

```markdown
# Session Log: {YYYY-MM-DD}

### Previous Session Tracking
* **Last Session Date:** {YYYY-MM-DD or None}
* **Final Status:** {Summary of ending state from previous session}
* **Key Achievements:** {List of verified accomplishments}
* **Leftover Blockers:** {Unresolved issues carried forward}

### Alignment & Verification Contract
* **Observability Criteria:** {Aligned logs, traces, metrics, security audit outputs, or state checks}
* **Benchmark Targets (Normal, Non-Normal & Performance Budgets):** {Aligned baselines & targets for p50/p95/p99/p99.9 Tail Latency, memory/CPU ceilings, Anomalies, Edge Cases, and Worst-Case/Adversarial inputs}
* **Validation, Security & Maintenance Criteria:** {Aligned acceptance checks, formal invariants, CVE/security checks, and pass/fail commands}
* **Formal Reasoning (If Applicable):** {Abstraction, Hypothesis, Lean Proof, and Test mapping summary}
* **Tools, Scratchpad, Research & RCA Used:** {Scripts in `memory/script/`, probes in `memory/scratchpad/`, research/CVE audits in `memory/research/`, or RCAs in `memory/rca/`}

### Current Session Tracking
* **Active Task:** {Current in-scope objective}
* **Last Completed Step:** {Most recent verified command, tool, or edit}
* **Next Action:** {Immediate next programmatic step or tool call}
* **Summary of Activity:** {Concise description of changes made in this session}
* **Progress / Changes:** {Specific files, modules, and tests modified}
* **Blockers / Unresolved Items:** {Current blockers, unknowns, or none}
```