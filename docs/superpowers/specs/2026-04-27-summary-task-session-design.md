# Summary, Task, and Session Analysis Design

## Goal

Update KittyNest analysis so Project detail has one Analyze flow, Session analysis only writes session summaries, Tasks are created manually, and markdown rendering supports tables.

## Assumptions

- When a Project Analyze run finds more than 20 sessions needing summary updates, it analyzes only the newest 20 by `updated_at` descending.
- Project progress uses all currently available analyzed session summaries for that project, ordered chronologically.
- Project summary keeps the existing review logic and output file path: `/Users/kc/.kittynest/projects/<project_name>/summary.md`.
- Project progress keeps the existing output path: `/Users/kc/.kittynest/projects/<project_name>/progress.md`.
- Manual task creation is only allowed for projects with `review_status = "reviewed"`.
- Task creation stores the user's prompt and the generated LLM prompt as markdown files under `/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>/`.

## Backend Design

### Project Analyze Flow

Replace the Project detail page's separate review/import actions with one backend job path.

The job will:

1. Select pending or failed sessions for the selected project.
2. Sort those sessions by `updated_at` descending.
3. If more than 20 need analysis, keep only the first 20.
4. Analyze those sessions with the simplified session analysis prompt.
5. Generate the project summary with the current project review logic.
6. Generate project progress from all analyzed session summaries in chronological order.

This should reuse the existing jobs table and Dashboard Analyze Jobs display. The existing `review_project` job kind can be replaced or wrapped by a clearer `analyze_project` command/job kind.

### Session Analysis

Simplify `remote_session_analysis` so the LLM returns only:

```json
{
  "session_title": "Session title",
  "summary": "Session summary"
}
```

Session processing will:

- Write `/projects/<project_slug>/sessions/<session_slug>/summary.md`.
- Update the session row title, summary, summary path, status, and processed timestamp.
- Preserve any existing `task_id` association if one already exists.
- Avoid creating tasks.
- Avoid appending task summary files.
- Avoid updating project progress.

Batch session analysis order changes from oldest-first to newest-first by `updated_at DESC`.

### Task Creation

Add a manual task creation backend command.

Inputs:

- `project_slug`
- user prompt text

Validation:

- Project must exist.
- Project must be reviewed.
- Prompt must be non-empty.

Behavior:

1. Generate a stable task slug from the prompt or LLM title.
2. Create the task directory: `/Users/kc/.kittynest/projects/<project_name>/tasks/<task_slug>/`.
3. Write `user_prompt.md`.
4. Enqueue a backend job that reads the user prompt, project summary, project progress, and project metadata.
5. The job asks the LLM to produce a project-realistic prompt and writes `llm_prompt.md`.
6. Insert the task row with `status = "discussing"` and zero sessions.

The LLM prompt-generation step runs asynchronously and appears in Dashboard Analyze Jobs.

## Frontend Design

### Project Detail

- Show one `Analyze` button.
- Remove the separate `Review Project` and `Import Historical Sessions` buttons.
- Keep showing Project Summary and Progress markdown panels.

### Tasks List

- Add manual task creation controls on the Tasks list page.
- The project selector lists only reviewed projects.
- The prompt input is required.
- Submitting creates/enqueues the task-generation job and refreshes active jobs.

### Session Detail

- Merge the first title card and the Path card.
- Use the session title as the card title.
- Card content shows:
  - Original Path: `session.rawPath`
  - System Path: `session.summaryPath` or a not-yet-generated message
- Keep the Analyze action in that card.

### Sessions List

- Add a `3 days` option to the Updated range selector.
- Keep `7 days` as the default unless tests or product feedback require changing it.

### Markdown Tables

Extend the local markdown renderer to detect simple pipe tables and render semantic `<table>`, `<thead>`, `<tbody>`, `<th>`, and `<td>` elements.

Supported table shape:

```markdown
| Column A | Column B |
| --- | --- |
| Value A | Value B |
```

The renderer should keep existing support for headings, list items, bold text, and inline code.

## Testing

Backend tests:

- Project Analyze caps project session processing at the newest 20 when more than 20 need analysis.
- Project Analyze writes both summary and progress after session updates.
- Session analysis accepts only `session_title` and `summary` JSON.
- Session processing no longer creates tasks or task summary files.
- Batch session queries return newest sessions first.
- Manual task creation rejects unreviewed projects and writes `user_prompt.md`.
- Task prompt-generation job writes `llm_prompt.md`.

Frontend tests:

- Project detail renders one Analyze button and no Review/Import buttons.
- Tasks page offers a reviewed-project selector and prompt input, then calls create task.
- Session detail merges title and path information.
- Sessions Updated selector includes `3 days`.
- Markdown table syntax renders as a table.

## Success Criteria

- A project can be analyzed from Project detail with one button.
- Session summary updates no longer create or update tasks.
- Project progress is generated only by the Project Analyze flow and uses all analyzed session summaries in chronological order.
- Users can manually create tasks only for reviewed projects.
- Task prompt files are written to the specified paths.
- Markdown tables render correctly in existing markdown panels.
