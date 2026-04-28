# Agent Session Task Persistence Design

## Context

KittyNest already has a right-side Task Assistant drawer, streamed Tauri events, an in-memory Rust agent session registry, basic assistant tools, a Tasks list, task detail pages, and SQLite-backed task records. The previous drawer spec explicitly excluded saved assistant sessions. This extension adds saved Drawer sessions, task creation from assistant context, memory lookup from assistant tools, and refresh semantics for project/model context changes.

The user confirmed two key behaviors:

- Saved sessions persist the exact Drawer UI timeline as JSON, including Thinking and Tool blocks, while the LLM naming call uses only user and assistant message text.
- The `create_task` tool pauses the current agent run until the user accepts or cancels a page-centered modal.

## Goals

- Add Drawer `Save`, `Refresh`, and automatic refresh when Drawer Project or Assistant Model changes.
- Save a Drawer session as a task under `/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>`.
- Use the configured Assistant Model to generate Task Name and Task Description from the session's user and assistant messages.
- Show saved Drawer sessions in the Tasks list with columns `Name`, `Project`, `Status`, and `Created`.
- Let users update task status among `Discussing`, `Developing`, and `Done`.
- Add a task detail page layout that shows task metadata, markdown description, and saved conversation messages.
- Let `Load` restore the full saved Drawer timeline and backend agent context into the Drawer.
- Let `Delete` remove the saved task and its task directory.
- Add a `read_memory` assistant tool that queries memories related to an entity.
- Add a `create_task` assistant tool that proposes a task from current agent context, shows a page-centered modal, and creates the task only after user acceptance.

## Non-Goals

- Do not add shell, edit, web, or external network tools beyond the existing Assistant Model calls.
- Do not merge saved Drawer sessions into scanned Claude/Codex source sessions.
- Do not redesign the whole Tasks page or Drawer aesthetic.
- Do not analyze saved Drawer sessions through the existing historical session analysis queue.
- Do not introduce a new transport; keep using Tauri commands and `agent://event`.

## Recommended Approach

Use SQLite `tasks` as the list/status index and task directories as the durable content store.

Each saved task directory contains:

- `description.md`: the LLM-generated task description.
- `session.json`: the exact Drawer timeline and session context needed to restore the Drawer.

The `tasks` table remains the source for list rows, status, and task identity. The file store remains the source for markdown description and detailed Drawer timeline. This keeps the change local to the existing task system and avoids mixing saved assistant drafts with scanned external sessions.

## Data Model

Add `created_at TEXT NOT NULL DEFAULT ''` to `tasks`. On migration, existing tasks receive `updated_at` as their `created_at` value when `created_at` is empty.

Extend `TaskRecord` with:

- `createdAt: string`
- `descriptionPath: string | null`
- `sessionPath: string | null`

For saved Drawer tasks:

- `summary_path` points to `description.md`.
- `brief` stores a short plain-text description preview.
- `created_at` stores the task creation timestamp.
- `updated_at` changes when status or metadata changes.

`session.json` shape:

```json
{
  "version": 1,
  "sessionId": "agent-session-id",
  "projectSlug": "KittyNest",
  "projectRoot": "/absolute/project/root",
  "createdAt": "2026-04-28T00:00:00Z",
  "messages": [],
  "todos": [],
  "context": {},
  "llmMessages": []
}
```

`messages` uses the current frontend `AgentMessage` shape so Thinking, Tool, User, Assistant, permission, ask-user, and error cards can be rendered exactly. `llmMessages` is the backend OpenAI-compatible context used to continue the agent session after Load.

## Backend Commands

Add these Tauri commands:

- `clear_agent_session(session_id)` clears backend session context and pending waits.
- `save_agent_session(session_id, project_slug, timeline)` generates task metadata, creates the task, writes `description.md` and `session.json`, and returns the created `TaskRecord`.
- `load_agent_session(project_slug, task_slug)` reads `session.json`, restores backend agent context for its `sessionId`, and returns the persisted timeline.
- `delete_task(project_slug, task_slug)` changes from "only empty tasks" to deleting saved Drawer tasks and their directory. Existing analyzed tasks with linked scanned sessions remain protected unless they have a saved `session.json` task directory and no scanned sessions.
- `resolve_agent_create_task(session_id, request_id, accepted)` resolves the pending `create_task` tool modal.

`save_agent_session` and `create_task` both use `LlmScenario::Assistant` through existing model resolution. The prompt requires strict JSON with:

```json
{
  "task_name": "short action-oriented name",
  "task_description": "markdown description grounded in the conversation"
}
```

If the model response is invalid JSON or missing fields, return a clear error and do not create a partial task.

## Agent Runtime Changes

Add registry methods:

- `clear_session(session_id)`: cancels active waits, clears messages, LLM messages, todos, and cancellation state.
- `session_export(session_id)`: returns stored messages, LLM messages, todos, and context inputs for persistence.
- `session_import(session_id, saved)`: replaces current backend context with loaded messages, LLM messages, and todos.
- `request_create_task_wait(session_id, proposal)`: emits a `create_task_request` event and blocks until Accept or Cancel.

Add `create_task` to tool schemas. When called:

1. The tool asks the Assistant Model to generate a task name and description from the current agent context.
2. Backend emits `agent://event` with `type: "create_task_request"`, `requestId`, `title`, `description`, and proposed task metadata.
3. Frontend renders a page-centered modal, not a Drawer-centered card.
4. Accept calls `resolve_agent_create_task` with acceptance data; backend creates the task and returns the created task path/result to the model.
5. Cancel returns a tool result stating the user canceled task creation.

Add `read_memory` to tool schemas. Parameters:

- `entity` string, required.

Execution:

- Query related sessions via existing graph entity lookup.
- Hydrate session titles and memory lines from `session_memories`.
- Return up to 10 related sessions and up to 20 memory lines.
- If no matches exist, return `No related memory found for <entity>.`

## Frontend Changes

Drawer header actions become:

- Save
- Refresh
- Close

`Refresh` clears:

- frontend `messages`
- `todos`
- `context`
- input text
- backend session context through `clear_agent_session`

Automatic refresh triggers:

- Drawer Project select changes.
- Settings save changes `scenarioModels.assistantModel` from the previous value.

The app owns a lightweight Drawer controller so Task Detail `Load` can open the Drawer and inject the loaded timeline. This avoids trying to control Drawer state only from inside `AgentDrawer`.

Tasks list columns become:

- `Name`
- `Project`
- `Status`
- `Created`

Task detail layout:

- Top card shows task name, project, status selector, created time, and action buttons `Delete` and `Load`.
- Description card renders `description.md` with markdown and a hidden vertical scrollbar.
- Conversation card renders only User and Assistant messages from `session.json`, using the same bubble/markdown styling as Drawer with detail-page width.

The `create_task` modal:

- Renders centered in the workspace viewport.
- Shows proposed task title and markdown description in a scrollable body with hidden vertical scrollbar.
- Provides `Accept` and `Cancel`.
- Blocks only the current tool call; the rest of the app remains usable.

## Error Handling

- Save is disabled when there is no selected reviewed Project or no user/assistant message pair.
- Save shows an error message if the Assistant Model is missing or returns invalid JSON.
- Refresh while a run is active first calls `stop_agent_run`, then clears state.
- Load replaces the current Drawer session. If a run is active, Load first stops it.
- Delete asks no extra confirmation in this iteration because the existing Delete button is already an explicit destructive action; failure returns the backend error message.
- `create_task` modal Cancel never creates files or DB rows.

## Testing Plan

Frontend tests:

- Drawer renders Save, Refresh, Close in that order.
- Refresh clears messages and calls `clearAgentSession`.
- Project select changes trigger refresh.
- Settings save with changed Assistant Model triggers Drawer refresh.
- Save calls `saveAgentSession` with project slug and full message timeline.
- Tasks list headers are `Name`, `Project`, `Status`, `Created`.
- Task detail shows metadata, description markdown, conversation messages, Delete, and Load.
- Load opens Drawer and renders loaded Thinking, Tool, User, and Assistant blocks.
- `create_task_request` renders a page-centered modal and Accept/Cancel call the resolver.

Backend tests:

- `tasks` migration adds and backfills `created_at`.
- Saving an agent session writes `description.md`, `session.json`, and a `discussing` task row.
- Save rejects invalid LLM JSON without partial files.
- Loading a saved session restores backend LLM context.
- Clearing a session removes messages, todos, pending waits, and cancellation state.
- `read_memory` returns related memories for an entity and a clear empty result otherwise.
- `create_task` blocks until resolved and creates a task only when accepted.

Verification commands:

```bash
npm test
cargo test
```

## Success Criteria

- The Drawer can be refreshed manually and automatically when Project or Assistant Model context changes.
- A Drawer session can be saved as a task with LLM-generated name and description.
- The saved task appears in Tasks with the requested columns and default `Discussing` status.
- Status can be changed among `Discussing`, `Developing`, and `Done`.
- Task detail can delete or load the saved task.
- Load fully restores Drawer context and UI timeline, including Thinking and Tool blocks.
- Assistant can call `read_memory` by entity.
- Assistant can call `create_task`, pause for the modal decision, and create a task only after Accept.
- `npm test` and `cargo test` pass.
