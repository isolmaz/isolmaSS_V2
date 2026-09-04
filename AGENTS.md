# Universal AI Agent Engineering & Operational Guidelines

> **Version:** 1.0.0  
> **Last updated:** 2026-09-04  
> **Purpose:** This document defines project-agnostic operating principles, quality standards, and delivery contracts for AI coding agents.

The terms **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** are interpreted according to RFC 2119.

## Scope and Precedence

1. Higher-priority platform and runtime instructions always take precedence.
2. Explicit user requirements take precedence over this document unless they conflict with higher-priority safety or platform constraints.
3. A more specific `AGENTS.md` located closer to a modified file takes precedence over a parent document.
4. Agents MUST preserve pre-existing user changes and MUST resolve instruction conflicts from repository context whenever this can be done safely.

---

## 1. Golden Rules & Testing Discipline

- New test files are opt-in. Agents MUST NOT create unit, integration, end-to-end, or spec files, or new test-only helpers or fixtures, unless the user explicitly requests or approves them. A request to implement, fix, test, or verify something does not by itself authorize new test files.
- Existing test files MAY be updated when needed to cover changed behavior. Tests MUST exercise observable behavior rather than source text, implementation shape, or the mere existence of code.
- Prefer existing tests and direct runtime checks over new test infrastructure.

### Test Execution Policy
1. **Baseline vs. Regression:** Run the smallest relevant verification after each coherent implementation iteration. Do not run a full project-wide suite before work unless a baseline is necessary to distinguish pre-existing failures.
2. **Development Efficiency:** During active development, run only the affected test, build, type-check, or lint targets.
3. **Final Verification:** Before delivery, run the broadest practical relevant suite. Run the full project suite when its cost and environment requirements are reasonable; otherwise report exactly what was not run and why.

---

## 2. Engineering Discipline, Scope & Safety

1. **Strict Scope Discipline:**
   - Implement ONLY what was requested.
   - Agents MUST NOT perform drive-by refactoring, unprompted code formatting, or unsolicited stylistic cleanups in unrelated code.
   - Solve the immediate problem cleanly without expanding scope.
2. **Dependency Approval Gate:**
   - Agents MUST NOT introduce new external packages, libraries, or dependencies without explicit user confirmation.
   - For an approved dependency, follow the repository's existing version-range and lockfile conventions and update all relevant manifests and lockfiles in the same change.
3. **Secrets & Credentials Protection:**
   - Agents MUST NEVER commit, log, or expose secrets, `.env` files, API keys, certificates, or private credentials.
   - Mask sensitive values in outputs and tool logs. If uncommitted secrets are detected in the workspace, notify the user immediately without revealing their values.
4. **Destructive Operations Gate:**
   - Agents MUST obtain explicit user approval before executing destructive or irreversible operations not already and unambiguously requested.
   - This includes recursive file deletion, force-pushing branches, resetting repository history, dropping database tables or schemas, and running irreversible data migrations.
   - Before an approved destructive or high-risk operation, state exactly what will be lost and, where feasible, the rollback path.
5. **Clean Cutover & Minimal Footprint:**
   - When replacing functionality, migrate every caller and remove the obsolete implementation, unused imports, and dead code within the requested scope. Do not leave deprecated shims or commented-out blocks unless explicitly required.
   - Avoid speculative abstractions. Prefer simple, readable, boring implementations over complex generalizations.

---

## 3. Problem Solving, Stability & Error Handling

1. **Root Cause Resolution:**
   - Fix the underlying cause of an issue, not its symptom.
   - Agents MUST NEVER silently swallow, catch-and-ignore, or mask unexpected errors.
   - A caught error MUST be re-thrown, enriched, converted into an explicit domain result, or handled through a deliberate recovery path. Expected control-flow errors need not be logged; unexpected failures MUST remain observable.
2. **Boundary & Invariant Preservation:**
   - Validate untrusted or externally sourced data at system boundaries.
   - Guard collection and index access when bounds are not statically guaranteed or invalid access is realistically possible. Do not duplicate guarantees already enforced by the type system or a validated invariant.
3. **Resource & Performance Safety:**
   - Performance-critical or high-frequency code paths (request handlers, render/event loops, callbacks, background jobs) MUST avoid unnecessary blocking calls, unbounded loops, and unbounded memory or resource growth.
   - Owned finite-lifetime resources MUST be released deterministically on every success and error path using the language's idiomatic mechanism. Long-lived resources MUST be tied to an explicit application or component lifecycle with a defined cleanup path.

---

## 4. Verification & Delivery Standards

Before concluding any task or handing over work:

1. **Compilation & Linting:** Changed code MUST introduce no new compiler, type-check, or linter errors. Relevant build, type-check, and lint targets MUST pass. Pre-existing unrelated failures MUST be reported without being modified unless the user requests otherwise.
2. **Existing Test Suite:** All relevant pre-existing tests MUST pass without regression. Any test or environment limitation MUST be reported precisely.
3. **Documentation Synchronization:** Update existing documentation when a documented public API, user-facing behavior, configuration contract, CLI option, or environment variable changes. Update a changelog only when the repository maintains one and its contribution policy requires an entry.
4. **Working Tree Discipline:**
   - Agents MUST preserve all pre-existing unrelated changes. Modifications introduced by the agent MUST be limited to the requested task.
   - Keep temporary or exploratory files outside the tracked working tree. Remove any scratch files, debug logs, or unrequested generated artifacts introduced during the task before delivery.
5. **Commits:** Create commits only when explicitly requested. Follow the repository's commit convention; if none exists, use Conventional Commits (for example, `feat(...)`, `fix(...)`, `refactor(...)`, or `docs(...)`).
6. **Grounded Verifiable Proof:** Conclusions MUST be supported by observed command output, passing relevant tests, or demonstrated runtime behavior. Verification claims MUST state exactly what was exercised; unverified assumptions MUST be identified as such.

---

## 5. Document Maintenance

- Keep the version number and last-updated date at the top of this document current whenever its rules change.
- Amend these rules deliberately, review them like code changes, and communicate material changes to anyone who relies on them.
