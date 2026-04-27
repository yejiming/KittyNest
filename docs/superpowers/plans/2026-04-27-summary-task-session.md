# Summary Task Session Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement one-button Project Analyze, session-only summaries, manual task creation, newest-first session batching, and markdown table rendering.

**Architecture:** Reuse the existing Tauri jobs table and React single-file app structure. Backend changes stay in `analysis.rs`, `db.rs`, `commands.rs`, `models.rs`, `lib.rs`, and existing tests; frontend changes stay in `App.tsx`, `api.ts`, `types.ts`, `styles.css`, and `App.test.tsx`.

**Tech Stack:** Rust/Tauri, rusqlite, React 18, Vitest, Testing Library.

**Workspace Note:** This directory is not a git repository, so no git worktree or commits can be used.

---

### Task 1: Backend Session Analysis

**Files:**
- Modify: `src-tauri/src/analysis.rs`
- Modify: `src-tauri/src/db.rs`

- [ ] Write failing Rust tests that prove session analysis accepts only `session_title` and `summary`, does not create tasks, and does not write task summary files.
- [ ] Run the targeted Rust tests and confirm they fail for the expected reasons.
- [ ] Simplify `SessionAnalysis`, `remote_session_analysis`, and `session_analysis_from_json`.
- [ ] Add or adjust DB session update helper so processed sessions can keep `task_id` optional.
- [ ] Remove task creation and task-summary append behavior from session storage.
- [ ] Run targeted Rust tests and confirm they pass.

### Task 2: Backend Job Flow

**Files:**
- Modify: `src-tauri/src/analysis.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/api.ts`

- [ ] Write failing Rust tests for newest-first pending session selection, project analyze capping to 20, and project analyze writing summary plus progress.
- [ ] Run the targeted Rust tests and confirm they fail for the expected reasons.
- [ ] Change pending session queries to order by `updated_at DESC`.
- [ ] Add project analysis enqueue and execution flow that analyzes up to 20 newest pending/failed sessions, then writes project summary and progress.
- [ ] Expose the command through Tauri and frontend API.
- [ ] Run targeted Rust tests and confirm they pass.

### Task 3: Manual Task Creation

**Files:**
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/analysis.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types.ts`
- Modify: `src/api.ts`

- [ ] Write failing Rust tests for reviewed-project validation, `user_prompt.md` writing, and async `llm_prompt.md` generation.
- [ ] Run the targeted Rust tests and confirm they fail for the expected reasons.
- [ ] Add create-task command and result type.
- [ ] Add a task prompt-generation job kind that reads project summary/progress and writes `llm_prompt.md`.
- [ ] Run targeted Rust tests and confirm they pass.

### Task 4: Frontend Behavior

**Files:**
- Modify: `src/App.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/api.ts`
- Modify: `src/types.ts`
- Modify: `src/styles.css`

- [ ] Write failing Vitest tests for Project one-button Analyze, Tasks manual create form, Session detail merged card, 3-day session range, and markdown table rendering.
- [ ] Run targeted Vitest tests and confirm they fail for the expected reasons.
- [ ] Update Project detail actions.
- [ ] Add Tasks list creation form using reviewed projects.
- [ ] Merge Session detail title/path cards.
- [ ] Add `3 days` option.
- [ ] Extend MarkdownBlock to render simple pipe tables.
- [ ] Add table CSS.
- [ ] Run targeted Vitest tests and confirm they pass.

### Task 5: Full Verification

**Files:**
- No production edits expected.

- [ ] Run `npm test`.
- [ ] Run `npm run build`.
- [ ] Run `cargo test` in `src-tauri`.
- [ ] Fix only failures caused by this change.
- [ ] Re-run failed verification commands until they pass or report the blocker with exact output.
