# Memory Jobs and UI Design

Date: 2026-04-27
Status: Design approved by user; ready for implementation planning

## Scope

This change updates the Memory module so long-running memory work runs through Analyze Jobs, graph storage is entity-only, and the Sessions, Session Detail, and Memory pages expose memories as first-class UI data.

In scope:
- Queue Memory refresh and Memory search through the existing jobs worker.
- Remove relation output from LLM memory contracts and stop storing relation rows.
- Store and query graph data by entities and their source sessions.
- Replace the Sessions list content with a Memories project list that expands to sessions with memory.
- Add memory path, related-memory graph, and per-line memory cards to Session Detail.
- Replace the Memory page with search and entity exploration.

Out of scope:
- Project-level and system-level memory generation.
- New graph layout libraries.
- Semantic/vector search.
- CozoDB migration.

## Assumptions

- A session "has memory" when `session_memories` has at least one row for that session.
- The memory file path is deterministic: `~/.kittynest/memories/sessions/<session_slug>/memory.md`.
- Graph association is based on shared normalized entity names across sessions.
- Memory search results should be persisted in SQLite so the UI can poll and survive refreshes.
- Existing Analyze Jobs worker remains the single background execution path.

## Approach

Use the existing `jobs` table and worker for both memory refresh and memory search. The frontend only enqueues work, refreshes job state, and reads persisted results.

This keeps the behavior consistent with current scan/analyze jobs and fixes the current refresh freeze, where the frontend command waits for all memory rebuild LLM calls to finish.

## LLM Contracts

Session analysis JSON:

```json
{
  "session_title": "Short title",
  "summary": "Session summary",
  "memories": ["Short fact or user preference"],
  "entities": [{ "name": "Entity", "type": "concept" }]
}
```

Memory rebuild JSON:

```json
{
  "memories": ["Short fact or user preference"],
  "entities": [{ "name": "Entity", "type": "concept" }]
}
```

Memory search entity extraction JSON:

```json
{
  "entities": ["Entity"]
}
```

No prompt asks for `relations`. JSON parsing rejects missing `memories` or `entities` but does not require `relations`.

## Backend Design

### Jobs

Add these job kinds:
- `rebuild_memories`: rebuild all already-analyzed sessions, replacing each session's memory file, memory rows, and entity graph rows.
- `search_memories`: extract entities from a user query, find related sessions, fuzzy-match memory rows containing those entities case-insensitively, and store display results.

Dashboard Memory refresh calls `enqueue_rebuild_memories`, then the existing Analyze Jobs panel shows progress.

Memory page Send calls `enqueue_search_memories(query)`, then polls for active jobs and latest search results.

### SQLite App DB

Keep `session_memories`.

Add `memory_searches`:
- `id`
- `job_id`
- `query`
- `status`
- `message`
- `created_at`
- `updated_at`

Add `memory_search_results`:
- `id`
- `search_id`
- `source_session`
- `session_title`
- `project_slug`
- `memory`
- `ordinal`

Add read helpers:
- list sessions that have memory.
- list memory rows for a session.
- list memory rows for sessions.
- latest memory search with results.

### Graph DB

Stop creating and writing `relation`.

Keep:
- `entity(id, name, type, source_session, source_project, first_seen)`
- `entity_alias(name, canonical_id)`

Add graph helpers:
- list entities with related session count, sorted descending.
- list sessions for an entity.
- list neighboring sessions for a session by shared entity names.
- reset all graph rows.

For existing graph databases, migration can leave an old `relation` table in place, but the application no longer reads or writes it.

### Commands and API

Add/adjust Tauri commands:
- `enqueue_rebuild_memories() -> { jobId, total }`
- `enqueue_search_memories(query) -> { jobId, total }`
- `get_memory_search() -> latest search + results`
- `get_session_memory(sessionId) -> memoryPath + lines + related sessions`
- `list_memory_entities() -> entity rows`
- `list_entity_sessions(entity) -> sessions`

Broaden markdown reading only as needed so memory files under `paths.memories_dir` can be read safely. Project markdown remains constrained to `paths.projects_dir`.

## Frontend Design

### Dashboard Memory Refresh

The Refresh button enqueues `rebuild_memories`. It shows a short notice such as `Memory refresh queued: 12 sessions` and relies on the existing Analyze Jobs panel for progress.

### Sessions Page

The page title remains the app route title, but the list panel title becomes `Memories`.

The page displays project rows matching the current Projects list table:
- Name
- Path
- Status
- Source

Clicking a project expands a nested session table filtered to sessions in that project that have memory. The expanded session rows use the current Sessions list fields:
- Name
- Path
- Project
- Task
- Source
- Status
- Updated

Clicking a session opens Session Detail.

### Session Detail

The first card includes:
- Original Path
- System Path
- Memory Path

Below Summary, add a Memory card:
- Top section: a canvas-like related-memory graph. Nodes include the current session and neighboring sessions that share entities. The graph supports dragging nodes and panning the background.
- Bottom section: one sub-card per line in `memory.md`, rendered as Markdown.

The graph is implemented with React state and pointer events, not a new dependency.

### Memory Page

The page has two vertical sections.

Search section:
- Input field.
- Icon-only Send button using a lucide send icon.
- Latest results below input.
- Each result shows Source Session Name and memory content.
- Search enqueues a job and does not block the UI.

Entity section:
- Table fields: Entity, related session count, created time.
- Sort by related session count descending.
- Clicking an entity expands related sessions using the same session row fields as the Sessions page expansion.
- Clicking a session opens Session Detail.

## Error Handling

- Empty memory search input does not enqueue.
- If LLM entity extraction fails, the search job fails and stores the error message.
- If a search has no entities or no matched memories, it completes with zero results and a clear message.
- Missing memory files show an empty-state message while DB rows remain the source of truth for lists.

## Testing

Rust tests:
- Session analysis accepts JSON without `relations`.
- Memory rebuild prompt asks for `memories` and `entities` only.
- Graph write persists only entities and resets only entity data.
- Rebuild memories can be enqueued and processed by `run_next_analysis_job`.
- Memory search can be enqueued, extracts entities, stores matched memory results, and completes.
- Entity listing returns session counts sorted descending.
- Session memory detail returns memory path, memory lines, and related sessions.

React tests:
- Dashboard Memory refresh calls enqueue API and does not call direct rebuild.
- Sessions page renders Memories project rows and expands memory-bearing sessions.
- Session Detail shows Memory Path, memory cards, and related graph area.
- Memory page enqueues search and renders persisted results.
- Entity list expands sessions and opens Session Detail.

Verification:
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `npm test`
