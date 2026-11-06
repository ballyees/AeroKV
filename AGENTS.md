# Operating Workflow

1. **Precedence and scope:** Follow explicit user requirements before this workflow. Work only in this workspace. Before executing work, perform strict scope definition by explicitly stating the Goal, In-Scope boundaries, and Out-of-Scope limitations. Eliminate conversational filler, status commentary, and text preambles; execute the next required development tool or terminal command immediately. Write concise, accurate, professional output; state assumptions and tradeoffs, choose the simplest sufficient solution, and request clarification when ambiguity blocks a safe decision.
2. **Context and memory:** At session initialization (first chat turn), load memory sequentially: `memory/rules.md` → `memory/refs.md` → select and read the **specific past `session.md`** referenced in `refs.md` relevant to the active task → `memory/project.md` → **only inspect files and modules directly relevant to the in-scope task**. Do not read the entire codebase during routine tasks. Detect the ISO date; create and read the daily session log using the Session Template below if absent.
   * **Missing `memory/project.md`:** If `memory/project.md` is missing, perform a one-time comprehensive codebase analysis to generate it using the **Project Architecture Template** below (covering all 15 architectural and operational sections).
   * **Missing `memory/rules.md`:** If `memory/rules.md` is missing, initialize and populate it with code style, coding conventions, project conventions, basic guidelines, quality criteria, and coding standards.
   * **Cascading Multi-Repo Updates:** In multi-repo or nested workspace layouts (e.g., `root/project1/project_frontend/`), when modifying code within specific sub-projects, **you must update memory in all relative project and component memory directories** (e.g., if working in `project1` and modifying `project1/project_frontend`, update both `project1/memory/` AND `project1/project_frontend/memory/`). Append progress, changes, blockers, and task status across all touched memory locations after codebase changes. Update `memory/refs.md` after daily-log changes and `memory/project.md` after high-level architectural changes.
3. **Plan and implement:** Before tools, files, commands, or code, record a verification plan with objective, edge cases, and module impact. Prioritize high-risk risk mitigation: identify and resolve the highest-risk unknowns, core logic dependencies, or architectural uncertainties in the workflow before working on trivial tasks. For core-business-logic changes, obtain user confirmation through the Walkthrough Artifact. Implement sequentially; preserve local style; avoid unrequested features, abstractions, refactors, and cleanup. Remove only unused code caused by the change.
4. **Engineering standards:** Adhere primarily to project conventions defined in `memory/rules.md`. If `memory/rules.md` is not provided or lacks specific rules, apply clean coding standards that keep code simple, uncomplex, and easily readable by humans while ensuring high technical quality and rigor. Keep code modular, typed, and maintainable; reserve entry points for orchestration and isolate business logic. **Mandatory Unused Code Deletion Standard (Frontend & Backend):** Zero tolerance for dead or unused code. All unused imports, variables, parameters, functions, dead methods, unreachable branches, and commented-out dead code blocks **MUST have their lines of code permanently deleted** (never commented out, never hidden behind suppressions). Enforce via `bun run clean:unused` on frontend and `uv run ruff check --fix --select F401` on backend. Prefer React TypeScript functional components with hooks, Node.js TypeScript services, and Tailwind; use NaiveUI, then DaisyUI, before Tailwind, and global color styling when required. Meet PEP 8 and ESLint. Before installing dependencies, inspect existing alternatives, verify reputation, pin exact versions, and use `bun`.
5. **Safety and verification:** Practice empirical verification: never assume code works; every implementation step must be verified by executing actual terminal commands, compile checks, linting, and passing unit tests. Practice rigorous edge-case testing: before advancing to the next workflow step, explicitly ask "What edge case or failure mode could prove this logic wrong?" and actively test it. Never force-delete, wipe databases, or clean Git without explicit instruction. Check ports and prevent orphaned servers. For UI changes, inspect the page and capture a screenshot. Document failures, fix them, and re-run checks. For major changes, provide a Walkthrough Artifact with modified files, compile output, and test results.
6. **Agent routing:** Only for `call-agent` work, select registered agents; do not invent agents. Assign lead/supporting agents for multi-domain requests, otherwise route unmatched work to `GeneralistAgent` with its reason. Output only `# ROUTING ANALYSIS`, `# EXPERT ADVICE`, and `# CONTINUATION WORKFLOW`: one routing sentence, specific lead-agent advice, then 5–10 strictly sequential imperative steps naming the target file/module, inputs/tools, and verification. Avoid filler and speculative phrasing, executing the next required tool action immediately.

---

### Project Architecture Template (`memory/project.md`)
*Generate when `memory/project.md` is absent by performing a one-time architectural codebase scan:*

```markdown
# Project Architectural Blueprint: {Project Name}

## 1. Project Overview & Business Domain
* Core mission, problem solved, user domain, key stakeholders, and high-level boundaries.

## 2. Tech Architecture & Runtime Stack
* Language versions, primary frameworks, package managers (`bun`), engines, and dependencies.

## 3. Directory Layout & Module Ownership
* Structural directory tree highlighting functional module boundaries and orchestration entry points.

## 4. Key Data Models & Schema Mappings
* Database tables, relations, entity lifecycles, and ORM schema mappings (e.g., Prisma, Drizzle).

## 5. System Features & Core Mechanics
* Critical business workflows, background jobs, caching strategies, and state machines.

## 6. Authentication, Authorization & Security
* Auth providers, JWT/session lifecycles, RBAC role matrices, and secrets management policies.

## 7. REST API Endpoints Reference
* Route paths, HTTP methods, request payload schemas, auth requirements, and status responses.

## 8. WebSocket & Asynchronous Event Protocol Spec
* Real-time events, subscription channels, event schemas, message brokers, and reconnection rules.

## 9. Third-Party Integrations & External Services
* External APIs, payment gateways, webhooks, notification services, and external dependencies.

## 10. Restricted & Auto-Generated Files (Do Not Edit)
* Code artifacts that must not be modified directly (e.g., generated types, migrations, build outputs).

## 11. Architectural Invariants & ADRs (Key Decisions)
* Critical decisions made, non-negotiable boundaries, and intentionally rejected alternatives.

## 12. Testing Strategy & Mocking Guidelines
* Unit, integration, and E2E test commands, test database isolation, and mock fixture setup.

## 13. Running Local Infrastructure
* Port mappings, database containers, environment variable setup, and seeding commands.

## 14. Containerized & Production Cloud Deployments
* Dockerfile specifications, Cloud Run / Kubernetes manifests, environment flags, and CI/CD pipelines.

## 15. Known Technical Debt & Fragile Areas
* Known bottlenecks, legacy code sections, concurrency risks, and areas requiring caution.
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

### Current Session Tracking
* **Active Task:** {Current in-scope objective}
* **Last Completed Step:** {Most recent verified command, tool, or edit}
* **Next Action:** {Immediate next programmatic step or tool call}
* **Summary of Activity:** {Concise description of changes made in this session}
* **Progress / Changes:** {Specific files, modules, and tests modified}
* **Blockers / Unresolved Items:** {Current blockers, unknowns, or none}
```