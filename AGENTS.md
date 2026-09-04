# Universal AI Agent Engineering & Operational Guidelines

> **Purpose:** This document defines universal operating principles, quality standards, and delivery contracts for AI coding agents. It is strictly project-agnostic and language/stack-agnostic, and can be applied to any codebase, framework, or runtime.

---

## 1. Golden Rules & Testing Discipline

- New test files are opt-in. Do not create unit, integration, end-to-end, or spec files, or new test-only helpers/fixtures, unless the user explicitly requests their creation or approves it first. A request to implement, fix, test, or verify something does not by itself authorize new test files. Assume no by default; ask only when creating them has a concrete benefit, not as a routine step.
- Prefer running existing tests and direct runtime/manual checks without adding test files. Where test changes are in scope, exercise observable behavior rather than asserting source-code strings, implementation shapes, or that tests exist.

### Test Execution Policy
1. **Zero Unsolicited Test Files:** Agents MUST NOT create new test files, test directories, or test scaffolding unless explicitly requested by the user.
2. **Baseline vs. Regression:** Run relevant tests after every modification. Agents MUST NOT run full project-wide test suites prior to starting work unless establishing a baseline is strictly necessary to determine whether a failure pre-existed.
3. **Large Suite Efficiency:** During active development and iteration, execute ONLY the affected test subset. Execute the full project test suite once prior to final delivery.

---

## 2. Engineering Discipline, Scope & Safety

1. **Strict Scope Discipline:**
   - Implement ONLY what was requested.
   - Agents MUST NOT perform drive-by refactoring, unprompted code formatting, or unsolicited stylistic cleanups in unrelated code.
   - Solve the immediate problem cleanly without expanding scope.
2. **Dependency Approval Gate:**
   - Agents MUST NOT introduce new external packages, libraries, or dependencies without explicit user confirmation.
   - When a dependency is approved, pin the exact version and update lockfiles/manifests within the same change.
3. **Secrets & Credentials Protection:**
   - Agents MUST NEVER commit, log, or expose secrets, `.env` files, API keys, certificates, or private credentials.
   - Mask sensitive values in outputs and tool logs. If uncommitted secrets are detected in the workspace, notify the user immediately.
4. **Destructive Operations Gate:**
   - Agents MUST obtain explicit user approval before executing irreversible or destructive operations.
   - This includes recursive file deletions (`rm -rf`), force-pushing branches (`git push --force`), resetting repository history (`git reset --hard`), dropping database tables/schemas, or running irreversible data migrations.
   - Before performing an approved destructive or high-risk operation, state what will be lost and, where feasible, what the rollback path is (backup, snapshot, reverse migration, `git reflog`, etc.).
5. **Clean Cutover & Minimal Footprint:**
   - When replacing or modifying functionality, migrate all callers and delete obsolete implementations, unused imports, and dead code. Do not leave deprecated shims or commented-out blocks.
   - Avoid speculative abstractions. Prefer simple, readable, boring implementations over complex generalizations.

---

## 3. Problem Solving, Stability & Error Handling

1. **Root Cause Resolution:**
   - Fix the underlying cause of an issue, not its symptom.
   - Agents MUST NEVER silently swallow, catch-and-ignore, or mask errors.
   - Catching exceptions or errors at architectural boundaries is permitted ONLY when the error is properly logged, enriched, or re-thrown.
2. **Boundary & Invariant Preservation:**
   - All input parsing, data transformations, and collection/array/index access MUST enforce boundary and type validation to prevent out-of-range access, invalid state, or corrupted data — regardless of language, runtime, or platform.
3. **Resource & Performance Safety:**
   - Performance-critical or high-frequency code paths (request handlers, render/event loops, callbacks, background jobs) MUST avoid unnecessary blocking calls, unbounded loops, or unbounded memory/resource growth.
   - Any acquired resource (file handles, network/database connections, locks, sessions, subscriptions, timers) MUST be released deterministically on all success and error paths, using the idiomatic mechanism of the language/framework in use (e.g., `try/finally`, context managers, `defer`, RAII, `using`, disposal hooks).

---

## 4. Verification & Delivery Standards

Before concluding any task or handing over work:

1. **Compilation & Linting:** Code MUST build cleanly with zero compiler/type errors and zero new linter warnings.
2. **Existing Test Suite:** All pre-existing relevant tests MUST pass without regression.
3. **Documentation Synchronization:** Whenever public APIs, configuration keys, CLI flags, environment variables, or observable behaviors change, update the corresponding documentation (`README.md`, configuration guides, schemas, changelogs) within the same turn.
4. **Working Tree Discipline & Conventional Commits:**
   - The working tree MUST contain ONLY intended modifications — zero scratch files, debug logs, or unrequested build artifacts.
   - Keep any temporary or exploratory files (scratch scripts, throwaway notes, debug dumps) outside the tracked working tree during development, and remove anything that does end up inside it before final delivery.
   - Commits MUST follow the Conventional Commits specification (e.g., `feat(...)`, `fix(...)`, `refactor(...)`, `docs(...)`).
5. **Grounded Verifiable Proof:** Conclusions MUST be substantiated with observable evidence (actual execution outputs, verified test results, or demonstrated runtime behavior) — not assumptions about what the code should do.

---

## 5. Document Maintenance

- This document should carry a version number and last-updated date so agents and maintainers can tell when guidance has changed.
- Any amendment to these rules should be made deliberately, reviewed like a code change, and communicated to anyone relying on this file.
