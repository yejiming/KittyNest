# Agent Session Task Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Save, refresh, load, delete, and create Task Assistant Drawer sessions as first-class KittyNest tasks with memory-reading and interactive task-creation assistant tools.

**Architecture:** Keep SQLite `tasks` as the task list/status index and write durable saved Drawer content into each task directory as `description.md` and `session.json`. Extend the existing Rust agent registry with clear/export/import and interactive `create_task` waits, then expose those behaviors through Tauri commands and `agent://event`. The React app owns visible Drawer timeline state and page-level task proposal modals, while backend commands own durable files, DB rows, LLM-generated task metadata, and restored LLM context.

**Tech Stack:** React 18, TypeScript, Tauri 2 commands/events, Rust, rusqlite, serde/serde_json, reqwest blocking OpenAI-compatible calls, Vitest, React Testing Library, Cargo tests.

---

## File Structure

- Modify `src/types.ts`: add `TaskRecord.createdAt`, optional `descriptionPath`, optional `sessionPath`.
- Modify `src/agentTypes.ts`: add saved session/timeline types, `create_task_request` event type, and normalization helpers.
- Modify `src/api.ts`: add wrappers for `clearAgentSession`, `saveAgentSession`, `loadAgentSession`, and `resolveAgentCreateTask`.
- Modify `src/AgentDrawer.tsx`: add Save/Refresh controls, controlled load support, save payload export, project-change refresh, and rendering for loaded timeline.
- Modify `src/App.tsx`: own Drawer load/refresh model-change coordination, update Tasks list/detail, render task proposal modal, and wire Load/Delete.
- Modify `src/App.test.tsx`: add frontend red tests for all new UI and API flows.
- Modify `src/styles.css`: add compact header action styles, task detail conversation styles, hidden-scroll markdown description styles, and page-centered create-task modal styles.
- Modify `src-tauri/src/models.rs`: add task fields and DTOs for saved agent sessions.
- Modify `src-tauri/src/db.rs`: migrate/backfill `tasks.created_at`, list new task fields, allow saved tasks to update status, add helpers to fetch project/task ids.
- Modify `src-tauri/src/commands.rs`: add clear/save/load/create-task resolver commands and update delete behavior.
- Modify `src-tauri/src/assistant/context.rs`: make `AgentStoredMessage` serializable for session export/import.
- Modify `src-tauri/src/assistant/mod.rs`: add session export/import/clear, `create_task_request` events, and pending create-task waits.
- Modify `src-tauri/src/assistant/tools.rs`: register `read_memory` and `create_task`.
- Create `src-tauri/src/assistant/tools/read_memory.rs`: memory entity lookup tool.
- Create `src-tauri/src/assistant/tools/create_task.rs`: interactive task proposal tool.
- Modify `src-tauri/src/assistant/llm.rs`: add a non-streaming JSON request helper for task metadata generation.
- Modify `src-tauri/src/lib.rs`: register new Tauri commands if command list is explicit.

## Task 1: Add Task Created Fields And Saved Session Metadata

**Files:**
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/models.rs`
- Modify: `src/types.ts`
- Modify: `src/App.test.tsx`

- [ ] **Step 1: Write the failing frontend test for Tasks columns**

Add this test near the existing Tasks list tests in `src/App.test.tsx`:

```tsx
it("shows saved task list columns with created date", async () => {
  vi.spyOn(api, "getAppState").mockResolvedValue({
    ...state,
    tasks: [
      {
        ...state.tasks[0],
        status: "discussing",
        createdAt: "2026-04-28T08:00:00Z",
        descriptionPath: "/Users/kc/.kittynest/projects/KittyNest/tasks/session-ingest/description.md",
        sessionPath: "/Users/kc/.kittynest/projects/KittyNest/tasks/session-ingest/session.json",
      },
    ],
  });

  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: /^tasks$/i }));

  expect(screen.getByRole("columnheader", { name: "Name" })).toBeInTheDocument();
  expect(screen.getByRole("columnheader", { name: "Project" })).toBeInTheDocument();
  expect(screen.getByRole("columnheader", { name: "Status" })).toBeInTheDocument();
  expect(screen.getByRole("columnheader", { name: "Created" })).toBeInTheDocument();
  expect(screen.queryByRole("columnheader", { name: "Sessions" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: /session ingest/i })).toHaveTextContent("2026-04-28");
});
```

- [ ] **Step 2: Run the focused frontend test to verify it fails**

Run:

```bash
npm test -- src/App.test.tsx -t "shows saved task list columns with created date"
```

Expected: FAIL because `TaskRecord` has no `createdAt` field and the Tasks table still renders `Sessions`.

- [ ] **Step 3: Write the failing backend migration/listing test**

Add this test inside `#[cfg(test)] mod tests` in `src-tauri/src/db.rs`:

```rust
#[test]
fn task_records_include_created_at_and_saved_paths() {
    let temp = tempfile::tempdir().unwrap();
    let paths = crate::models::AppPaths::from_data_dir(temp.path().join("kittynest"));
    let connection = open(&paths).unwrap();
    migrate(&connection).unwrap();
    let project_id = ensure_project_for_workdir(
        &connection,
        "/tmp/saved-task-project",
        "codex",
        "2026-04-28T08:00:00Z",
    )
    .unwrap();
    let task_dir = paths.projects_dir.join("saved-task-project").join("tasks").join("draft");
    std::fs::create_dir_all(&task_dir).unwrap();
    let description = task_dir.join("description.md");
    let session = task_dir.join("session.json");
    std::fs::write(&description, "Description").unwrap();
    std::fs::write(&session, "{}").unwrap();
    upsert_task(
        &connection,
        project_id,
        "draft",
        "Draft Task",
        "brief",
        "discussing",
        &description.to_string_lossy(),
    )
    .unwrap();

    let task = list_tasks(&connection).unwrap().remove(0);

    assert_eq!(task.created_at, task.updated_at);
    assert_eq!(task.description_path.as_deref(), Some(description.to_string_lossy().as_ref()));
    assert_eq!(task.session_path.as_deref(), Some(session.to_string_lossy().as_ref()));
}
```

- [ ] **Step 4: Run the backend test to verify it fails**

Run:

```bash
cd src-tauri && cargo test task_records_include_created_at_and_saved_paths
```

Expected: FAIL because `TaskRecord` lacks the new fields and `tasks.created_at` is not migrated.

- [ ] **Step 5: Add model and TypeScript fields**

In `src-tauri/src/models.rs`, change `TaskRecord` to:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub project_slug: String,
    pub slug: String,
    pub title: String,
    pub brief: String,
    pub status: String,
    pub summary_path: String,
    pub description_path: Option<String>,
    pub session_path: Option<String>,
    pub session_count: usize,
    pub created_at: String,
    pub updated_at: String,
}
```

In `src/types.ts`, change `TaskRecord` to:

```ts
export interface TaskRecord {
  projectSlug: string;
  slug: string;
  title: string;
  brief: string;
  status: string;
  summaryPath: string;
  descriptionPath?: string | null;
  sessionPath?: string | null;
  sessionCount: number;
  createdAt: string;
  updatedAt: string;
}
```

Update the test `state.tasks[0]` object in `src/App.test.tsx` to include:

```tsx
createdAt: "2026-04-26T01:00:00Z",
descriptionPath: null,
sessionPath: null,
```

- [ ] **Step 6: Migrate `tasks.created_at` and list saved file paths**

In `src-tauri/src/db.rs`, after the existing `tasks` table creation, add a column migration:

```rust
add_column_if_missing(
    connection,
    "tasks",
    "created_at",
    "created_at TEXT NOT NULL DEFAULT ''",
)?;
connection.execute(
    "UPDATE tasks SET created_at = updated_at WHERE created_at = ''",
    [],
)?;
```

Change the `CREATE TABLE IF NOT EXISTS tasks` statement to include:

```sql
created_at TEXT NOT NULL DEFAULT '',
updated_at TEXT NOT NULL,
```

Change `upsert_task` so inserts set both created and updated time, while updates preserve created time:

```rust
connection.execute(
    r#"
    INSERT INTO tasks (project_id, slug, title, brief, status, summary_path, created_at, updated_at)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
    "#,
    params![project_id, slug, title, brief, status, summary_path, now],
)?;
```

Change `list_tasks` SELECT and mapper to:

```rust
SELECT p.slug, t.slug, t.title, t.brief,
       CASE WHEN COUNT(s.id) = 0 THEN t.status ELSE t.status END AS status,
       t.summary_path,
       CASE WHEN t.summary_path LIKE '%/description.md' THEN t.summary_path ELSE NULL END AS description_path,
       CASE WHEN t.summary_path LIKE '%/description.md'
            THEN substr(t.summary_path, 1, length(t.summary_path) - length('/description.md')) || '/session.json'
            ELSE NULL
       END AS session_path,
       COUNT(s.id) AS session_count,
       t.created_at,
       t.updated_at
FROM tasks t
JOIN projects p ON p.id = t.project_id
LEFT JOIN sessions s ON s.task_id = t.id
GROUP BY t.id
ORDER BY t.updated_at DESC, t.title ASC
```

```rust
let session_count: i64 = row.get(8)?;
Ok(TaskRecord {
    project_slug: row.get(0)?,
    slug: row.get(1)?,
    title: row.get(2)?,
    brief: row.get(3)?,
    status: row.get(4)?,
    summary_path: row.get(5)?,
    description_path: row.get(6)?,
    session_path: row.get(7)?,
    session_count: session_count as usize,
    created_at: row.get(9)?,
    updated_at: row.get(10)?,
})
```

- [ ] **Step 7: Update Tasks table rendering**

In `src/App.tsx`, change `TasksList` header and row:

```tsx
<div className="table-header" role="row">
  <span role="columnheader">Name</span>
  <span role="columnheader">Project</span>
  <span role="columnheader">Status</span>
  <span role="columnheader">Created</span>
</div>
{tasks.map((task) => (
  <button key={`${task.projectSlug}-${task.slug}`} className="list-row" onClick={() => onOpen(task.projectSlug, task.slug)}>
    <strong>{task.title}</strong>
    <span>{task.projectSlug}</span>
    <small>{task.status}</small>
    <small>{task.createdAt ? task.createdAt.slice(0, 10) : compactAgeLabel(task.updatedAt)}</small>
  </button>
))}
```

- [ ] **Step 8: Run focused tests to verify green**

Run:

```bash
npm test -- src/App.test.tsx -t "shows saved task list columns with created date"
cd src-tauri && cargo test task_records_include_created_at_and_saved_paths
```

Expected: both PASS.

- [ ] **Step 9: Commit**

Run:

```bash
git add src/types.ts src/App.tsx src/App.test.tsx src-tauri/src/models.rs src-tauri/src/db.rs
git commit -m "feat: track saved task creation metadata"
```

## Task 2: Add Backend Agent Session Export, Import, And Clear

**Files:**
- Modify: `src-tauri/src/assistant/context.rs`
- Modify: `src-tauri/src/assistant/mod.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/api.ts`
- Modify: `src/App.test.tsx`

- [ ] **Step 1: Write failing Rust registry tests**

Add these tests inside `src-tauri/src/assistant/mod.rs` tests:

```rust
#[test]
fn clear_session_removes_messages_todos_and_context() {
    let registry = super::AgentRegistry::new_for_tests(VecEmitter::default());
    registry.run_with_llm_for_tests(
        "session-1",
        tempfile::tempdir().unwrap().path().to_path_buf(),
        tempfile::tempdir().unwrap().path().to_path_buf(),
        crate::config::default_llm_settings(),
        "hello",
        |_messages, _tools, _on_token| {
            Ok(super::llm::AssistantLlmResponse {
                content: "world".into(),
                tool_calls: Vec::new(),
            })
        },
    );

    assert!(!registry.session_export("session-1").messages.is_empty());

    registry.clear_session("session-1");

    assert!(registry.session_export("session-1").messages.is_empty());
    assert!(registry.session_export("session-1").llm_messages.is_empty());
}

#[test]
fn import_session_replaces_backend_context() {
    let registry = super::AgentRegistry::new_for_tests(VecEmitter::default());
    registry.session_import(
        "session-1",
        super::AgentSessionSnapshot {
            messages: vec![super::context::AgentStoredMessage::new("user", "loaded")],
            llm_messages: vec![serde_json::json!({"role": "user", "content": "loaded"})],
            todos: vec![super::tools::AgentTodo {
                content: "Loaded todo".into(),
                active_form: "Loading todo".into(),
                status: "pending".into(),
            }],
        },
    );

    let exported = registry.session_export("session-1");

    assert_eq!(exported.messages[0].content, "loaded");
    assert_eq!(exported.llm_messages[0]["content"], "loaded");
    assert_eq!(exported.todos[0].content, "Loaded todo");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd src-tauri
cargo test clear_session_removes_messages_todos_and_context
cargo test import_session_replaces_backend_context
```

Expected: FAIL because `AgentSessionSnapshot`, `session_export`, `session_import`, and `clear_session` do not exist.

- [ ] **Step 3: Write failing frontend API test for refresh**

Add this test near existing drawer tests in `src/App.test.tsx`:

```tsx
it("refreshes the drawer by clearing frontend and backend session state", async () => {
  vi.spyOn(api, "getAppState").mockResolvedValue({
    ...state,
    projects: [{ ...state.projects[0], reviewStatus: "reviewed" }],
  });
  const clearAgentSession = vi
    .spyOn(api as ApiWithReviewQueue & { clearAgentSession: (sessionId: string) => Promise<{ cleared: boolean }> }, "clearAgentSession")
    .mockResolvedValue({ cleared: true });

  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: /^assistant$/i }));
  window.dispatchEvent(new CustomEvent("kittynest-agent-event", {
    detail: { sessionId: window.sessionStorage.getItem("kittynest:agent-session"), type: "token", delta: "hello" },
  }));
  expect(await screen.findByText("hello")).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: /^refresh assistant$/i }));

  await waitFor(() => expect(clearAgentSession).toHaveBeenCalledTimes(1));
  expect(screen.queryByText("hello")).not.toBeInTheDocument();
});
```

Also extend `ApiWithReviewQueue` with:

```ts
clearAgentSession: (sessionId: string) => Promise<{ cleared: boolean }>;
```

- [ ] **Step 4: Run frontend test to verify it fails**

Run:

```bash
npm test -- src/App.test.tsx -t "refreshes the drawer by clearing frontend and backend session state"
```

Expected: FAIL because there is no `clearAgentSession` wrapper and no Refresh button.

- [ ] **Step 5: Make stored messages serializable and add snapshot methods**

In `src-tauri/src/assistant/context.rs`, change `AgentStoredMessage` derive to:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentStoredMessage {
    pub role: String,
    pub content: String,
}
```

In `src-tauri/src/assistant/mod.rs`, add near `AgentSession`:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionSnapshot {
    pub messages: Vec<AgentStoredMessage>,
    pub llm_messages: Vec<serde_json::Value>,
    pub todos: Vec<AgentTodo>,
}
```

Add registry methods:

```rust
pub fn clear_session(&self, session_id: &str) {
    self.stop_run(session_id);
    let mut sessions = self.inner.sessions.lock().expect("agent sessions lock poisoned");
    sessions.insert(session_id.into(), AgentSession::default());
}

pub fn session_export(&self, session_id: &str) -> AgentSessionSnapshot {
    self.inner
        .sessions
        .lock()
        .expect("agent sessions lock poisoned")
        .get(session_id)
        .map(|session| AgentSessionSnapshot {
            messages: session.messages.clone(),
            llm_messages: session.llm_messages.clone(),
            todos: session.todos.clone(),
        })
        .unwrap_or_default()
}

pub fn session_import(&self, session_id: &str, snapshot: AgentSessionSnapshot) {
    self.stop_run(session_id);
    let mut sessions = self.inner.sessions.lock().expect("agent sessions lock poisoned");
    sessions.insert(
        session_id.into(),
        AgentSession {
            messages: snapshot.messages,
            llm_messages: snapshot.llm_messages,
            todos: snapshot.todos,
            ..AgentSession::default()
        },
    );
}
```

- [ ] **Step 6: Add clear command and frontend wrapper**

In `src-tauri/src/commands.rs`, add:

```rust
#[tauri::command]
pub fn clear_agent_session(
    app: tauri::AppHandle,
    session_id: String,
    _services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    assistant_registry(app).clear_session(&session_id);
    Ok(serde_json::json!({ "cleared": true }))
}
```

In `src-tauri/src/lib.rs`, register `clear_agent_session` beside other assistant commands.

In `src/api.ts`, add:

```ts
export async function clearAgentSession(sessionId: string): Promise<{ cleared: boolean }> {
  if (!isTauriRuntime()) {
    return { cleared: true };
  }
  return invoke<{ cleared: boolean }>("clear_agent_session", { sessionId });
}
```

- [ ] **Step 7: Add Refresh button and local clear behavior**

In `src/AgentDrawer.tsx`, import `RefreshCw` and `clearAgentSession`, then add:

```tsx
async function refreshSession() {
  if (running) {
    await stopAgentRun(sessionId);
  }
  await clearAgentSession(sessionId);
  setMessages([]);
  setTodos([]);
  setContext(emptyContext);
  setInput("");
  setRunning(false);
}
```

In the header action area, before Close:

```tsx
<button aria-label="Refresh assistant" className="agent-icon-button" onClick={() => void refreshSession()}>
  <RefreshCw size={16} />
</button>
```

- [ ] **Step 8: Run focused tests**

Run:

```bash
npm test -- src/App.test.tsx -t "refreshes the drawer by clearing frontend and backend session state"
cd src-tauri
cargo test clear_session_removes_messages_todos_and_context
cargo test import_session_replaces_backend_context
```

Expected: all PASS.

- [ ] **Step 9: Commit**

Run:

```bash
git add src/api.ts src/AgentDrawer.tsx src/App.test.tsx src-tauri/src/assistant/context.rs src-tauri/src/assistant/mod.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: add assistant session refresh primitives"
```

## Task 3: Save And Load Drawer Sessions As Tasks

**Files:**
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/assistant/llm.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src/api.ts`
- Modify: `src/agentTypes.ts`
- Modify: `src/AgentDrawer.tsx`
- Modify: `src/App.test.tsx`

- [ ] **Step 1: Write failing Rust tests for save and load**

Add this test in `src-tauri/src/commands.rs` tests, creating a new `#[cfg(test)] mod tests` if needed:

```rust
#[test]
fn parse_task_metadata_requires_name_and_description() {
    let parsed = super::parse_task_metadata_json(
        r#"{"task_name":"Save Drawer","task_description":"Persist **session**."}"#,
    )
    .unwrap();

    assert_eq!(parsed.task_name, "Save Drawer");
    assert!(parsed.task_description.contains("Persist"));

    let error = super::parse_task_metadata_json(r#"{"task_name":""}"#).unwrap_err();
    assert!(error.to_string().contains("task_description"));
}
```

- [ ] **Step 2: Run backend test to verify it fails**

Run:

```bash
cd src-tauri && cargo test parse_task_metadata_requires_name_and_description
```

Expected: FAIL because `parse_task_metadata_json` does not exist.

- [ ] **Step 3: Write failing frontend Save/Load API test**

Add this test in `src/App.test.tsx` near drawer tests:

```tsx
it("saves the full drawer timeline and can load it back", async () => {
  vi.spyOn(api, "getAppState").mockResolvedValue({
    ...state,
    projects: [{ ...state.projects[0], reviewStatus: "reviewed" }],
  });
  const saveAgentSession = vi
    .spyOn(api as ApiWithReviewQueue & {
      saveAgentSession: (sessionId: string, projectSlug: string, timeline: unknown) => Promise<import("./types").TaskRecord>;
    }, "saveAgentSession")
    .mockResolvedValue({
      ...state.tasks[0],
      status: "discussing",
      createdAt: "2026-04-28T08:00:00Z",
      descriptionPath: "/Users/kc/.kittynest/projects/KittyNest/tasks/save-drawer/description.md",
      sessionPath: "/Users/kc/.kittynest/projects/KittyNest/tasks/save-drawer/session.json",
    });

  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: /^assistant$/i }));
  await userEvent.type(screen.getByLabelText("Message Task Assistant"), "Please persist this");
  await userEvent.click(screen.getByRole("button", { name: /^send$/i }));
  window.dispatchEvent(new CustomEvent("kittynest-agent-event", {
    detail: { sessionId: window.sessionStorage.getItem("kittynest:agent-session"), type: "done", reply: "Persisted answer" },
  }));

  await userEvent.click(screen.getByRole("button", { name: /^save assistant session$/i }));

  await waitFor(() => expect(saveAgentSession).toHaveBeenCalledTimes(1));
  expect(saveAgentSession.mock.calls[0][2]).toMatchObject({
    version: 1,
    projectSlug: "KittyNest",
  });
});
```

Extend `ApiWithReviewQueue` with:

```ts
saveAgentSession: (sessionId: string, projectSlug: string, timeline: unknown) => Promise<import("./types").TaskRecord>;
loadAgentSession: (projectSlug: string, taskSlug: string) => Promise<import("./agentTypes").SavedAgentSession>;
```

- [ ] **Step 4: Run frontend test to verify it fails**

Run:

```bash
npm test -- src/App.test.tsx -t "saves the full drawer timeline and can load it back"
```

Expected: FAIL because Save button and API wrapper do not exist.

- [ ] **Step 5: Add non-streaming task metadata LLM helper**

In `src-tauri/src/assistant/llm.rs`, add:

```rust
pub fn request_openai_json(
    settings: &crate::models::LlmSettings,
    messages: Vec<serde_json::Value>,
) -> anyhow::Result<String> {
    if settings.interface != "openai" {
        anyhow::bail!("Task Assistant currently requires an OpenAI-compatible Assistant model");
    }
    if !crate::llm::configured_for_remote(settings) {
        anyhow::bail!("LLM settings are incomplete");
    }
    let max_tokens = if settings.max_tokens == 0 { 4096 } else { settings.max_tokens };
    let body = serde_json::json!({
        "model": settings.model,
        "messages": messages,
        "stream": false,
        "max_tokens": max_tokens,
        "max_completion_tokens": max_tokens,
        "temperature": if settings.temperature.is_finite() { settings.temperature } else { 0.2 }
    });
    let value: serde_json::Value = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?
        .post(assistant_endpoint(&settings.base_url))
        .bearer_auth(&settings.api_key)
        .json(&body)
        .send()?
        .error_for_status()?
        .json()?;
    Ok(value
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string())
}
```

- [ ] **Step 6: Add task metadata DTOs, parser, and save/load command internals**

In `src-tauri/src/models.rs`, add:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTimelinePayload {
    pub version: usize,
    pub session_id: String,
    pub project_slug: String,
    pub messages: Vec<serde_json::Value>,
    pub todos: Vec<serde_json::Value>,
    pub context: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedAgentSessionPayload {
    pub version: usize,
    pub session_id: String,
    pub project_slug: String,
    pub project_root: String,
    pub created_at: String,
    pub messages: Vec<serde_json::Value>,
    pub todos: Vec<serde_json::Value>,
    pub context: serde_json::Value,
    pub llm_messages: Vec<serde_json::Value>,
}
```

In `src-tauri/src/commands.rs`, add:

```rust
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TaskMetadataDraft {
    pub task_name: String,
    pub task_description: String,
}

pub(crate) fn parse_task_metadata_json(content: &str) -> anyhow::Result<TaskMetadataDraft> {
    let value: serde_json::Value = serde_json::from_str(content.trim())?;
    let task_name = value
        .get("task_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if task_name.is_empty() {
        anyhow::bail!("task_name is required");
    }
    let task_description = value
        .get("task_description")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if task_description.is_empty() {
        anyhow::bail!("task_description is required");
    }
    Ok(TaskMetadataDraft { task_name, task_description })
}

fn task_metadata_messages(timeline: &crate::models::AgentTimelinePayload) -> Vec<serde_json::Value> {
    let transcript = timeline.messages.iter()
        .filter_map(|message| {
            let role = message.get("role").and_then(serde_json::Value::as_str)?;
            if role != "user" && role != "assistant" {
                return None;
            }
            let content = message.get("content").and_then(serde_json::Value::as_str).unwrap_or("").trim();
            if content.is_empty() {
                return None;
            }
            Some(format!("{role}: {content}"))
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    vec![
        serde_json::json!({"role": "system", "content": "Return only JSON with task_name and task_description. The task_name must be concise. The task_description must be markdown grounded only in the transcript."}),
        serde_json::json!({"role": "user", "content": transcript}),
    ]
}
```

Add command signatures:

```rust
#[tauri::command]
pub fn save_agent_session(
    app: tauri::AppHandle,
    session_id: String,
    project_slug: String,
    timeline: crate::models::AgentTimelinePayload,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    save_agent_session_inner(app, &services.paths, &session_id, &project_slug, timeline)
        .map(|task| serde_json::to_value(task).unwrap_or_else(|_| serde_json::json!({})))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn load_agent_session(
    app: tauri::AppHandle,
    project_slug: String,
    task_slug: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    load_agent_session_inner(app, &services.paths, &project_slug, &task_slug)
        .map(|payload| serde_json::to_value(payload).unwrap_or_else(|_| serde_json::json!({})))
        .map_err(to_command_error)
}
```

Implement `save_agent_session_inner` with this flow:

```rust
let connection = crate::db::open(paths)?;
crate::db::migrate(&connection)?;
let (project_id, project) = crate::db::get_project_by_slug(&connection, project_slug)?
    .ok_or_else(|| anyhow::anyhow!("Project not found: {project_slug}"))?;
if project.review_status != "reviewed" {
    anyhow::bail!("Task Assistant requires a reviewed project");
}
let settings = crate::config::resolve_llm_settings(
    &crate::config::read_llm_settings(paths)?,
    crate::config::LlmScenario::Assistant,
);
let raw = crate::assistant::llm::request_openai_json(&settings, task_metadata_messages(&timeline))?;
let draft = parse_task_metadata_json(&raw)?;
let base_slug = crate::utils::slugify_lower(&draft.task_name);
let task_slug = crate::db::unique_task_slug(&connection, project_id, &base_slug)?;
let task_dir = paths.projects_dir.join(&project.slug).join("tasks").join(&task_slug);
std::fs::create_dir_all(&task_dir)?;
let description_path = task_dir.join("description.md");
let session_path = task_dir.join("session.json");
std::fs::write(&description_path, format!("{}\n", draft.task_description))?;
let snapshot = assistant_registry(app).session_export(session_id);
let saved = crate::models::SavedAgentSessionPayload {
    version: 1,
    session_id: session_id.to_string(),
    project_slug: project.slug.clone(),
    project_root: project.workdir.clone(),
    created_at: crate::utils::now_rfc3339(),
    messages: timeline.messages,
    todos: timeline.todos,
    context: timeline.context,
    llm_messages: snapshot.llm_messages,
};
std::fs::write(&session_path, serde_json::to_string_pretty(&saved)?)?;
crate::db::upsert_task(
    &connection,
    project_id,
    &task_slug,
    &draft.task_name,
    &draft.task_description,
    "discussing",
    &description_path.to_string_lossy(),
)?;
let task = crate::db::list_tasks(&connection)?
    .into_iter()
    .find(|task| task.project_slug == project.slug && task.slug == task_slug)
    .ok_or_else(|| anyhow::anyhow!("saved task not found after create"))?;
Ok(task)
```

Implement `load_agent_session_inner`:

```rust
let connection = crate::db::open(paths)?;
crate::db::migrate(&connection)?;
let task = crate::db::list_tasks(&connection)?
    .into_iter()
    .find(|task| task.project_slug == project_slug && task.slug == task_slug)
    .ok_or_else(|| anyhow::anyhow!("Task not found: {project_slug}/{task_slug}"))?;
let session_path = task.session_path.ok_or_else(|| anyhow::anyhow!("Task has no saved agent session"))?;
let content = std::fs::read_to_string(&session_path)?;
let saved: crate::models::SavedAgentSessionPayload = serde_json::from_str(&content)?;
let messages = saved.llm_messages.iter()
    .filter_map(|message| {
        let role = message.get("role").and_then(serde_json::Value::as_str)?;
        let content = message.get("content").and_then(serde_json::Value::as_str).unwrap_or("");
        Some(crate::assistant::context::AgentStoredMessage::new(role, content))
    })
    .collect::<Vec<_>>();
let todos = saved.todos.iter()
    .filter_map(|todo| serde_json::from_value::<crate::assistant::tools::AgentTodo>(todo.clone()).ok())
    .collect::<Vec<_>>();
assistant_registry(app).session_import(
    &saved.session_id,
    crate::assistant::AgentSessionSnapshot {
        messages,
        llm_messages: saved.llm_messages.clone(),
        todos,
    },
);
Ok(saved)
```

- [ ] **Step 7: Add frontend types and API wrappers**

In `src/agentTypes.ts`, add:

```ts
export interface AgentTimelinePayload {
  version: 1;
  sessionId: string;
  projectSlug: string;
  messages: AgentMessage[];
  todos: AgentTodoItem[];
  context: AgentContextSnapshot;
}

export interface SavedAgentSession {
  version: number;
  sessionId: string;
  projectSlug: string;
  projectRoot: string;
  createdAt: string;
  messages: AgentMessage[];
  todos: AgentTodoItem[];
  context: AgentContextSnapshot;
  llmMessages: unknown[];
}
```

In `src/api.ts`, add:

```ts
export async function saveAgentSession(
  sessionId: string,
  projectSlug: string,
  timeline: unknown,
): Promise<TaskRecord> {
  if (!isTauriRuntime()) {
    return {
      projectSlug,
      slug: "saved-session",
      title: "Saved Session",
      brief: "Saved assistant session",
      status: "discussing",
      summaryPath: "",
      descriptionPath: "",
      sessionPath: "",
      sessionCount: 0,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
  }
  return invoke<TaskRecord>("save_agent_session", { sessionId, projectSlug, timeline });
}

export async function loadAgentSession(projectSlug: string, taskSlug: string): Promise<SavedAgentSession> {
  if (!isTauriRuntime()) {
    return {
      version: 1,
      sessionId: "browser-session",
      projectSlug,
      projectRoot: "",
      createdAt: new Date().toISOString(),
      messages: [],
      todos: [],
      context: normalizeAgentContext({}),
      llmMessages: [],
    };
  }
  return invoke<SavedAgentSession>("load_agent_session", { projectSlug, taskSlug });
}
```

Import `TaskRecord` and `SavedAgentSession` types as needed.

- [ ] **Step 8: Add Save button and timeline export**

In `src/AgentDrawer.tsx`, add props:

```ts
onSaved?: () => void;
loadedSession?: SavedAgentSession | null;
```

Add Save icon import:

```ts
Save,
```

Add function:

```tsx
async function saveSession() {
  if (!selectedProject) return;
  const timeline = {
    version: 1 as const,
    sessionId,
    projectSlug: selectedProject,
    messages,
    todos,
    context,
  };
  await saveAgentSession(sessionId, selectedProject, timeline);
  onSaved?.();
}
```

Add button before Refresh:

```tsx
<button
  aria-label="Save assistant session"
  className="agent-icon-button"
  disabled={!selectedProject || !messages.some((message) => message.role === "user") || !messages.some((message) => message.role === "assistant")}
  onClick={() => void saveSession()}
>
  <Save size={16} />
</button>
```

Add load effect:

```tsx
useEffect(() => {
  if (!loadedSession) return;
  setMessages(loadedSession.messages);
  setTodos(loadedSession.todos ?? []);
  setContext(normalizeAgentContext(loadedSession.context));
  setSelectedProject(loadedSession.projectSlug);
  setInput("");
  setRunning(false);
}, [loadedSession]);
```

- [ ] **Step 9: Register commands and run focused tests**

Register `save_agent_session` and `load_agent_session` in `src-tauri/src/lib.rs`.

Run:

```bash
npm test -- src/App.test.tsx -t "saves the full drawer timeline and can load it back"
cd src-tauri && cargo test parse_task_metadata_requires_name_and_description
```

Expected: both PASS.

- [ ] **Step 10: Commit**

Run:

```bash
git add src/api.ts src/agentTypes.ts src/AgentDrawer.tsx src/App.test.tsx src-tauri/src/models.rs src-tauri/src/assistant/llm.rs src-tauri/src/commands.rs src-tauri/src/db.rs src-tauri/src/lib.rs
git commit -m "feat: save assistant sessions as tasks"
```

## Task 4: Update Task Detail For Description, Conversation, Load, Delete, And Status

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/styles.css`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Write failing task detail frontend test**

Replace the current task detail metadata expectation test body with:

```tsx
it("renders saved task detail metadata description conversation and load action", async () => {
  vi.spyOn(api, "getAppState").mockResolvedValue({
    ...state,
    tasks: [
      {
        ...state.tasks[0],
        status: "discussing",
        sessionCount: 0,
        createdAt: "2026-04-28T08:00:00Z",
        summaryPath: "/Users/kc/.kittynest/projects/KittyNest/tasks/session-ingest/description.md",
        descriptionPath: "/Users/kc/.kittynest/projects/KittyNest/tasks/session-ingest/description.md",
        sessionPath: "/Users/kc/.kittynest/projects/KittyNest/tasks/session-ingest/session.json",
      },
    ],
    sessions: [],
  });
  vi.spyOn(api, "readMarkdownFile").mockResolvedValue({ content: "Build **Drawer Save**." });
  const loadAgentSession = vi.spyOn(api as ApiWithReviewQueue, "loadAgentSession").mockResolvedValue({
    version: 1,
    sessionId: "saved-session",
    projectSlug: "KittyNest",
    projectRoot: "/Users/kc/KittyNest",
    createdAt: "2026-04-28T08:00:00Z",
    messages: [
      { id: "thinking-1", role: "thinking", content: "planning", status: "finished", expanded: false },
      { id: "tool-1", role: "tool", toolCallId: "call_1", name: "read_file", output: "file", status: "done", expanded: false },
      { id: "user-1", role: "user", content: "Please save this" },
      { id: "assistant-1", role: "assistant", content: "Saved." },
    ],
    todos: [],
    context: { usedTokens: 10, maxTokens: 100, remainingTokens: 90, thinkingTokens: 2, breakdown: { system: 1, user: 3, assistant: 4, tool: 2 } },
    llmMessages: [],
  });

  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: /^tasks$/i }));
  await userEvent.click(screen.getByRole("button", { name: /session ingest/i }));

  expect(screen.getByRole("heading", { name: "Session Ingest" })).toBeInTheDocument();
  expect(screen.getByText("KittyNest")).toBeInTheDocument();
  expect(screen.getByText("2026-04-28T08:00:00Z")).toBeInTheDocument();
  expect(await screen.findByText("Drawer Save", { selector: "strong" })).toBeInTheDocument();
  expect(screen.getByText("Please save this")).toBeInTheDocument();
  expect(screen.getByText("Saved.")).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: /^load$/i }));

  await waitFor(() => expect(loadAgentSession).toHaveBeenCalledWith("KittyNest", "session-ingest"));
  expect(await screen.findByLabelText("Agent Assistant")).toHaveClass("open");
  expect(screen.getByText("planning")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run frontend test to verify it fails**

Run:

```bash
npm test -- src/App.test.tsx -t "renders saved task detail metadata description conversation and load action"
```

Expected: FAIL because the current detail page shows prompt panels and no Load action.

- [ ] **Step 3: Write backend status/delete tests**

In `src-tauri/src/db.rs`, update or add:

```rust
#[test]
fn saved_empty_tasks_can_move_between_all_task_statuses() {
    let temp = tempfile::tempdir().unwrap();
    let paths = crate::models::AppPaths::from_data_dir(temp.path().join("kittynest"));
    let connection = open(&paths).unwrap();
    migrate(&connection).unwrap();
    let project_id = ensure_project_for_workdir(&connection, "/tmp/status-project", "codex", "2026-04-28T08:00:00Z").unwrap();
    let task_dir = paths.projects_dir.join("status-project").join("tasks").join("saved");
    std::fs::create_dir_all(&task_dir).unwrap();
    let description = task_dir.join("description.md");
    let session = task_dir.join("session.json");
    std::fs::write(&description, "Description").unwrap();
    std::fs::write(&session, "{}").unwrap();
    upsert_task(&connection, project_id, "saved", "Saved", "brief", "discussing", &description.to_string_lossy()).unwrap();

    assert!(update_task_status(&connection, "status-project", "saved", "developing").unwrap());
    assert!(update_task_status(&connection, "status-project", "saved", "done").unwrap());
}
```

- [ ] **Step 4: Run backend test to verify it fails**

Run:

```bash
cd src-tauri && cargo test saved_empty_tasks_can_move_between_all_task_statuses
```

Expected: FAIL because empty tasks currently cannot become `developing` or `done`.

- [ ] **Step 5: Relax status rule only for saved tasks**

In `src-tauri/src/db.rs`, change `update_task_status` session-count query to also fetch summary path:

```rust
let task: Option<(i64, String)> = connection
    .query_row(
        r#"
        SELECT COUNT(s.id), t.summary_path
        FROM tasks t
        JOIN projects p ON p.id = t.project_id
        LEFT JOIN sessions s ON s.task_id = t.id
        WHERE t.slug = ?1 AND p.slug = ?2
        GROUP BY t.id
        "#,
        params![task_slug, project_slug],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()?;
let Some((session_count, summary_path)) = task else {
    return Ok(false);
};
let is_saved_agent_task = summary_path.ends_with("/description.md");
if !matches!(status, "discussing" | "developing" | "done") {
    anyhow::bail!("invalid task status: {status}");
}
if session_count == 0 && !is_saved_agent_task && status != "discussing" {
    anyhow::bail!("empty task can only be discussing");
}
```

- [ ] **Step 6: Add Task detail load state in App**

In `src/App.tsx`, import `loadAgentSession` and `type SavedAgentSession`, then add state:

```tsx
const [loadedAgentSession, setLoadedAgentSession] = useState<SavedAgentSession | null>(null);
```

Pass to Drawer:

```tsx
<AgentDrawer
  open={agentDrawerOpen}
  projects={state.projects}
  loadedSession={loadedAgentSession}
  onClose={() => setAgentDrawerOpen(false)}
  onSaved={() => void refreshCached() /* or refresh() if cached misses new task */}
/>
```

Add TaskView prop:

```tsx
onLoad={async () => {
  const loaded = await loadAgentSession(currentTask.projectSlug, currentTask.slug);
  setLoadedAgentSession(loaded);
  setAgentDrawerOpen(true);
}}
```

- [ ] **Step 7: Replace TaskView layout for saved session tasks**

Change `TaskView` props to include `onLoad`:

```tsx
onLoad: () => Promise<void>;
```

Inside `TaskView`, load saved session JSON for conversation:

```tsx
const [savedSession, setSavedSession] = useState<SavedAgentSession | null>(null);
useEffect(() => {
  setSavedSession(null);
  if (!task.sessionPath) return;
  void loadAgentSession(task.projectSlug, task.slug).then(setSavedSession).catch(() => setSavedSession(null));
}, [task.projectSlug, task.slug, task.sessionPath]);
```

Render top card:

```tsx
<PanelTitle
  title={task.title}
  action={
    <div className="panel-actions">
      {task.sessionPath && <IconButton label="Load" icon={<Bot size={16} />} onClick={() => void onLoad()} />}
      <IconButton label="Delete" icon={<Trash2 size={16} />} onClick={onDelete} disabled={!task.sessionPath && task.sessionCount > 0} />
    </div>
  }
/>
<div className="task-meta">
  <span>Project</span>
  <strong>{task.projectSlug}</strong>
  <span>Status</span>
  <strong>{task.status}</strong>
  <span>Created</span>
  <strong>{task.createdAt}</strong>
</div>
```

Render description and conversation:

```tsx
<MarkdownPanel
  title="Task Description"
  path={task.descriptionPath ?? task.summaryPath}
  empty="Task description has not been written yet."
/>
{savedSession && (
  <div className="panel task-conversation-card">
    <h3>Conversation</h3>
    {savedSession.messages
      .filter((message) => message.role === "user" || message.role === "assistant")
      .map((message) => (
        <AgentMessageView key={message.id} message={message} readonly />
      ))}
  </div>
)}
```

Export `AgentMessageView` from `src/AgentDrawer.tsx` and add `readonly?: boolean` so detail rendering can omit action handlers.

- [ ] **Step 8: Add CSS**

In `src/styles.css`, add:

```css
.task-conversation-card {
  display: grid;
  gap: 14px;
}

.task-conversation-card .agent-message {
  max-width: min(780px, 100%);
}

.markdown-panel .markdown-scroll {
  scrollbar-width: none;
}

.markdown-panel .markdown-scroll::-webkit-scrollbar {
  width: 0;
  height: 0;
}
```

- [ ] **Step 9: Run focused tests**

Run:

```bash
npm test -- src/App.test.tsx -t "renders saved task detail metadata description conversation and load action"
cd src-tauri && cargo test saved_empty_tasks_can_move_between_all_task_statuses
```

Expected: both PASS.

- [ ] **Step 10: Commit**

Run:

```bash
git add src/App.tsx src/AgentDrawer.tsx src/App.test.tsx src/styles.css src-tauri/src/db.rs src-tauri/src/commands.rs
git commit -m "feat: load saved assistant tasks from detail"
```

## Task 5: Auto Refresh On Project And Assistant Model Changes

**Files:**
- Modify: `src/AgentDrawer.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`

- [ ] **Step 1: Write failing test for Project select refresh**

Add:

```tsx
it("refreshes assistant context when drawer project changes", async () => {
  vi.spyOn(api, "getAppState").mockResolvedValue({
    ...state,
    projects: [
      { ...state.projects[0], slug: "KittyNest", displayTitle: "KittyNest", reviewStatus: "reviewed" },
      { ...state.projects[0], slug: "Other", displayTitle: "Other", workdir: "/Users/kc/Other", reviewStatus: "reviewed" },
    ],
  });
  const clearAgentSession = vi.spyOn(api as ApiWithReviewQueue, "clearAgentSession").mockResolvedValue({ cleared: true });

  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: /^assistant$/i }));
  await userEvent.click(screen.getByRole("button", { name: /assistant settings/i }));
  await userEvent.selectOptions(screen.getByLabelText("Project"), "Other");

  await waitFor(() => expect(clearAgentSession).toHaveBeenCalledTimes(1));
});
```

- [ ] **Step 2: Write failing test for Assistant Model refresh**

Add:

```tsx
it("refreshes assistant context when assistant model setting changes", async () => {
  const modelA = { ...state.llmSettings, models: [
    { ...state.llmSettings, id: "openrouter-a", remark: "A" },
    { ...state.llmSettings, id: "openrouter-b", remark: "B" },
  ], scenarioModels: { ...state.llmSettings.scenarioModels, defaultModel: "openrouter-a", assistantModel: "openrouter-a" } };
  vi.spyOn(api, "getAppState").mockResolvedValue({ ...state, llmSettings: modelA });
  const saveLlmSettings = vi.spyOn(api, "saveLlmSettings").mockResolvedValue({ saved: true });
  const clearAgentSession = vi.spyOn(api as ApiWithReviewQueue, "clearAgentSession").mockResolvedValue({ cleared: true });

  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: /^assistant$/i }));
  await userEvent.click(screen.getByRole("button", { name: /^settings$/i }));
  await userEvent.selectOptions(screen.getByLabelText("Assistant model"), "openrouter-b");
  await userEvent.click(screen.getByRole("button", { name: /^save$/i }));

  await waitFor(() => expect(saveLlmSettings).toHaveBeenCalledTimes(1));
  await waitFor(() => expect(clearAgentSession).toHaveBeenCalledTimes(1));
});
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
npm test -- src/App.test.tsx -t "refreshes assistant context when"
```

Expected: FAIL because project/model changes do not trigger refresh.

- [ ] **Step 4: Expose imperative refresh trigger**

In `src/AgentDrawer.tsx`, add prop:

```ts
refreshSignal?: number;
```

Add effect:

```tsx
useEffect(() => {
  if (!open || !refreshSignal) return;
  void refreshSession();
}, [refreshSignal]);
```

Change project select handler:

```tsx
onChange={(event) => {
  setSelectedProject(event.target.value);
  void refreshSession();
}}
```

- [ ] **Step 5: Track Assistant Model changes in App**

In `src/App.tsx`, add:

```tsx
const [agentRefreshSignal, setAgentRefreshSignal] = useState(0);
```

Change Settings `onSave`:

```tsx
onSave={(settings) => runAction("Save settings", async () => {
  const previousAssistantModel = state.llmSettings.scenarioModels.assistantModel;
  await saveLlmSettings(settings);
  if (settings.scenarioModels.assistantModel !== previousAssistantModel) {
    setAgentRefreshSignal((value) => value + 1);
  }
  return "LLM settings saved";
}, "cached")}
```

Pass to Drawer:

```tsx
refreshSignal={agentRefreshSignal}
```

- [ ] **Step 6: Run focused tests**

Run:

```bash
npm test -- src/App.test.tsx -t "refreshes assistant context when"
```

Expected: PASS.

- [ ] **Step 7: Commit**

Run:

```bash
git add src/AgentDrawer.tsx src/App.tsx src/App.test.tsx
git commit -m "feat: refresh assistant context on scope changes"
```

## Task 6: Add `read_memory` Assistant Tool

**Files:**
- Create: `src-tauri/src/assistant/tools/read_memory.rs`
- Modify: `src-tauri/src/assistant/tools.rs`
- Modify: `src-tauri/src/assistant/mod.rs`

- [ ] **Step 1: Write failing tool tests**

In `src-tauri/src/assistant/tools.rs` tests, add:

```rust
#[test]
fn read_memory_requires_entity() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = super::ToolEnvironment::for_tests(temp.path());

    let result = super::execute_tool("read_memory", serde_json::json!({}), &mut env);

    assert_eq!(result, "Error: entity is required");
}

#[test]
fn read_memory_returns_empty_message_for_unknown_entity() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = super::ToolEnvironment::for_tests(temp.path());

    let result = super::execute_tool("read_memory", serde_json::json!({"entity": "SQLite"}), &mut env);

    assert!(result.contains("No related memory found for SQLite."));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd src-tauri
cargo test read_memory_requires_entity
cargo test read_memory_returns_empty_message_for_unknown_entity
```

Expected: FAIL because the tool is unknown.

- [ ] **Step 3: Add tool environment app paths**

Change `ToolEnvironment` in `src-tauri/src/assistant/tools.rs`:

```rust
pub struct ToolEnvironment {
    pub project_root: PathBuf,
    pub project_summary_root: PathBuf,
    pub project_slug: String,
    pub app_paths: Option<crate::models::AppPaths>,
    ...
}
```

Initialize `project_slug: String::new()` and `app_paths: None` in `new`.

In `src-tauri/src/assistant/mod.rs`, when creating `ToolEnvironment`, set:

```rust
env.project_slug = project_slug.clone();
env.app_paths = Some(paths.clone());
```

This requires adding `paths: crate::models::AppPaths` and `project_slug: String` to `start_run`, `run_with_llm_for_tests`, and `run_inner`. For tests, pass `crate::models::AppPaths::from_data_dir(temp.path().join("kittynest"))` and `"test-project".to_string()`.

- [ ] **Step 4: Create read_memory tool**

Create `src-tauri/src/assistant/tools/read_memory.rs`:

```rust
use super::{function_schema, ToolEnvironment};

pub fn schema() -> serde_json::Value {
    function_schema(
        "read_memory",
        "Read memories related to an entity from KittyNest memory graph.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "entity": {"type": "string"}
            },
            "required": ["entity"]
        }),
    )
}

pub fn execute(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let Some(entity) = arguments.get("entity").and_then(serde_json::Value::as_str).map(str::trim).filter(|value| !value.is_empty()) else {
        return "Error: entity is required".into();
    };
    let Some(paths) = env.app_paths.as_ref() else {
        return format!("No related memory found for {entity}.");
    };
    let related = match crate::graph::related_sessions_for_entity(paths, entity) {
        Ok(related) => related,
        Err(error) => return format!("Error: {error}"),
    };
    if related.is_empty() {
        return format!("No related memory found for {entity}.");
    }
    let connection = match crate::db::open(paths).and_then(|connection| {
        crate::db::migrate(&connection)?;
        Ok(connection)
    }) {
        Ok(connection) => connection,
        Err(error) => return format!("Error: {error}"),
    };
    let titles = crate::db::list_sessions(&connection)
        .unwrap_or_default()
        .into_iter()
        .map(|session| (session.session_id, session.title.unwrap_or(session.raw_path)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut lines = vec![format!("Memory related to {entity}:")];
    for session in related.into_iter().take(10) {
        lines.push(format!("- Session: {}", titles.get(&session.session_id).cloned().unwrap_or(session.session_id.clone())));
        for memory in crate::db::session_memories_by_session_id(&connection, &session.session_id)
            .unwrap_or_default()
            .into_iter()
            .take(2)
        {
            lines.push(format!("  - {memory}"));
        }
    }
    lines.join("\n")
}
```

- [ ] **Step 5: Register tool**

In `src-tauri/src/assistant/tools.rs`, add:

```rust
mod read_memory;
```

In `tool_schemas`, include:

```rust
read_memory::schema(),
```

In `execute_tool`, add:

```rust
"read_memory" => read_memory::execute(arguments, env),
```

- [ ] **Step 6: Run focused tests**

Run:

```bash
cd src-tauri
cargo test read_memory_requires_entity
cargo test read_memory_returns_empty_message_for_unknown_entity
```

Expected: PASS.

- [ ] **Step 7: Commit**

Run:

```bash
git add src-tauri/src/assistant/tools.rs src-tauri/src/assistant/tools/read_memory.rs src-tauri/src/assistant/mod.rs
git commit -m "feat: add assistant memory lookup tool"
```

## Task 7: Add Interactive `create_task` Assistant Tool And Page Modal

**Files:**
- Create: `src-tauri/src/assistant/tools/create_task.rs`
- Modify: `src-tauri/src/assistant/tools.rs`
- Modify: `src-tauri/src/assistant/mod.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src/api.ts`
- Modify: `src/agentTypes.ts`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/styles.css`

- [ ] **Step 1: Write failing backend create-task wait test**

Add inside `src-tauri/src/assistant/mod.rs` tests:

```rust
#[test]
fn create_task_request_blocks_until_resolved() {
    let emitter = VecEmitter::default();
    let registry = super::AgentRegistry::new_for_tests(emitter.clone());
    let registry_clone = registry.clone();
    let handle = std::thread::spawn(move || {
        registry_clone.request_create_task_wait(
            "session-1",
            "Draft Task",
            "Create **draft** task.",
        )
    });
    std::thread::sleep(std::time::Duration::from_millis(20));
    let event = emitter.events.lock().unwrap()[0].clone();

    assert_eq!(event.event_type, "create_task_request");
    assert!(registry.resolve_create_task(
        "session-1",
        event.request_id.as_deref().unwrap(),
        true,
    ));

    let result = handle.join().unwrap();
    assert!(result.accepted);
}
```

Add inside `src-tauri/src/commands.rs` tests:

```rust
#[test]
fn create_saved_agent_task_from_draft_writes_description_and_session() {
    let temp = tempfile::tempdir().unwrap();
    let paths = crate::models::AppPaths::from_data_dir(temp.path().join("kittynest"));
    let connection = crate::db::open(&paths).unwrap();
    crate::db::migrate(&connection).unwrap();
    crate::db::ensure_project_for_workdir(
        &connection,
        "/tmp/create-tool-project",
        "codex",
        "2026-04-28T08:00:00Z",
    )
    .unwrap();
    let snapshot = crate::assistant::AgentSessionSnapshot {
        messages: vec![crate::assistant::context::AgentStoredMessage::new("user", "make task")],
        llm_messages: vec![serde_json::json!({"role": "user", "content": "make task"})],
        todos: Vec::new(),
    };

    let task = super::create_saved_agent_task_from_draft(
        &paths,
        "create-tool-project",
        "session-1",
        "Create Tool Task",
        "Create **tool** task.",
        snapshot,
    )
    .unwrap();

    assert_eq!(task.status, "discussing");
    assert!(std::path::Path::new(task.description_path.as_deref().unwrap()).exists());
    assert!(std::path::Path::new(task.session_path.as_deref().unwrap()).exists());
}
```

- [ ] **Step 2: Run backend test to verify it fails**

Run:

```bash
cd src-tauri
cargo test create_task_request_blocks_until_resolved
cargo test create_saved_agent_task_from_draft_writes_description_and_session
```

Expected: FAIL because create-task pending state and saved-task helper do not exist.

- [ ] **Step 3: Write failing frontend modal test**

Add:

```tsx
it("renders page-centered create task modal and resolves accept or cancel", async () => {
  vi.spyOn(api, "getAppState").mockResolvedValue({
    ...state,
    projects: [{ ...state.projects[0], reviewStatus: "reviewed" }],
  });
  const resolveAgentCreateTask = vi
    .spyOn(api as ApiWithReviewQueue & {
      resolveAgentCreateTask: (sessionId: string, requestId: string, accepted: boolean) => Promise<{ resolved: boolean }>;
    }, "resolveAgentCreateTask")
    .mockResolvedValue({ resolved: true });

  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: /^assistant$/i }));
  window.dispatchEvent(new CustomEvent("kittynest-agent-event", {
    detail: {
      sessionId: window.sessionStorage.getItem("kittynest:agent-session"),
      type: "create_task_request",
      requestId: "request-create-1",
      title: "Drawer Save",
      description: "Create **saved task**.",
    },
  }));

  expect(await screen.findByRole("dialog", { name: "Create Task" })).toBeInTheDocument();
  expect(screen.getByText("saved task", { selector: "strong" })).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: /^accept$/i }));

  await waitFor(() => expect(resolveAgentCreateTask).toHaveBeenCalledWith(
    expect.any(String),
    "request-create-1",
    true,
  ));
});
```

- [ ] **Step 4: Run frontend test to verify it fails**

Run:

```bash
npm test -- src/App.test.tsx -t "renders page-centered create task modal"
```

Expected: FAIL because event type, modal, and resolver do not exist.

- [ ] **Step 5: Add event type and API wrapper**

In `src/agentTypes.ts`, add `"create_task_request"` to `AgentEventType`.

In `src/api.ts`, add:

```ts
export async function resolveAgentCreateTask(
  sessionId: string,
  requestId: string,
  accepted: boolean,
): Promise<{ resolved: boolean }> {
  if (!isTauriRuntime()) {
    return { resolved: true };
  }
  return invoke<{ resolved: boolean }>("resolve_agent_create_task", { sessionId, requestId, accepted });
}
```

- [ ] **Step 6: Add backend pending create-task state**

In `src-tauri/src/assistant/mod.rs`, add:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateTaskDecision {
    pub accepted: bool,
}

#[derive(Default)]
struct PendingCreateTask {
    response: Mutex<Option<CreateTaskDecision>>,
    available: Condvar,
}
```

Add `pending_create_task: HashMap<String, Arc<PendingCreateTask>>` to `AgentSession`.

Add methods:

```rust
pub fn resolve_create_task(&self, session_id: &str, request_id: &str, accepted: bool) -> bool {
    let pending = {
        let sessions = self.inner.sessions.lock().expect("agent sessions lock poisoned");
        sessions.get(session_id).and_then(|session| session.pending_create_task.get(request_id).cloned())
    };
    let Some(pending) = pending else {
        return false;
    };
    *pending.response.lock().expect("create task response lock poisoned") = Some(CreateTaskDecision { accepted });
    pending.available.notify_all();
    true
}

pub fn request_create_task_wait(&self, session_id: &str, title: &str, description: &str) -> CreateTaskDecision {
    let request_id = self.next_request_id();
    let pending = Arc::new(PendingCreateTask::default());
    {
        let mut sessions = self.inner.sessions.lock().expect("agent sessions lock poisoned");
        sessions.entry(session_id.into()).or_default().pending_create_task.insert(request_id.clone(), pending.clone());
    }
    let mut event = AgentEvent::new(session_id, "create_task_request");
    event.request_id = Some(request_id.clone());
    event.title = Some(title.into());
    event.description = Some(description.into());
    self.emit(event);
    let mut response = pending.response.lock().expect("create task response lock poisoned");
    while response.is_none() {
        response = pending.available.wait(response).expect("create task response lock poisoned");
    }
    let decision = response.take().unwrap_or(CreateTaskDecision { accepted: false });
    if let Some(session) = self.inner.sessions.lock().expect("agent sessions lock poisoned").get_mut(session_id) {
        session.pending_create_task.remove(&request_id);
    }
    decision
}
```

Update `stop_run` to resolve pending create-task waits as canceled.

- [ ] **Step 7: Add resolver command**

In `src-tauri/src/commands.rs`, add:

```rust
#[tauri::command]
pub fn resolve_agent_create_task(
    app: tauri::AppHandle,
    session_id: String,
    request_id: String,
    accepted: bool,
    _services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let resolved = assistant_registry(app).resolve_create_task(&session_id, &request_id, accepted);
    Ok(serde_json::json!({ "resolved": resolved }))
}
```

Register it in `src-tauri/src/lib.rs`.

- [ ] **Step 8: Add create_task tool**

Create `src-tauri/src/assistant/tools/create_task.rs`:

```rust
use super::{function_schema, ToolEnvironment};

pub fn schema() -> serde_json::Value {
    function_schema(
        "create_task",
        "Propose a KittyNest task from the current assistant context and wait for the user to accept or cancel.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_name": {"type": "string"},
                "task_description": {"type": "string"}
            },
            "required": ["task_name", "task_description"]
        }),
    )
}

pub fn execute(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let title = arguments
        .get("task_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim();
    if title.is_empty() {
        return "Error: task_name is required".into();
    }
    let description = arguments
        .get("task_description")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim();
    if description.is_empty() {
        return "Error: task_description is required".into();
    }
    let Some(handler) = env.create_task_handler.as_mut() else {
        return "Error: create_task is unavailable in this context".into();
    };
    handler(title, description)
}
```

Add handler support to `ToolEnvironment`:

```rust
pub create_task_handler: Option<Box<dyn FnMut(&str, &str) -> String + Send>>,
```

Add setter:

```rust
pub fn set_create_task_handler<F>(&mut self, handler: F)
where
    F: FnMut(&str, &str) -> String + Send + 'static,
{
    self.create_task_handler = Some(Box::new(handler));
}
```

Register `create_task` in `tool_schemas` and `execute_tool`.

Add this helper in `src-tauri/src/commands.rs` so both the tool and future command paths can create the same saved task shape:

```rust
pub(crate) fn create_saved_agent_task_from_draft(
    paths: &crate::models::AppPaths,
    project_slug: &str,
    session_id: &str,
    title: &str,
    description: &str,
    snapshot: crate::assistant::AgentSessionSnapshot,
) -> anyhow::Result<crate::models::TaskRecord> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let (project_id, project) = crate::db::get_project_by_slug(&connection, project_slug)?
        .ok_or_else(|| anyhow::anyhow!("Project not found: {project_slug}"))?;
    let task_slug = crate::db::unique_task_slug(
        &connection,
        project_id,
        &crate::utils::slugify_lower(title),
    )?;
    let task_dir = paths.projects_dir.join(&project.slug).join("tasks").join(&task_slug);
    std::fs::create_dir_all(&task_dir)?;
    let description_path = task_dir.join("description.md");
    let session_path = task_dir.join("session.json");
    std::fs::write(&description_path, format!("{}\n", description.trim()))?;
    let messages = snapshot.messages.iter().enumerate().map(|(index, message)| {
        serde_json::json!({
            "id": format!("loaded-{}-{}", message.role, index),
            "role": message.role,
            "content": message.content
        })
    }).collect::<Vec<_>>();
    let saved = crate::models::SavedAgentSessionPayload {
        version: 1,
        session_id: session_id.to_string(),
        project_slug: project.slug.clone(),
        project_root: project.workdir.clone(),
        created_at: crate::utils::now_rfc3339(),
        messages,
        todos: serde_json::to_value(&snapshot.todos)?.as_array().cloned().unwrap_or_default(),
        context: serde_json::json!({}),
        llm_messages: snapshot.llm_messages,
    };
    std::fs::write(&session_path, serde_json::to_string_pretty(&saved)?)?;
    crate::db::upsert_task(
        &connection,
        project_id,
        &task_slug,
        title,
        description,
        "discussing",
        &description_path.to_string_lossy(),
    )?;
    crate::db::list_tasks(&connection)?
        .into_iter()
        .find(|task| task.project_slug == project.slug && task.slug == task_slug)
        .ok_or_else(|| anyhow::anyhow!("saved task not found after create"))
}
```

In `run_inner`, set handler:

```rust
let create_task_registry = self.clone();
let create_task_session_id = session_id.to_string();
let create_task_paths = paths.clone();
let create_task_project_slug = project_slug.clone();
env.set_create_task_handler(move |title, description| {
    let decision = create_task_registry.request_create_task_wait(
        &create_task_session_id,
        title,
        description,
    );
    if !decision.accepted {
        return "Task creation canceled by user.".into();
    }
    match crate::commands::create_saved_agent_task_from_draft(
        &create_task_paths,
        &create_task_project_slug,
        &create_task_session_id,
        title,
        description,
        create_task_registry.session_export(&create_task_session_id),
    ) {
        Ok(task) => format!(
            "Task created: {} at /projects/{}/tasks/{}",
            task.title, task.project_slug, task.slug
        ),
        Err(error) => format!("Error: {error}"),
    }
});
```

- [ ] **Step 9: Add App modal rendering**

In `src/App.tsx`, add state:

```tsx
const [taskProposal, setTaskProposal] = useState<{
  sessionId: string;
  requestId: string;
  title: string;
  description: string;
} | null>(null);
```

Add a window listener for browser-preview events or route this through `AgentDrawer` with an `onCreateTaskRequest` callback. The simplest path is adding a prop to `AgentDrawer`:

```tsx
onCreateTaskRequest?: (proposal: { sessionId: string; requestId: string; title: string; description: string }) => void;
```

In `AgentDrawer.handleAgentEvent`:

```tsx
if (event.type === "create_task_request") {
  onCreateTaskRequest?.({
    sessionId: event.sessionId,
    requestId: event.requestId ?? "",
    title: event.title ?? "Create Task",
    description: event.description ?? "",
  });
  return;
}
```

Render in `App`:

```tsx
{taskProposal && (
  <div className="task-proposal-backdrop" role="presentation">
    <section className="task-proposal-modal" role="dialog" aria-label="Create Task">
      <header>
        <strong>{taskProposal.title}</strong>
      </header>
      <div className="task-proposal-body">
        <MarkdownContent content={taskProposal.description} />
      </div>
      <footer>
        <button onClick={() => {
          void resolveAgentCreateTask(taskProposal.sessionId, taskProposal.requestId, false);
          setTaskProposal(null);
        }}>Cancel</button>
        <button onClick={() => {
          void resolveAgentCreateTask(taskProposal.sessionId, taskProposal.requestId, true);
          setTaskProposal(null);
          void refresh();
        }}>Accept</button>
      </footer>
    </section>
  </div>
)}
```

- [ ] **Step 10: Add modal CSS**

In `src/styles.css`, add:

```css
.task-proposal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 70;
  display: grid;
  place-items: center;
  background: rgba(2, 8, 18, 0.58);
}

.task-proposal-modal {
  width: min(620px, calc(100vw - 32px));
  max-height: min(680px, calc(100vh - 48px));
  border: 1px solid rgba(116, 255, 241, 0.25);
  border-radius: 8px;
  background: rgba(7, 18, 31, 0.98);
  box-shadow: 0 24px 80px rgba(0, 0, 0, 0.42);
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
}

.task-proposal-modal header,
.task-proposal-modal footer {
  padding: 16px;
}

.task-proposal-body {
  min-height: 0;
  overflow-y: auto;
  padding: 0 16px 16px;
  scrollbar-width: none;
}

.task-proposal-body::-webkit-scrollbar {
  width: 0;
}
```

- [ ] **Step 11: Run focused tests**

Run:

```bash
npm test -- src/App.test.tsx -t "renders page-centered create task modal"
cd src-tauri
cargo test create_task_request_blocks_until_resolved
cargo test create_saved_agent_task_from_draft_writes_description_and_session
```

Expected: both PASS.

- [ ] **Step 12: Commit**

Run:

```bash
git add src/api.ts src/agentTypes.ts src/App.tsx src/AgentDrawer.tsx src/App.test.tsx src/styles.css src-tauri/src/assistant/mod.rs src-tauri/src/assistant/tools.rs src-tauri/src/assistant/tools/create_task.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: add interactive assistant task creation"
```

## Task 8: Delete Saved Tasks And Refresh State After Mutations

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`

- [ ] **Step 1: Write failing backend delete test**

In `src-tauri/src/commands.rs` tests, add:

```rust
#[test]
fn delete_saved_task_removes_task_directory() {
    let temp = tempfile::tempdir().unwrap();
    let paths = crate::models::AppPaths::from_data_dir(temp.path().join("kittynest"));
    let connection = crate::db::open(&paths).unwrap();
    crate::db::migrate(&connection).unwrap();
    let project_id = crate::db::ensure_project_for_workdir(
        &connection,
        "/tmp/delete-saved-project",
        "codex",
        "2026-04-28T08:00:00Z",
    )
    .unwrap();
    let task_dir = paths.projects_dir.join("delete-saved-project").join("tasks").join("saved");
    std::fs::create_dir_all(&task_dir).unwrap();
    let description = task_dir.join("description.md");
    std::fs::write(&description, "Description").unwrap();
    std::fs::write(task_dir.join("session.json"), "{}").unwrap();
    crate::db::upsert_task(&connection, project_id, "saved", "Saved", "brief", "discussing", &description.to_string_lossy()).unwrap();

    let deleted = super::delete_task_inner(&paths, "delete-saved-project", "saved").unwrap();

    assert!(deleted);
    assert!(!task_dir.exists());
}
```

- [ ] **Step 2: Run backend test to verify it fails if current delete blocks it**

Run:

```bash
cd src-tauri && cargo test delete_saved_task_removes_task_directory
```

Expected: FAIL if task directory naming or delete guard does not support saved tasks.

- [ ] **Step 3: Update delete behavior carefully**

In `src-tauri/src/commands.rs`, keep existing session-linked protection for scanned tasks. Use this logic:

```rust
pub(crate) fn delete_task_inner(
    paths: &crate::models::AppPaths,
    project_slug: &str,
    task_slug: &str,
) -> anyhow::Result<bool> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let task = crate::db::list_tasks(&connection)?
        .into_iter()
        .find(|task| task.project_slug == project_slug && task.slug == task_slug);
    let Some(task) = task else {
        return Ok(false);
    };
    if task.session_count > 0 && task.session_path.is_none() {
        anyhow::bail!("task has sessions");
    }
    let deleted = crate::db::delete_task_by_slug(&connection, project_slug, task_slug)?;
    if deleted {
        let task_dir = paths.projects_dir.join(project_slug).join("tasks").join(task_slug);
        if task_dir.exists() {
            std::fs::remove_dir_all(task_dir)?;
        }
    }
    Ok(deleted)
}
```

Add `delete_task_by_slug` to `src-tauri/src/db.rs`:

```rust
pub fn delete_task_by_slug(
    connection: &rusqlite::Connection,
    project_slug: &str,
    task_slug: &str,
) -> anyhow::Result<bool> {
    let changed = connection.execute(
        r#"
        DELETE FROM tasks
        WHERE slug = ?1 AND project_id = (SELECT id FROM projects WHERE slug = ?2)
        "#,
        params![task_slug, project_slug],
    )?;
    Ok(changed > 0)
}
```

- [ ] **Step 4: Update App mutation refresh**

In `src/App.tsx`, after successful Save, Delete, Accept create-task, and Load, call `refresh()` rather than only cached refresh when a new task row or deleted row is expected.

Use:

```tsx
onSaved={() => void refresh()}
```

For Delete action keep:

```tsx
setView("tasks");
return "Task deleted";
```

`runAction` already refreshes after returning.

- [ ] **Step 5: Run focused backend test**

Run:

```bash
cd src-tauri && cargo test delete_saved_task_removes_task_directory
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add src/App.tsx src-tauri/src/commands.rs src-tauri/src/db.rs
git commit -m "feat: delete saved assistant tasks"
```

## Task 9: Full Verification And Cleanup

**Files:**
- Modify only files touched by failing verification.

- [ ] **Step 1: Run full frontend test suite**

Run:

```bash
npm test
```

Expected: all frontend tests PASS.

- [ ] **Step 2: Fix any frontend failures with minimal scoped edits**

For a type failure caused by test state missing new task fields, update only the test fixture task object:

```tsx
createdAt: "2026-04-26T01:00:00Z",
descriptionPath: null,
sessionPath: null,
```

For an accessible-name mismatch, prefer changing the test to the real user-facing button label only if the UI label already matches the requested behavior. Otherwise change the UI aria-label.

- [ ] **Step 3: Run full backend test suite**

Run:

```bash
cd src-tauri && cargo test
```

Expected: all Rust tests PASS.

- [ ] **Step 4: Fix any backend failures with minimal scoped edits**

If existing empty-task tests expect the old restriction, update them to distinguish saved tasks from non-saved empty tasks:

```rust
let error = super::update_task_status(&connection, "empty-task", "empty-task", "done")
    .unwrap_err();
assert!(error.to_string().contains("empty task can only be discussing"));
```

Keep the new saved-task test proving saved empty tasks can move to `developing` and `done`.

- [ ] **Step 5: Check worktree scope**

Run:

```bash
git status --short
git diff --stat
```

Expected: only files from this plan are modified; pre-existing `prompt.md` may still appear as an unrelated unstaged user change and must remain untouched.

- [ ] **Step 6: Commit verification fixes if any**

If verification required edits, run:

```bash
git add src src-tauri
git commit -m "test: stabilize assistant task persistence"
```

If no verification edits were needed after Task 8, do not create an empty commit.
