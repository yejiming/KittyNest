# Task Assistant Drawer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a right-side Task Assistant drawer driven by the Settings Assistant model, with Rust tools and Tauri event streaming.

**Architecture:** The frontend owns drawer state, message rendering, and user responses; it starts and controls runs through Tauri commands. The Rust backend owns agent sessions, model streaming, tool execution, permission waits, cancellation, and context estimates; it emits all stream updates through one Tauri event name, `agent://event`.

**Tech Stack:** React 18, TypeScript, Tauri 2 commands/events, Rust, reqwest blocking streaming, serde/serde_json, rusqlite project lookups, Vitest, Cargo tests.

---

## File Structure

- Create `src/AgentDrawer.tsx`: Drawer UI, event subscription, message rendering, settings modal, resize behavior, input controls.
- Create `src/agentTypes.ts`: Shared frontend event/message/context/task-list types plus pure normalization helpers.
- Modify `src/App.tsx`: Remove Create Task card, wire sidebar Assistant button, render drawer.
- Modify `src/api.ts`: Add agent command wrappers and browser-preview fallbacks.
- Do not modify `src/types.ts`; keep drawer-specific DTOs in `src/agentTypes.ts`.
- Modify `src/styles.css`: Remove unused task-create scrollbar selectors, add drawer/modal/message/input/task-list/context styles.
- Modify `src/App.test.tsx`: Update Tasks tests and add drawer integration tests.
- Create `src-tauri/src/assistant_context.rs`: Think-block stream filter and estimated context accounting.
- Create `src-tauri/src/assistant_tools.rs`: Tool schemas and Rust implementations for `read_file`, `grep`, `glob`, `todo_write`, `ask_user`.
- Create `src-tauri/src/assistant_llm.rs`: OpenAI-compatible request body, SSE parser, streamed response aggregation, tool-call parsing.
- Create `src-tauri/src/assistant.rs`: Session registry, run lifecycle, cancellation, permission/ask-user coordination, event emission.
- Modify `src-tauri/src/commands.rs`: Add Tauri commands for start/stop/permission/ask-user.
- Modify `src-tauri/src/lib.rs`: Register assistant modules and commands.
- Modify `src-tauri/Cargo.toml`: Add `glob = "0.3"` for glob pattern matching.

## Task 1: Remove Create Task From Tasks Page

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/styles.css`

- [ ] **Step 1: Write the failing frontend test**

Add this test in `src/App.test.tsx` near the existing Tasks tests, replacing the old manual create-task test:

```tsx
it("shows the tasks table without create task controls", async () => {
  vi.spyOn(api, "getAppState").mockResolvedValue({
    ...state,
    projects: [{ ...state.projects[0], reviewStatus: "reviewed" }],
  });
  const createTask = vi.spyOn(api as ApiWithReviewQueue, "createTask");

  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: /^tasks$/i }));

  expect(screen.getAllByRole("heading", { name: "Tasks" }).length).toBeGreaterThan(0);
  expect(screen.getByRole("button", { name: /session ingest/i })).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Create Task" })).not.toBeInTheDocument();
  expect(screen.queryByLabelText(/task prompt/i)).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /^create task$/i })).not.toBeInTheDocument();
  expect(createTask).not.toHaveBeenCalled();
});
```

- [ ] **Step 2: Run the focused test to verify it fails**

Run:

```bash
npm test -- src/App.test.tsx -t "shows the tasks table without create task controls"
```

Expected: FAIL because the Create Task panel still renders.

- [ ] **Step 3: Remove the Create Task props and form**

In `src/App.tsx`, remove `Plus` only if it becomes unused after later tasks; keep it if Settings still uses it. Remove the `createManualTask` function and stop passing `busy`, `projects`, and `onCreate` into `TasksList`.

Change the Tasks route to:

```tsx
{view === "tasks" && (
  <TasksList
    tasks={state.tasks}
    onOpen={(projectSlug, taskSlug) => {
      setSelectedProject(projectSlug);
      setSelectedTask(taskSlug);
      setView("taskDetail");
    }}
  />
)}
```

Change `TasksList` to:

```tsx
function TasksList({
  tasks,
  onOpen,
}: {
  tasks: TaskRecord[];
  onOpen: (projectSlug: string, taskSlug: string) => void;
}) {
  return (
    <section className="detail-layout">
      <div className="panel wide">
        <PanelTitle title="Tasks" />
        <div className="list-page data-table tasks-table" role="table">
          <div className="table-header" role="row">
            <span role="columnheader">Name</span>
            <span role="columnheader">Project</span>
            <span role="columnheader">Status</span>
            <span role="columnheader">Sessions</span>
          </div>
          {tasks.map((task) => (
            <button key={`${task.projectSlug}-${task.slug}`} className="list-row" onClick={() => onOpen(task.projectSlug, task.slug)}>
              <strong>{task.title}</strong>
              <span>{task.projectSlug}</span>
              <small>{task.status}</small>
              <small>{task.sessionCount}</small>
            </button>
          ))}
          {tasks.length === 0 && <EmptyLine text="No tasks yet." />}
        </div>
      </div>
    </section>
  );
}
```

In `src/styles.css`, remove `.task-create-form textarea` from scrollbar selector groups and remove the `.task-create-form` block plus its nested `label` and `textarea` rules.

- [ ] **Step 4: Run the focused test to verify it passes**

Run:

```bash
npm test -- src/App.test.tsx -t "shows the tasks table without create task controls"
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/App.tsx src/App.test.tsx src/styles.css
git commit -m "refactor: remove manual task creation card"
```

## Task 2: Add Frontend Agent Types And Pure Helpers

**Files:**
- Create: `src/agentTypes.ts`
- Modify: `src/App.test.tsx`

- [ ] **Step 1: Write failing tests for context and event normalization**

Add this describe block at the bottom of `src/App.test.tsx`:

```tsx
describe("agent drawer helpers", () => {
  it("normalizes context snapshots with one decimal shares", async () => {
    const { normalizeAgentContext, contextShareLabel } = await import("./agentTypes");
    const context = normalizeAgentContext({
      usedTokens: 1000,
      maxTokens: 4000,
      remainingTokens: 3000,
      thinkingTokens: 125,
      breakdown: { system: 250, user: 250, assistant: 300, tool: 75 },
    });

    expect(context.usedTokens).toBe(1000);
    expect(contextShareLabel(context, "system")).toBe("25.0%");
    expect(contextShareLabel(context, "thinking")).toBe("12.5%");
    expect(contextShareLabel(context, "tool")).toBe("7.5%");
  });

  it("creates stable ids for tool events using the tool call id", async () => {
    const { messageFromAgentEvent } = await import("./agentTypes");
    const message = messageFromAgentEvent({
      sessionId: "session-1",
      type: "tool_start",
      toolCallId: "call_read",
      name: "read_file",
      arguments: { file_path: "src/App.tsx" },
      summary: "read_file src/App.tsx",
    });

    expect(message).toMatchObject({
      id: "tool-call_read",
      role: "tool",
      toolCallId: "call_read",
      name: "read_file",
    });
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
npm test -- src/App.test.tsx -t "agent drawer helpers"
```

Expected: FAIL because `src/agentTypes.ts` does not exist.

- [ ] **Step 3: Create `src/agentTypes.ts`**

Add:

```ts
export type AgentEventType =
  | "token"
  | "thinking_status"
  | "thinking_delta"
  | "tool_start"
  | "tool_output"
  | "tool_end"
  | "todo_update"
  | "permission_request"
  | "ask_user_request"
  | "done"
  | "cancelled"
  | "error";

export interface AgentContextBreakdown {
  system: number;
  user: number;
  assistant: number;
  tool: number;
}

export interface AgentContextSnapshot {
  usedTokens: number;
  maxTokens: number;
  remainingTokens: number;
  thinkingTokens: number;
  breakdown: AgentContextBreakdown;
}

export interface AgentTodoItem {
  content: string;
  activeForm: string;
  status: "pending" | "in_progress" | "completed";
}

export interface AgentOption {
  label: string;
  value?: string;
  description?: string;
  recommended?: boolean;
  selected?: boolean;
  supplementalInfo?: string;
  inputText?: string;
}

export interface AgentAskQuestion {
  header: string;
  question: string;
  multiSelect: boolean;
  allowFreeformInput: boolean;
  options: AgentOption[];
  freeformInput: string;
}

export interface AgentEvent {
  sessionId: string;
  type: AgentEventType;
  delta?: string;
  status?: string;
  reply?: string;
  error?: string;
  context?: unknown;
  todos?: unknown[];
  toolCallId?: string;
  name?: string;
  summary?: string;
  arguments?: Record<string, unknown>;
  resultPreview?: string;
  requestId?: string;
  title?: string;
  description?: string;
  options?: unknown[];
  questions?: unknown[];
}

export interface AgentMessage {
  id: string;
  role: "user" | "assistant" | "thinking" | "tool" | "permission" | "ask_user" | "error";
  content?: string;
  status?: string;
  expanded?: boolean;
  expandedByUser?: boolean;
  toolCallId?: string;
  name?: string;
  summary?: string;
  arguments?: Record<string, unknown>;
  argumentsText?: string;
  output?: string;
  resultPreview?: string;
  requestId?: string;
  title?: string;
  description?: string;
  options?: AgentOption[];
  questions?: AgentAskQuestion[];
}

const emptyBreakdown: AgentContextBreakdown = {
  system: 0,
  user: 0,
  assistant: 0,
  tool: 0,
};

function numberField(value: unknown): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 ? Math.round(parsed) : 0;
}

export function normalizeAgentContext(value: unknown): AgentContextSnapshot {
  const source = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
  const rawBreakdown = source.breakdown && typeof source.breakdown === "object"
    ? (source.breakdown as Record<string, unknown>)
    : {};
  const breakdown = {
    system: numberField(rawBreakdown.system),
    user: numberField(rawBreakdown.user),
    assistant: numberField(rawBreakdown.assistant),
    tool: numberField(rawBreakdown.tool),
  };
  const breakdownTotal = breakdown.system + breakdown.user + breakdown.assistant + breakdown.tool;
  const usedTokens = Math.max(numberField(source.usedTokens ?? source.used_tokens), breakdownTotal);
  const maxTokens = numberField(source.maxTokens ?? source.max_tokens);
  const remainingTokens = maxTokens > 0
    ? Math.max(0, maxTokens - usedTokens)
    : numberField(source.remainingTokens ?? source.remaining_tokens);
  return {
    usedTokens,
    maxTokens,
    remainingTokens,
    thinkingTokens: numberField(source.thinkingTokens ?? source.thinking_tokens),
    breakdown: { ...emptyBreakdown, ...breakdown },
  };
}

export function contextShareLabel(context: AgentContextSnapshot, key: keyof AgentContextBreakdown | "thinking"): string {
  const value = key === "thinking" ? context.thinkingTokens : context.breakdown[key];
  const percent = context.usedTokens > 0 ? (value / context.usedTokens) * 100 : 0;
  return `${percent.toFixed(1)}%`;
}

export function messageFromAgentEvent(event: AgentEvent): AgentMessage | null {
  if (event.type === "tool_start") {
    const toolCallId = String(event.toolCallId ?? "");
    const args = event.arguments && typeof event.arguments === "object" ? event.arguments : {};
    return {
      id: `tool-${toolCallId || Date.now()}`,
      role: "tool",
      toolCallId,
      name: String(event.name ?? "tool"),
      status: "running",
      summary: String(event.summary ?? event.name ?? "tool"),
      arguments: args,
      argumentsText: JSON.stringify(args, null, 2),
      output: "",
      resultPreview: "",
      expanded: false,
      expandedByUser: false,
    };
  }
  if (event.type === "permission_request") {
    return {
      id: `permission-${String(event.requestId ?? Date.now())}`,
      role: "permission",
      requestId: String(event.requestId ?? ""),
      title: String(event.title ?? "Permission Required"),
      description: String(event.description ?? ""),
      options: normalizeOptions(event.options),
    };
  }
  if (event.type === "ask_user_request") {
    return {
      id: `ask-user-${String(event.requestId ?? Date.now())}`,
      role: "ask_user",
      requestId: String(event.requestId ?? ""),
      title: String(event.title ?? "Need your input"),
      questions: normalizeQuestions(event.questions),
    };
  }
  return null;
}

function normalizeOptions(value: unknown): AgentOption[] {
  if (!Array.isArray(value)) return [];
  return value.map((item) => {
    const option = item && typeof item === "object" ? (item as Record<string, unknown>) : {};
    return {
      label: String(option.label ?? option.value ?? "Option"),
      value: String(option.value ?? option.label ?? ""),
      description: String(option.description ?? ""),
      recommended: Boolean(option.recommended),
      supplementalInfo: String(option.supplementalInfo ?? option.supplemental_info ?? ""),
      selected: false,
      inputText: "",
    };
  });
}

function normalizeQuestions(value: unknown): AgentAskQuestion[] {
  if (!Array.isArray(value)) return [];
  return value.map((item) => {
    const question = item && typeof item === "object" ? (item as Record<string, unknown>) : {};
    return {
      header: String(question.header ?? "Question"),
      question: String(question.question ?? ""),
      multiSelect: Boolean(question.multiSelect),
      allowFreeformInput: question.allowFreeformInput !== false,
      options: normalizeOptions(question.options),
      freeformInput: "",
    };
  });
}
```

- [ ] **Step 4: Run focused tests**

Run:

```bash
npm test -- src/App.test.tsx -t "agent drawer helpers"
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/agentTypes.ts src/App.test.tsx
git commit -m "feat: add agent drawer frontend types"
```

## Task 3: Add Backend Context And Think-Block Helpers

**Files:**
- Create: `src-tauri/src/assistant_context.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing Rust tests in the new module**

Create `src-tauri/src/assistant_context.rs` with only the tests first:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn think_filter_splits_partial_tags() {
        let mut filter = super::ThinkBlockStreamFilter::default();
        let mut events = Vec::new();
        for chunk in ["hello <thi", "nk>hidden</th", "ink> visible"] {
            events.extend(filter.consume(chunk));
        }
        assert_eq!(
            events,
            vec![
                super::ThinkStreamEvent::Visible("hello ".into()),
                super::ThinkStreamEvent::ThinkingStatus("running".into()),
                super::ThinkStreamEvent::ThinkingDelta("hidden".into()),
                super::ThinkStreamEvent::ThinkingStatus("finished".into()),
                super::ThinkStreamEvent::Visible(" visible".into()),
            ]
        );
    }

    #[test]
    fn context_snapshot_counts_roles_and_thinking() {
        let messages = vec![
            super::AgentStoredMessage::new("user", "hello"),
            super::AgentStoredMessage::new("assistant", "<think>hidden</think>visible"),
            super::AgentStoredMessage::new("tool", "tool output"),
        ];
        let snapshot = super::estimate_context("system prompt", &messages, 10_000);

        assert!(snapshot.used_tokens > 0);
        assert!(snapshot.breakdown.system > 0);
        assert!(snapshot.breakdown.user > 0);
        assert!(snapshot.breakdown.assistant > 0);
        assert!(snapshot.breakdown.tool > 0);
        assert!(snapshot.thinking_tokens > 0);
        assert_eq!(snapshot.max_tokens, 10_000);
    }
}
```

Add `pub mod assistant_context;` to `src-tauri/src/lib.rs` so the module compiles.

- [ ] **Step 2: Run the module test to verify it fails**

Run:

```bash
cd src-tauri
cargo test assistant_context
```

Expected: FAIL with missing `ThinkBlockStreamFilter`, `ThinkStreamEvent`, `AgentStoredMessage`, and `estimate_context`.

- [ ] **Step 3: Implement the helper module**

Replace the file with:

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThinkStreamEvent {
    Visible(String),
    ThinkingStatus(String),
    ThinkingDelta(String),
}

#[derive(Default)]
pub struct ThinkBlockStreamFilter {
    inside_think: bool,
    pending: String,
    seen_think: bool,
}

impl ThinkBlockStreamFilter {
    pub fn consume(&mut self, chunk: &str) -> Vec<ThinkStreamEvent> {
        let text = format!("{}{}", self.pending, chunk);
        self.pending.clear();
        let mut events = Vec::new();
        let mut visible = String::new();
        let mut thinking = String::new();
        let mut index = 0;

        while index < text.len() {
            let remainder = &text[index..];
            if remainder.starts_with("<think>") {
                if !visible.is_empty() {
                    events.push(ThinkStreamEvent::Visible(std::mem::take(&mut visible)));
                }
                self.inside_think = true;
                self.seen_think = true;
                events.push(ThinkStreamEvent::ThinkingStatus("running".into()));
                index += "<think>".len();
                continue;
            }
            if remainder.starts_with("</think>") {
                if !thinking.is_empty() {
                    events.push(ThinkStreamEvent::ThinkingDelta(std::mem::take(&mut thinking)));
                }
                self.inside_think = false;
                events.push(ThinkStreamEvent::ThinkingStatus("finished".into()));
                index += "</think>".len();
                continue;
            }
            if remainder.starts_with('<')
                && ("<think>".starts_with(remainder) || "</think>".starts_with(remainder))
            {
                self.pending = remainder.to_string();
                break;
            }
            let Some(character) = remainder.chars().next() else {
                break;
            };
            if self.inside_think {
                thinking.push(character);
            } else {
                visible.push(character);
            }
            index += character.len_utf8();
        }

        if !visible.is_empty() {
            events.push(ThinkStreamEvent::Visible(visible));
        }
        if !thinking.is_empty() {
            events.push(ThinkStreamEvent::ThinkingDelta(thinking));
        }
        events
    }

    pub fn needs_finish_event(&self) -> bool {
        self.seen_think && self.inside_think
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentStoredMessage {
    pub role: String,
    pub content: String,
}

impl AgentStoredMessage {
    pub fn new(role: &str, content: &str) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentContextBreakdown {
    pub system: usize,
    pub user: usize,
    pub assistant: usize,
    pub tool: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentContextSnapshot {
    pub used_tokens: usize,
    pub max_tokens: usize,
    pub remaining_tokens: usize,
    pub thinking_tokens: usize,
    pub breakdown: AgentContextBreakdown,
}

pub fn estimate_context(
    system_prompt: &str,
    messages: &[AgentStoredMessage],
    max_tokens: usize,
) -> AgentContextSnapshot {
    let mut breakdown = AgentContextBreakdown {
        system: estimate_tokens(system_prompt),
        user: 0,
        assistant: 0,
        tool: 0,
    };
    let mut thinking_tokens = 0;

    for message in messages {
        let tokens = estimate_tokens(&message.content);
        match message.role.as_str() {
            "user" => breakdown.user += tokens,
            "assistant" => {
                breakdown.assistant += tokens;
                thinking_tokens += estimate_thinking_tokens(&message.content);
            }
            "tool" => breakdown.tool += tokens,
            _ => {}
        }
    }

    let used_tokens = breakdown.system + breakdown.user + breakdown.assistant + breakdown.tool;
    AgentContextSnapshot {
        used_tokens,
        max_tokens,
        remaining_tokens: max_tokens.saturating_sub(used_tokens),
        thinking_tokens,
        breakdown,
    }
}

fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

fn estimate_thinking_tokens(text: &str) -> usize {
    let mut total = 0;
    let mut rest = text;
    while let Some(start) = rest.find("<think>") {
        let after_start = &rest[start + "<think>".len()..];
        let Some(end) = after_start.find("</think>") else {
            total += estimate_tokens(after_start);
            break;
        };
        total += estimate_tokens(&after_start[..end]);
        rest = &after_start[end + "</think>".len()..];
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn think_filter_splits_partial_tags() {
        let mut filter = ThinkBlockStreamFilter::default();
        let mut events = Vec::new();
        for chunk in ["hello <thi", "nk>hidden</th", "ink> visible"] {
            events.extend(filter.consume(chunk));
        }
        assert_eq!(
            events,
            vec![
                ThinkStreamEvent::Visible("hello ".into()),
                ThinkStreamEvent::ThinkingStatus("running".into()),
                ThinkStreamEvent::ThinkingDelta("hidden".into()),
                ThinkStreamEvent::ThinkingStatus("finished".into()),
                ThinkStreamEvent::Visible(" visible".into()),
            ]
        );
    }

    #[test]
    fn context_snapshot_counts_roles_and_thinking() {
        let messages = vec![
            AgentStoredMessage::new("user", "hello"),
            AgentStoredMessage::new("assistant", "<think>hidden</think>visible"),
            AgentStoredMessage::new("tool", "tool output"),
        ];
        let snapshot = estimate_context("system prompt", &messages, 10_000);

        assert!(snapshot.used_tokens > 0);
        assert!(snapshot.breakdown.system > 0);
        assert!(snapshot.breakdown.user > 0);
        assert!(snapshot.breakdown.assistant > 0);
        assert!(snapshot.breakdown.tool > 0);
        assert!(snapshot.thinking_tokens > 0);
        assert_eq!(snapshot.max_tokens, 10_000);
    }
}
```

- [ ] **Step 4: Run the module tests**

Run:

```bash
cd src-tauri
cargo test assistant_context
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src-tauri/src/assistant_context.rs src-tauri/src/lib.rs
git commit -m "feat: add assistant context helpers"
```

## Task 4: Add Rust Tool Schemas And Tool Execution

**Files:**
- Create: `src-tauri/src/assistant_tools.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Write failing Rust tests for tools**

Create `src-tauri/src/assistant_tools.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    #[test]
    fn read_file_returns_line_numbers_inside_project() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("src.txt");
        std::fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();
        let mut env = super::ToolEnvironment::for_tests(temp.path());

        let result = super::execute_tool(
            "read_file",
            serde_json::json!({"file_path": file, "offset": 2, "limit": 1}),
            &mut env,
        );

        assert_eq!(result, "2\tbeta");
    }

    #[test]
    fn grep_searches_matching_lines() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.rs"), "fn main() {}\nlet needle = true;\n").unwrap();
        let mut env = super::ToolEnvironment::for_tests(temp.path());

        let result = super::execute_tool(
            "grep",
            serde_json::json!({"pattern": "needle", "path": temp.path(), "include": "*.rs"}),
            &mut env,
        );

        assert!(result.contains("a.rs:2: let needle = true;"));
    }

    #[test]
    fn glob_lists_matching_files() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/App.tsx"), "").unwrap();
        let mut env = super::ToolEnvironment::for_tests(temp.path());

        let result = super::execute_tool(
            "glob",
            serde_json::json!({"pattern": "src/**/*.tsx", "path": temp.path()}),
            &mut env,
        );

        assert!(result.contains("App.tsx"));
    }

    #[test]
    fn todo_write_stores_unfinished_items() {
        let temp = tempfile::tempdir().unwrap();
        let mut env = super::ToolEnvironment::for_tests(temp.path());

        let result = super::execute_tool(
            "todo_write",
            serde_json::json!({
                "todos": [
                    {"content": "Ship drawer", "active_form": "Shipping drawer", "status": "in_progress"},
                    {"content": "Run tests", "active_form": "Running tests", "status": "pending"}
                ]
            }),
            &mut env,
        );

        assert!(result.contains("Todo list updated."));
        assert_eq!(env.todos.len(), 2);
    }

    #[test]
    fn outside_path_requests_permission() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("secret.txt");
        std::fs::write(&file, "secret").unwrap();
        let decisions = Arc::new(Mutex::new(vec!["deny".to_string()]));
        let mut env = super::ToolEnvironment::for_tests(temp.path())
            .with_permission_decisions(Arc::clone(&decisions));

        let result = super::execute_tool(
            "read_file",
            serde_json::json!({"file_path": file}),
            &mut env,
        );

        assert!(result.contains("User denied permission"));
        assert_eq!(env.permission_requests.len(), 1);
    }
}
```

Add `pub mod assistant_tools;` to `src-tauri/src/lib.rs`. Add `glob = "0.3"` to `[dependencies]` in `src-tauri/Cargo.toml`.

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd src-tauri
cargo test assistant_tools
```

Expected: FAIL with missing tool types/functions.

- [ ] **Step 3: Implement tool environment, schemas, and execution**

Implement `src-tauri/src/assistant_tools.rs` with these public shapes:

```rust
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use glob::Pattern;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

const SKIP_DIRS: &[&str] = &[".git", "node_modules", "__pycache__", ".venv", "venv", ".tox", "dist", "build"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTodo {
    pub content: String,
    pub active_form: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRequestRecord {
    pub title: String,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct PermissionDecision {
    pub value: String,
    pub supplemental_info: String,
}

pub struct ToolEnvironment {
    pub project_root: PathBuf,
    pub todos: Vec<AgentTodo>,
    pub permission_requests: Vec<PermissionRequestRecord>,
    permission_decisions: Option<Arc<Mutex<Vec<String>>>>,
    ask_user_handler: Option<Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send>>,
}

impl ToolEnvironment {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            todos: Vec::new(),
            permission_requests: Vec::new(),
            permission_decisions: None,
            ask_user_handler: None,
        }
    }

    #[cfg(test)]
    pub fn for_tests(project_root: &Path) -> Self {
        Self::new(project_root.to_path_buf())
    }

    #[cfg(test)]
    pub fn with_permission_decisions(mut self, decisions: Arc<Mutex<Vec<String>>>) -> Self {
        self.permission_decisions = Some(decisions);
        self
    }

    pub fn set_ask_user_handler<F>(&mut self, handler: F)
    where
        F: FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + 'static,
    {
        self.ask_user_handler = Some(Box::new(handler));
    }

    pub fn request_permission(&mut self, title: &str, description: &str) -> PermissionDecision {
        self.permission_requests.push(PermissionRequestRecord {
            title: title.into(),
            description: description.into(),
        });
        let value = self
            .permission_decisions
            .as_ref()
            .and_then(|items| items.lock().ok()?.pop())
            .unwrap_or_else(|| "deny".into());
        PermissionDecision {
            value,
            supplemental_info: String::new(),
        }
    }
}

pub fn tool_schemas() -> Vec<serde_json::Value> {
    vec![
        function_schema("read_file", "Read a file's contents with line numbers.", serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {"type": "string"},
                "offset": {"type": "integer"},
                "limit": {"type": "integer"}
            },
            "required": ["file_path"]
        })),
        function_schema("grep", "Search file contents with regex.", serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string"},
                "path": {"type": "string"},
                "include": {"type": "string"}
            },
            "required": ["pattern"]
        })),
        function_schema("glob", "Find files matching a glob pattern.", serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string"},
                "path": {"type": "string"}
            },
            "required": ["pattern"]
        })),
        function_schema("todo_write", "Create and manage the current task list.", serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {"type": "string"},
                            "active_form": {"type": "string"},
                            "status": {"type": "string"}
                        },
                        "required": ["content", "active_form", "status"]
                    }
                }
            },
            "required": ["todos"]
        })),
        function_schema("ask_user", "Ask the user clarifying questions.", serde_json::json!({
            "type": "object",
            "properties": {"questions": {"type": "array"}},
            "required": ["questions"]
        })),
    ]
}

fn function_schema(name: &str, description: &str, parameters: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters
        }
    })
}
```

Add the implementations:

```rust
pub fn execute_tool(name: &str, arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    match name {
        "read_file" => read_file(arguments, env),
        "grep" => grep(arguments, env),
        "glob" => glob_files(arguments, env),
        "todo_write" => todo_write(arguments, env),
        "ask_user" => ask_user(arguments, env),
        _ => format!("Error: unknown tool '{name}'"),
    }
}
```

Implement helpers with these rules:

```rust
fn resolve_tool_path(raw: Option<&str>, default: &Path, env: &mut ToolEnvironment) -> Result<PathBuf, String> {
    let requested = raw.map(PathBuf::from).unwrap_or_else(|| default.to_path_buf());
    let absolute = if requested.is_absolute() {
        requested
    } else {
        env.project_root.join(requested)
    };
    let canonical = absolute.canonicalize().map_err(|error| format!("Error: {error}"))?;
    let project_root = env.project_root.canonicalize().map_err(|error| format!("Error: {error}"))?;
    if canonical.starts_with(&project_root) {
        return Ok(canonical);
    }
    let decision = env.request_permission(
        "File Permission",
        &format!("Allow the assistant to access this path outside the selected project?\n\n{}", canonical.display()),
    );
    if decision.value == "allow" {
        Ok(canonical)
    } else {
        Err("User denied permission grant".into())
    }
}
```

Complete `read_file`, `grep`, `glob_files`, `todo_write`, and `ask_user` using the behavior from the design spec and KittyCopilot references. Keep all returned errors as strings beginning with `Error:` except denied permission, which must include `User denied permission grant`.

- [ ] **Step 4: Run tool tests**

Run:

```bash
cd src-tauri
cargo test assistant_tools
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/assistant_tools.rs src-tauri/src/lib.rs
git commit -m "feat: add assistant rust tools"
```

## Task 5: Add OpenAI-Compatible Streaming Parser

**Files:**
- Create: `src-tauri/src/assistant_llm.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write parser tests first**

Create `src-tauri/src/assistant_llm.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn sse_parser_collects_tokens_and_tool_call_arguments() {
        let input = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hi \"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"file_path\\\":\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"src/App.tsx\\\"}\"}}]}}]}\n\n",
            "data: [DONE]\n\n"
        );

        let response = super::parse_openai_sse(input.as_bytes(), |_| {}).unwrap();

        assert_eq!(response.content, "Hi ");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call_1");
        assert_eq!(response.tool_calls[0].name, "read_file");
        assert_eq!(
            response.tool_calls[0].arguments,
            serde_json::json!({"file_path": "src/App.tsx"})
        );
    }

    #[test]
    fn openai_body_uses_max_completion_tokens_and_tools() {
        let mut settings = crate::config::default_llm_settings();
        settings.model = "openai/gpt-4o-mini".into();
        settings.max_tokens = 123;
        let body = super::openai_stream_body(
            &settings,
            vec![serde_json::json!({"role": "user", "content": "hello"})],
            vec![serde_json::json!({"type": "function", "function": {"name": "read_file", "parameters": {"type": "object"}}})],
        );

        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 123);
        assert_eq!(body["max_completion_tokens"], 123);
        assert_eq!(body["tools"][0]["function"]["name"], "read_file");
    }
}
```

Add `pub mod assistant_llm;` to `src-tauri/src/lib.rs`.

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd src-tauri
cargo test assistant_llm
```

Expected: FAIL with missing parser and body functions.

- [ ] **Step 3: Implement LLM streaming parser and request body**

Add public structs:

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct AssistantToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssistantLlmResponse {
    pub content: String,
    pub tool_calls: Vec<AssistantToolCall>,
}
```

Implement:

```rust
pub fn openai_stream_body(
    settings: &crate::models::LlmSettings,
    messages: Vec<serde_json::Value>,
    tools: Vec<serde_json::Value>,
) -> serde_json::Value {
    let max_tokens = if settings.max_tokens == 0 { 4096 } else { settings.max_tokens };
    let mut body = serde_json::json!({
        "model": settings.model,
        "messages": messages,
        "stream": true,
        "max_tokens": max_tokens,
        "max_completion_tokens": max_tokens,
        "temperature": if settings.temperature.is_finite() { settings.temperature } else { 0.2 }
    });
    if !tools.is_empty() {
        body["tools"] = serde_json::Value::Array(tools);
    }
    body
}
```

Implement `parse_openai_sse<R, F>(reader: R, on_token: F) -> anyhow::Result<AssistantLlmResponse>` where `R: std::io::Read` and `F: FnMut(&str)`. It must:

- Read lines through `std::io::BufReader`.
- Ignore non-`data:` lines.
- Stop on `data: [DONE]`.
- Append `choices[0].delta.content` to `content` and call `on_token`.
- Merge `choices[0].delta.tool_calls[*]` by `index`.
- Parse concatenated function arguments as JSON; if parsing fails, use `{}`.

Add:

```rust
pub fn request_openai_stream<F>(
    settings: &crate::models::LlmSettings,
    messages: Vec<serde_json::Value>,
    tools: Vec<serde_json::Value>,
    on_token: F,
) -> anyhow::Result<AssistantLlmResponse>
where
    F: FnMut(&str),
{
    if settings.interface != "openai" {
        anyhow::bail!("Task Assistant currently requires an OpenAI-compatible Assistant model");
    }
    if !crate::llm::configured_for_remote(settings) {
        anyhow::bail!("LLM settings are incomplete");
    }
    let endpoint = assistant_endpoint(&settings.base_url);
    let body = openai_stream_body(settings, messages, tools);
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?
        .post(endpoint)
        .bearer_auth(&settings.api_key)
        .json(&body)
        .send()?
        .error_for_status()?;
    parse_openai_sse(response, on_token)
}
```

Keep `assistant_endpoint` local to this module and mirror existing `llm.rs` OpenAI endpoint behavior.

- [ ] **Step 4: Run parser tests**

Run:

```bash
cd src-tauri
cargo test assistant_llm
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src-tauri/src/assistant_llm.rs src-tauri/src/lib.rs
git commit -m "feat: add assistant llm streaming parser"
```

## Task 6: Add Assistant Session Runtime

**Files:**
- Create: `src-tauri/src/assistant.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write runtime tests first**

Create `src-tauri/src/assistant.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct VecEmitter {
        events: Arc<Mutex<Vec<super::AgentEvent>>>,
    }

    impl super::AgentEventEmitter for VecEmitter {
        fn emit(&self, event: &super::AgentEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    #[test]
    fn permission_request_blocks_until_resolved() {
        let emitter = VecEmitter::default();
        let registry = super::AgentRegistry::new_for_tests(emitter.clone());
    let request_id = registry.create_permission_request(
            "session-1",
            "File Permission",
            "Read outside project?",
        );

        assert_eq!(emitter.events.lock().unwrap()[0].event_type, "permission_request");
        assert!(registry.resolve_permission("session-1", &request_id, "allow", ""));
    }

    #[test]
    fn stop_marks_session_cancelled() {
        let registry = super::AgentRegistry::new_for_tests(VecEmitter::default());
        registry.ensure_session("session-1");

        assert!(registry.stop_run("session-1"));
        assert!(registry.is_cancelled("session-1"));
    }
}
```

Add `pub mod assistant;` to `src-tauri/src/lib.rs`.

- [ ] **Step 2: Run runtime tests to verify they fail**

Run:

```bash
cd src-tauri
cargo test assistant::tests
```

Expected: FAIL with missing runtime types.

- [ ] **Step 3: Implement event DTOs, emitter trait, and registry**

Implement these public shapes:

```rust
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub session_id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub questions: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub todos: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

pub trait AgentEventEmitter: Clone + Send + Sync + 'static {
    fn emit(&self, event: &AgentEvent);
}
```

Implement `AgentRegistry<E: AgentEventEmitter>` with:

- `new(emitter: E) -> Self`
- `new_for_tests(emitter: E) -> Self`
- `ensure_session(&self, session_id: &str)`
- `stop_run(&self, session_id: &str) -> bool`
- `is_cancelled(&self, session_id: &str) -> bool`
- `resolve_permission(&self, session_id: &str, request_id: &str, value: &str, supplemental_info: &str) -> bool`
- `resolve_ask_user(&self, session_id: &str, request_id: &str, answers: serde_json::Value) -> bool`
- `create_permission_request(&self, session_id: &str, title: &str, description: &str) -> String`

Use `uuid` is not available; create request ids with `format!("request-{}-{}", counter, crate::utils::now_rfc3339().replace(':', "").replace('.', "").replace('-', ""))` and an atomic counter inside the registry. Add `std::sync::atomic::AtomicUsize`.

- [ ] **Step 4: Run runtime tests**

Run:

```bash
cd src-tauri
cargo test assistant::tests
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src-tauri/src/assistant.rs src-tauri/src/lib.rs
git commit -m "feat: add assistant session runtime"
```

## Task 7: Connect Agent Loop To Tools And LLM

**Files:**
- Modify: `src-tauri/src/assistant.rs`
- Modify: `src-tauri/src/assistant_tools.rs`
- Modify: `src-tauri/src/assistant_llm.rs`

- [ ] **Step 1: Add agent-loop tests with fake LLM**

In `src-tauri/src/assistant.rs`, add tests:

```rust
#[test]
fn run_with_fake_llm_streams_token_and_done() {
    let emitter = VecEmitter::default();
    let registry = super::AgentRegistry::new_for_tests(emitter.clone());
    let settings = crate::config::default_llm_settings();
    let project_root = tempfile::tempdir().unwrap();
    registry.run_with_llm_for_tests(
        "session-1",
        project_root.path().to_path_buf(),
        settings,
        "hello",
        |_messages, _tools, on_token| {
            on_token("Hello");
            Ok(crate::assistant_llm::AssistantLlmResponse {
                content: "Hello".into(),
                tool_calls: Vec::new(),
            })
        },
    );

    let events = emitter.events.lock().unwrap();
    assert_eq!(events[0].event_type, "token");
    assert_eq!(events.last().unwrap().event_type, "done");
}

#[test]
fn run_with_fake_llm_executes_todo_tool_as_task_list_update() {
    let emitter = VecEmitter::default();
    let registry = super::AgentRegistry::new_for_tests(emitter.clone());
    let settings = crate::config::default_llm_settings();
    let project_root = tempfile::tempdir().unwrap();
    let mut rounds = 0;

    registry.run_with_llm_for_tests(
        "session-1",
        project_root.path().to_path_buf(),
        settings,
        "plan",
        |_messages, _tools, _on_token| {
            rounds += 1;
            if rounds == 1 {
                Ok(crate::assistant_llm::AssistantLlmResponse {
                    content: String::new(),
                    tool_calls: vec![crate::assistant_llm::AssistantToolCall {
                        id: "todo_1".into(),
                        name: "todo_write".into(),
                        arguments: serde_json::json!({
                            "todos": [
                                {"content": "Ship drawer", "active_form": "Shipping drawer", "status": "in_progress"}
                            ]
                        }),
                    }],
                })
            } else {
                Ok(crate::assistant_llm::AssistantLlmResponse {
                    content: "Ready".into(),
                    tool_calls: Vec::new(),
                })
            }
        },
    );

    let events = emitter.events.lock().unwrap();
    assert!(events.iter().any(|event| event.event_type == "todo_update"));
    assert_eq!(events.last().unwrap().event_type, "done");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd src-tauri
cargo test assistant::tests
```

Expected: FAIL with missing `run_with_llm_for_tests`.

- [ ] **Step 3: Implement the loop**

Add a run function that accepts a closure for testability:

```rust
pub fn run_with_llm_for_tests<F>(
    &self,
    session_id: &str,
    project_root: PathBuf,
    settings: crate::models::LlmSettings,
    user_input: &str,
    mut llm: F,
) where
    F: FnMut(
        Vec<serde_json::Value>,
        Vec<serde_json::Value>,
        &mut dyn FnMut(&str),
    ) -> anyhow::Result<crate::assistant_llm::AssistantLlmResponse>,
{
    let result = self.run_inner(session_id, project_root, settings, user_input, &mut llm);
    if let Err(error) = result {
        self.emit_error(session_id, &error.to_string());
    }
}
```

Implement `run_inner` with up to 50 rounds. It must:

- Store a user message.
- Build system prompt with the project root and available tools.
- Emit token/thinking events through `ThinkBlockStreamFilter`.
- Suppress visible tool cards for `todo_write`; emit `todo_update` after it executes.
- Emit tool cards for `read_file`, `grep`, `glob`, and `ask_user`.
- Append tool results and continue.
- Emit `done` with visible assistant content and context snapshot when no tool calls remain.

For production, add:

```rust
pub fn start_run(
    &self,
    session_id: String,
    project_root: PathBuf,
    settings: crate::models::LlmSettings,
    message: String,
) {
    let registry = self.clone();
    std::thread::spawn(move || {
        registry.run_with_llm_for_tests(&session_id, project_root, settings, &message, |messages, tools, on_token| {
            crate::assistant_llm::request_openai_stream(&settings, messages, tools, on_token)
        });
    });
}
```

- [ ] **Step 4: Run runtime tests**

Run:

```bash
cd src-tauri
cargo test assistant::tests
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src-tauri/src/assistant.rs src-tauri/src/assistant_tools.rs src-tauri/src/assistant_llm.rs
git commit -m "feat: connect assistant loop to tools"
```

## Task 8: Add Tauri Commands And Event Emitter

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add command tests for project validation and model resolution**

In `src-tauri/src/commands.rs` tests module, add:

```rust
#[test]
fn assistant_project_root_requires_reviewed_project() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
    crate::config::initialize_workspace(&paths).unwrap();
    let mut connection = crate::db::open(&paths).unwrap();
    crate::db::migrate(&connection).unwrap();
    let project_dir = temp.path().join("app");
    std::fs::create_dir_all(&project_dir).unwrap();
    crate::db::upsert_raw_sessions(
        &mut connection,
        &[RawSession {
            source: "codex".into(),
            session_id: "assistant-project-root".into(),
            workdir: project_dir.to_string_lossy().to_string(),
            created_at: "2026-04-28T00:00:00Z".into(),
            updated_at: "2026-04-28T00:00:01Z".into(),
            raw_path: temp.path().join("session.jsonl").to_string_lossy().to_string(),
            messages: vec![RawMessage {
                role: "user".into(),
                content: "hello".into(),
            }],
        }],
    )
    .unwrap();

    let error = super::assistant_project_root(&paths, "app").unwrap_err().to_string();

    assert!(error.contains("reviewed"));
}
```

- [ ] **Step 2: Run command test to verify it fails**

Run:

```bash
cd src-tauri
cargo test assistant_project_root_requires_reviewed_project
```

Expected: FAIL with missing `assistant_project_root`.

- [ ] **Step 3: Implement command helpers and Tauri commands**

Add an app-handle emitter:

```rust
#[derive(Clone)]
pub struct TauriAgentEmitter {
    app: tauri::AppHandle,
}

impl crate::assistant::AgentEventEmitter for TauriAgentEmitter {
    fn emit(&self, event: &crate::assistant::AgentEvent) {
        let _ = self.app.emit("agent://event", event);
    }
}
```

Add commands:

```rust
#[tauri::command]
pub fn start_agent_run(
    app: tauri::AppHandle,
    session_id: String,
    project_slug: String,
    message: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let project_root = assistant_project_root(&services.paths, &project_slug).map_err(to_command_error)?;
    let settings = crate::config::resolve_llm_settings(
        &crate::config::read_llm_settings(&services.paths).map_err(to_command_error)?,
        crate::config::LlmScenario::Assistant,
    );
    crate::assistant::global_registry(TauriAgentEmitter { app })
        .start_run(session_id, project_root, settings, message);
    Ok(serde_json::json!({"started": true}))
}
```

Also add:

- `stop_agent_run(session_id, app, services)` returning `{ "stopped": bool }`
- `resolve_agent_permission(session_id, request_id, value, supplemental_info, app, services)` returning `{ "resolved": bool }`
- `resolve_agent_ask_user(session_id, request_id, answers, app, services)` returning `{ "resolved": bool }`

Implement `assistant_project_root(paths, project_slug)` by opening the DB, listing projects, selecting matching slug, requiring `review_status == "reviewed"`, and returning `PathBuf::from(project.workdir)`.

Register the commands in `src-tauri/src/lib.rs` inside `tauri::generate_handler!`.

- [ ] **Step 4: Run command tests**

Run:

```bash
cd src-tauri
cargo test assistant_project_root_requires_reviewed_project
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: expose assistant tauri commands"
```

## Task 9: Add API Command Wrappers

**Files:**
- Modify: `src/api.ts`
- Modify: `src/App.test.tsx`

- [ ] **Step 1: Write API wrapper expectations in frontend tests**

Update the `ApiWithReviewQueue` type in `src/App.test.tsx`:

```ts
  startAgentRun: (sessionId: string, projectSlug: string, message: string) => Promise<{ started: boolean }>;
  stopAgentRun: (sessionId: string) => Promise<{ stopped: boolean }>;
  resolveAgentPermission: (sessionId: string, requestId: string, value: string, supplementalInfo: string) => Promise<{ resolved: boolean }>;
  resolveAgentAskUser: (sessionId: string, requestId: string, answers: Record<string, unknown>) => Promise<{ resolved: boolean }>;
```

No runtime assertion is needed yet; this creates compile pressure for exported wrappers once drawer tests use them.

- [ ] **Step 2: Run TypeScript test compilation to verify missing exports**

Run:

```bash
npm test -- src/App.test.tsx -t "agent drawer helpers"
```

Expected: FAIL if wrappers are not exported and referenced by later tests. If it still passes, proceed; Task 10 will exercise the wrappers.

- [ ] **Step 3: Add wrappers to `src/api.ts`**

Add:

```ts
export async function startAgentRun(
  sessionId: string,
  projectSlug: string,
  message: string,
): Promise<{ started: boolean }> {
  if (!isTauriRuntime()) {
    return { started: true };
  }
  return invoke<{ started: boolean }>("start_agent_run", { sessionId, projectSlug, message });
}

export async function stopAgentRun(sessionId: string): Promise<{ stopped: boolean }> {
  if (!isTauriRuntime()) {
    return { stopped: true };
  }
  return invoke<{ stopped: boolean }>("stop_agent_run", { sessionId });
}

export async function resolveAgentPermission(
  sessionId: string,
  requestId: string,
  value: string,
  supplementalInfo = "",
): Promise<{ resolved: boolean }> {
  if (!isTauriRuntime()) {
    return { resolved: true };
  }
  return invoke<{ resolved: boolean }>("resolve_agent_permission", {
    sessionId,
    requestId,
    value,
    supplementalInfo,
  });
}

export async function resolveAgentAskUser(
  sessionId: string,
  requestId: string,
  answers: Record<string, unknown>,
): Promise<{ resolved: boolean }> {
  if (!isTauriRuntime()) {
    return { resolved: true };
  }
  return invoke<{ resolved: boolean }>("resolve_agent_ask_user", {
    sessionId,
    requestId,
    answers,
  });
}
```

- [ ] **Step 4: Run frontend tests**

Run:

```bash
npm test -- src/App.test.tsx -t "agent drawer helpers"
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/api.ts src/App.test.tsx
git commit -m "feat: add assistant api wrappers"
```

## Task 10: Build The Agent Drawer Component

**Files:**
- Create: `src/AgentDrawer.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/styles.css`

- [ ] **Step 1: Write drawer integration tests first**

Add tests in `src/App.test.tsx`:

```tsx
it("opens assistant drawer from the sidebar and lists reviewed projects", async () => {
  vi.spyOn(api, "getAppState").mockResolvedValue({
    ...state,
    projects: [
      { ...state.projects[0], reviewStatus: "reviewed" },
      { ...state.projects[0], slug: "Draft", displayTitle: "Draft", reviewStatus: "not_reviewed" },
    ],
  });

  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: /^assistant$/i }));
  expect(screen.getByRole("complementary", { name: /agent assistant/i })).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: /assistant settings/i }));
  expect(screen.getByRole("tab", { name: /task assistant/i })).toBeInTheDocument();
  expect(screen.getByRole("combobox", { name: /project/i })).toHaveTextContent("KittyNest");
  expect(screen.getByRole("combobox", { name: /project/i })).not.toHaveTextContent("Draft");
});

it("sends and stops assistant runs", async () => {
  vi.spyOn(api, "getAppState").mockResolvedValue({
    ...state,
    projects: [{ ...state.projects[0], reviewStatus: "reviewed" }],
  });
  const start = vi.spyOn(api as ApiWithReviewQueue, "startAgentRun").mockResolvedValue({ started: true });
  const stop = vi.spyOn(api as ApiWithReviewQueue, "stopAgentRun").mockResolvedValue({ stopped: true });

  render(<App />);

  await userEvent.click(await screen.findByRole("button", { name: /^assistant$/i }));
  await userEvent.type(screen.getByLabelText(/message task assistant/i), "Explain the project");
  await userEvent.click(screen.getByRole("button", { name: /send/i }));

  await waitFor(() => expect(start).toHaveBeenCalledWith(expect.any(String), "KittyNest", "Explain the project"));
  await userEvent.click(screen.getByRole("button", { name: /stop/i }));
  await waitFor(() => expect(stop).toHaveBeenCalledWith(expect.any(String)));
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
npm test -- src/App.test.tsx -t "assistant drawer"
```

Expected: FAIL because the drawer does not exist.

- [ ] **Step 3: Create `src/AgentDrawer.tsx`**

Implement the component with these props:

```tsx
export function AgentDrawer({
  open,
  projects,
  onClose,
}: {
  open: boolean;
  projects: ProjectRecord[];
  onClose: () => void;
}) {
  // local state: width, messages, inputText, pending, selectedProjectSlug,
  // settings modal open, context snapshot, task-list items, collapse flags.
}
```

Required implementation details:

- Use `listen<AgentEvent>("agent://event", handler)` inside `useEffect` only when `open`.
- Filter events by a stable `sessionId` created from `window.sessionStorage`.
- Use `startAgentRun`, `stopAgentRun`, `resolveAgentPermission`, and `resolveAgentAskUser` from `src/api.ts`.
- Use `ReactMarkdown` and `remarkGfm` for assistant Markdown.
- Use lucide icons: `Settings`, `Send`, `CircleStop`, `X`, `ChevronDown`, `ChevronUp`, `Wrench`, `BrainCircuit`.
- Use native buttons and `aria-label`s matching the tests: `Assistant settings`, `Send`, `Stop`, `Message Task Assistant`.
- When no reviewed project exists, disable Send and show an inline empty line: `Review a project before using Task Assistant.`

Do not add saved sessions, model editing, or extra tool controls.

- [ ] **Step 4: Wire it into `src/App.tsx`**

Add imports:

```tsx
import { Bot } from "lucide-react";
import { AgentDrawer } from "./AgentDrawer";
```

Add state:

```tsx
const [agentDrawerOpen, setAgentDrawerOpen] = useState(false);
```

Replace the `.ledger` sidebar block with:

```tsx
<button className="assistant-launch" aria-label="Assistant" onClick={() => setAgentDrawerOpen(true)}>
  <Bot size={20} />
  <span>Assistant</span>
</button>
```

Render before the footer:

```tsx
<AgentDrawer
  open={agentDrawerOpen}
  projects={state.projects}
  onClose={() => setAgentDrawerOpen(false)}
/>
```

- [ ] **Step 5: Add minimal styles**

In `src/styles.css`, add:

```css
.assistant-launch {
  align-items: center;
  background: rgba(0, 216, 255, 0.09);
  border: 1px solid rgba(0, 216, 255, 0.4);
  border-radius: 8px;
  color: #bffaff;
  display: flex;
  gap: 11px;
  height: 45px;
  margin-top: auto;
  padding: 0 14px;
  text-align: left;
}

.assistant-launch:hover {
  background: rgba(0, 216, 255, 0.18);
  border-color: rgba(127, 255, 94, 0.48);
  color: #bcffd0;
}
```

Add drawer styles with classes used by the component: `.agent-drawer`, `.agent-resize-handle`, `.agent-header`, `.agent-messages`, `.agent-message`, `.agent-message-assistant`, `.agent-message-user`, `.agent-fold-card`, `.agent-tool-card`, `.agent-permission-card`, `.agent-task-list-card`, `.agent-composer`, `.agent-round-button`, `.agent-context-ring`, `.agent-context-tooltip`, `.agent-modal-backdrop`, `.agent-modal`, `.agent-tabs`.

- [ ] **Step 6: Run drawer tests**

Run:

```bash
npm test -- src/App.test.tsx -t "assistant drawer"
```

Expected: PASS.

- [ ] **Step 7: Commit**

Run:

```bash
git add src/AgentDrawer.tsx src/App.tsx src/App.test.tsx src/styles.css
git commit -m "feat: add assistant drawer shell"
```

## Task 11: Render Stream Events In The Drawer

**Files:**
- Modify: `src/AgentDrawer.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/agentTypes.ts`
- Modify: `src/styles.css`

- [ ] **Step 1: Add event rendering tests**

Mock Tauri event listening in `src/App.test.tsx`:

```tsx
vi.mock("@tauri-apps/api/event", () => {
  let listener: ((event: { payload: unknown }) => void) | null = null;
  return {
    listen: vi.fn((_eventName: string, callback: (event: { payload: unknown }) => void) => {
      listener = callback;
      return Promise.resolve(() => {
        listener = null;
      });
    }),
    __emitAgentEvent: (payload: unknown) => listener?.({ payload }),
  };
});
```

Add test:

```tsx
it("renders assistant thinking tool and task-list stream events", async () => {
  vi.spyOn(api, "getAppState").mockResolvedValue({
    ...state,
    projects: [{ ...state.projects[0], reviewStatus: "reviewed" }],
  });
  const eventApi = await import("@tauri-apps/api/event") as typeof import("@tauri-apps/api/event") & {
    __emitAgentEvent: (payload: unknown) => void;
  };

  render(<App />);
  await userEvent.click(await screen.findByRole("button", { name: /^assistant$/i }));

  const sessionId = window.sessionStorage.getItem("kittynest:agent-session") ?? "";
  act(() => {
    eventApi.__emitAgentEvent({ sessionId, type: "thinking_status", status: "running" });
    eventApi.__emitAgentEvent({ sessionId, type: "thinking_delta", delta: "hidden reasoning" });
    eventApi.__emitAgentEvent({ sessionId, type: "tool_start", toolCallId: "call_1", name: "read_file", arguments: { file_path: "src/App.tsx" }, summary: "read_file src/App.tsx" });
    eventApi.__emitAgentEvent({ sessionId, type: "tool_output", toolCallId: "call_1", delta: "1\timport React" });
    eventApi.__emitAgentEvent({ sessionId, type: "tool_end", toolCallId: "call_1", status: "done", resultPreview: "1\timport React" });
    eventApi.__emitAgentEvent({ sessionId, type: "todo_update", todos: [{ content: "Ship drawer", activeForm: "Shipping drawer", status: "in_progress" }] });
    eventApi.__emitAgentEvent({ sessionId, type: "token", delta: "**Hello**" });
    eventApi.__emitAgentEvent({ sessionId, type: "done", reply: "**Hello**", context: { usedTokens: 100, maxTokens: 1000, thinkingTokens: 10, breakdown: { system: 20, user: 20, assistant: 40, tool: 20 } } });
  });

  expect(screen.getByText(/thinking/i)).toBeInTheDocument();
  expect(screen.getByText(/read_file/i)).toBeInTheDocument();
  expect(screen.getByText(/ship drawer/i)).toBeInTheDocument();
  expect(screen.getByText("Hello")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run event rendering test to verify it fails**

Run:

```bash
npm test -- src/App.test.tsx -t "renders assistant thinking tool and task-list stream events"
```

Expected: FAIL because event handling is incomplete.

- [ ] **Step 3: Implement event reducer behavior in `AgentDrawer.tsx`**

Add local functions matching KittyCopilot behavior:

- `startAssistantMessage()`
- `finalizeActiveAssistantMessage(finalContent?: string)`
- `startThinkingMessage()`
- `finalizeActiveThinkingMessage()`
- `updateToolMessage(toolCallId, updater)`
- `handleAgentEvent(event: AgentEvent)`

Rules:

- `token` appends to one active assistant message.
- `thinking_status` creates/finishes one thinking card.
- `thinking_delta` appends to the active thinking card.
- `tool_start` creates a card through `messageFromAgentEvent`.
- `tool_output` appends `output` and updates `resultPreview`.
- `tool_end` sets status and preview.
- `todo_update` normalizes and displays the task-list card.
- `done` finalizes assistant text, context, and pending state.
- `cancelled` finalizes cards and sets pending false.
- `error` adds an error message.

- [ ] **Step 4: Run event rendering test**

Run:

```bash
npm test -- src/App.test.tsx -t "renders assistant thinking tool and task-list stream events"
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/AgentDrawer.tsx src/App.test.tsx src/agentTypes.ts src/styles.css
git commit -m "feat: render assistant stream events"
```

## Task 12: Add Permission, Ask-User, Task-List Collapse, And Context Tooltip UI

**Files:**
- Modify: `src/AgentDrawer.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/styles.css`

- [ ] **Step 1: Write interaction tests**

Add tests:

```tsx
it("resolves permission cards and removes them after selection", async () => {
  vi.spyOn(api, "getAppState").mockResolvedValue({
    ...state,
    projects: [{ ...state.projects[0], reviewStatus: "reviewed" }],
  });
  const resolve = vi.spyOn(api as ApiWithReviewQueue, "resolveAgentPermission").mockResolvedValue({ resolved: true });
  const eventApi = await import("@tauri-apps/api/event") as typeof import("@tauri-apps/api/event") & {
    __emitAgentEvent: (payload: unknown) => void;
  };

  render(<App />);
  await userEvent.click(await screen.findByRole("button", { name: /^assistant$/i }));
  const sessionId = window.sessionStorage.getItem("kittynest:agent-session") ?? "";

  act(() => {
    eventApi.__emitAgentEvent({
      sessionId,
      type: "permission_request",
      requestId: "permission-1",
      title: "File Permission",
      description: "Read outside project?",
      options: [{ label: "Allow", value: "allow" }, { label: "Deny", value: "deny" }],
    });
  });

  await userEvent.click(screen.getByRole("button", { name: /^allow$/i }));

  await waitFor(() => expect(resolve).toHaveBeenCalledWith(sessionId, "permission-1", "allow", ""));
  expect(screen.queryByText("Read outside project?")).not.toBeInTheDocument();
});

it("shows context shares in the context tooltip", async () => {
  vi.spyOn(api, "getAppState").mockResolvedValue({
    ...state,
    projects: [{ ...state.projects[0], reviewStatus: "reviewed" }],
  });
  const eventApi = await import("@tauri-apps/api/event") as typeof import("@tauri-apps/api/event") & {
    __emitAgentEvent: (payload: unknown) => void;
  };

  render(<App />);
  await userEvent.click(await screen.findByRole("button", { name: /^assistant$/i }));
  const sessionId = window.sessionStorage.getItem("kittynest:agent-session") ?? "";
  act(() => {
    eventApi.__emitAgentEvent({
      sessionId,
      type: "done",
      reply: "ok",
      context: { usedTokens: 1000, maxTokens: 4000, thinkingTokens: 125, breakdown: { system: 250, user: 250, assistant: 300, tool: 75 } },
    });
  });

  await userEvent.hover(screen.getByLabelText(/context usage/i));

  expect(screen.getByText(/system: 25.0%/i)).toBeInTheDocument();
  expect(screen.getByText(/thinking: 12.5%/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
npm test -- src/App.test.tsx -t "permission cards|context shares"
```

Expected: FAIL until interaction UI is complete.

- [ ] **Step 3: Implement interactions**

In `AgentDrawer.tsx`:

- For permission cards, render one button per option. On click, call `resolveAgentPermission(sessionId, requestId, value, supplementalInfo)` and remove the card.
- For ask-user cards, render options, option input text fields, freeform input, Submit, and Skip. On submit, call `resolveAgentAskUser`.
- For the task-list card, hide it when the normalized item array is empty. Expand when items first appear. Collapse/expand with a centered top handle.
- For the context ring, render SVG `circle` elements with a gray track and progress stroke. Render tooltip content on hover/focus.

- [ ] **Step 4: Run interaction tests**

Run:

```bash
npm test -- src/App.test.tsx -t "permission cards|context shares"
```

Expected: PASS.

- [ ] **Step 5: Commit**

Run:

```bash
git add src/AgentDrawer.tsx src/App.test.tsx src/styles.css
git commit -m "feat: complete assistant drawer interactions"
```

## Task 13: Final Integration And Verification

**Files:**
- All changed files from prior tasks.

- [ ] **Step 1: Run full frontend tests**

Run:

```bash
npm test
```

Expected: PASS.

- [ ] **Step 2: Run full Rust tests**

Run:

```bash
cd src-tauri
cargo test
```

Expected: PASS.

- [ ] **Step 3: Run a production build check**

Run:

```bash
npm run build
```

Expected: PASS with TypeScript and Vite build successful.

- [ ] **Step 4: Fix any failures using the smallest scoped changes**

If a failure appears, write or adjust the smallest relevant test first, then change only the code needed for that failure. Do not refactor unrelated dashboard, memory, settings, or scanner code.

- [ ] **Step 5: Commit final polish if any fixes were needed**

Run only if Step 4 changed files:

```bash
git add src src-tauri
git commit -m "fix: stabilize assistant drawer integration"
```

## Self-Review Notes

- Spec coverage: Tasks page removal is Task 1; Assistant button and drawer are Task 10; Tauri event transport is Tasks 6, 8, 10, and 11; Rust tools are Task 4; model selection is Task 8; message rendering is Tasks 10 through 12; stop/cancel is Tasks 6, 8, and 10; context ring is Tasks 2 and 12.
- Placeholder scan: The plan uses `todo_write` and task-list wording as feature names. There are no unresolved placeholder instructions.
- Type consistency: Frontend events use `sessionId`/`toolCallId`; Rust DTOs serialize with camelCase and `type`, so the event payloads match the TypeScript `AgentEvent` interface.
