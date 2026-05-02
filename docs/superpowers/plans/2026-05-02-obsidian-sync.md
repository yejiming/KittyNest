# Obsidian Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Incrementally sync KittyNest's Projects, Sessions, Tasks, Memories, and Entity graph to an Obsidian vault as interconnected Markdown notes with `[[wikilink]]` references.

**Architecture:** New `sync` Rust module with four sub-modules: `obsidian.rs` (vault detection), `renderer.rs` (Obsidian Markdown generation), `state.rs` (sync_state table for incremental tracking), and `mod.rs` (SyncManager orchestrator). Integrated via a new `sync_to_obsidian` background job type and three Tauri commands. Frontend adds an Obsidian Sync settings panel.

**Tech Stack:** Rust (rusqlite, sha2), TypeScript/React, Tauri 2

---

## File Map

### Create
| File | Responsibility |
|------|----------------|
| `src-tauri/src/sync/mod.rs` | SyncManager orchestrator, `run_sync()` |
| `src-tauri/src/sync/obsidian.rs` | Vault auto-detection, `ObsidianVault` struct |
| `src-tauri/src/sync/renderer.rs` | Render Obsidian Markdown with wikilinks |
| `src-tauri/src/sync/state.rs` | `sync_state` table CRUD, content hashing |

### Modify
| File | Changes |
|------|---------|
| `src-tauri/Cargo.toml` | Add `sha2` dependency |
| `src-tauri/src/data/db/schema.rs` | Add `sync_state` table DDL |
| `src-tauri/src/data/db/mod.rs` | Add `include!("sync.rs")` |
| `src-tauri/src/data/db/sync.rs` | New: sync_state DB queries (included by mod.rs) |
| `src-tauri/src/data/models.rs` | Add `ObsidianVault`, `SyncStatus`, `ObsidianConfig` structs |
| `src-tauri/src/config/settings.rs` | Read/write `[obsidian]` section in config.toml |
| `src-tauri/src/lib.rs` | Add `pub mod sync;` |
| `src-tauri/src/commands/mod.rs` | Register 3 new commands |
| `src-tauri/src/commands/jobs.rs` | Add `detect_obsidian_vaults`, `sync_to_obsidian`, `get_sync_status` commands |
| `src-tauri/src/analysis/jobs.rs` | Add `sync_to_obsidian` job handler, auto-trigger after analysis |
| `src/types.ts` | Add `ObsidianVault`, `SyncStatus` interfaces |
| `src/api.ts` | Add 3 API wrapper functions |
| `src/App.tsx` | Add Obsidian Sync panel to SettingsView |
| `src/styles.css` | Add styles for Obsidian Sync panel |

---

## Task 1: Schema & Data Layer

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/data/db/schema.rs`
- Create: `src-tauri/src/data/db/sync.rs`
- Modify: `src-tauri/src/data/db/mod.rs`
- Modify: `src-tauri/src/data/models.rs`

### Step 1: Add dependencies to Cargo.toml

Add to `[dependencies]` in `src-tauri/Cargo.toml`:

```toml
sha2 = "0.10"
```

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest/src-tauri && cargo check`
Expected: Compiles successfully (sha2 is added)

### Step 2: Add sync_state table to schema.rs

Append the `sync_state` table creation to the `execute_batch` string in `src-tauri/src/data/db/schema.rs`, before the closing `";`:

```sql
CREATE TABLE IF NOT EXISTS sync_state (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    last_synced_at TEXT NOT NULL,
    obsidian_path TEXT NOT NULL,
    UNIQUE(kind, source_id)
);
```

Then add a migration helper at the bottom of schema.rs (after the existing `add_column_if_missing` function):

```rust
pub fn drop_sync_state(connection: &rusqlite::Connection) -> anyhow::Result<()> {
    connection.execute_batch("DROP TABLE IF EXISTS sync_state;")?;
    Ok(())
}
```

### Step 3: Create sync.rs DB layer

Create `src-tauri/src/data/db/sync.rs`:

```rust
use sha2::{Digest, Sha256};

pub struct SyncStateRow {
    pub id: i64,
    pub kind: String,
    pub source_id: String,
    pub content_hash: String,
    pub last_synced_at: String,
    pub obsidian_path: String,
}

pub fn upsert_sync_state(
    connection: &rusqlite::Connection,
    kind: &str,
    source_id: &str,
    content_hash: &str,
    obsidian_path: &str,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    connection.execute(
        "INSERT INTO sync_state (kind, source_id, content_hash, last_synced_at, obsidian_path)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(kind, source_id) DO UPDATE SET
           content_hash = excluded.content_hash,
           last_synced_at = excluded.last_synced_at,
           obsidian_path = excluded.obsidian_path",
        rusqlite::params![kind, source_id, content_hash, now, obsidian_path],
    )?;
    Ok(())
}

pub fn get_sync_state(
    connection: &rusqlite::Connection,
    kind: &str,
    source_id: &str,
) -> anyhow::Result<Option<SyncStateRow>> {
    connection
        .query_row(
            "SELECT id, kind, source_id, content_hash, last_synced_at, obsidian_path
             FROM sync_state WHERE kind = ?1 AND source_id = ?2",
            rusqlite::params![kind, source_id],
            |row| {
                Ok(SyncStateRow {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    source_id: row.get(2)?,
                    content_hash: row.get(3)?,
                    last_synced_at: row.get(4)?,
                    obsidian_path: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

pub fn all_sync_states(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<SyncStateRow>> {
    let mut stmt = connection.prepare(
        "SELECT id, kind, source_id, content_hash, last_synced_at, obsidian_path FROM sync_state",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(SyncStateRow {
            id: row.get(0)?,
            kind: row.get(1)?,
            source_id: row.get(2)?,
            content_hash: row.get(3)?,
            last_synced_at: row.get(4)?,
            obsidian_path: row.get(5)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn sync_states_by_kind(
    connection: &rusqlite::Connection,
    kind: &str,
) -> anyhow::Result<Vec<SyncStateRow>> {
    let mut stmt = connection.prepare(
        "SELECT id, kind, source_id, content_hash, last_synced_at, obsidian_path
         FROM sync_state WHERE kind = ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![kind], |row| {
        Ok(SyncStateRow {
            id: row.get(0)?,
            kind: row.get(1)?,
            source_id: row.get(2)?,
            content_hash: row.get(3)?,
            last_synced_at: row.get(4)?,
            obsidian_path: row.get(5)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn delete_sync_state(
    connection: &rusqlite::Connection,
    kind: &str,
    source_id: &str,
) -> anyhow::Result<bool> {
    let count = connection.execute(
        "DELETE FROM sync_state WHERE kind = ?1 AND source_id = ?2",
        rusqlite::params![kind, source_id],
    )?;
    Ok(count > 0)
}

pub fn clear_all_sync_state(connection: &rusqlite::Connection) -> anyhow::Result<usize> {
    let count = connection.execute("DELETE FROM sync_state", [])?;
    Ok(count)
}

pub fn compute_content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}
```

### Step 4: Include sync.rs in mod.rs

Add to `src-tauri/src/data/db/mod.rs`, after the last `include!` line:

```rust
include!("sync.rs");
```

### Step 5: Add models

Add to `src-tauri/src/data/models.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsidianVault {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub vault_path: Option<String>,
    pub auto_sync: bool,
    pub delete_removed: bool,
    pub last_sync_at: Option<String>,
    pub total_synced: usize,
    pub kind_counts: SyncKindCounts,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncKindCounts {
    pub projects: usize,
    pub sessions: usize,
    pub tasks: usize,
    pub memories: usize,
    pub entities: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub unchanged: usize,
}
```

### Step 6: Verify compilation

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest/src-tauri && cargo check`
Expected: Compiles successfully

### Step 7: Commit

```bash
git add src-tauri/Cargo.toml src-tauri/src/data/db/schema.rs src-tauri/src/data/db/sync.rs src-tauri/src/data/db/mod.rs src-tauri/src/data/models.rs
git commit -m "feat(sync): add sync_state table and data layer"
```

---

## Task 2: Obsidian Config

**Files:**
- Modify: `src-tauri/src/config/settings.rs`
- Modify: `src-tauri/src/data/models.rs` (if ObsidianConfig struct needed)

### Step 1: Add ObsidianConfig struct

Add to `src-tauri/src/data/models.rs` (near the LlmSettings struct):

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsidianConfig {
    pub vault_path: Option<String>,
    pub auto_sync: bool,
    pub delete_removed: bool,
}

impl Default for ObsidianConfig {
    fn default() -> Self {
        Self {
            vault_path: None,
            auto_sync: true,
            delete_removed: true,
        }
    }
}
```

### Step 2: Add config read/write functions

Add to `src-tauri/src/config/settings.rs`:

```rust
pub fn read_obsidian_config(paths: &AppPaths) -> anyhow::Result<ObsidianConfig> {
    if !paths.config_path.exists() {
        return Ok(ObsidianConfig::default());
    }
    let content = std::fs::read_to_string(&paths.config_path)?;
    let value: toml::Value = toml::from_str(&content)?;
    if let Some(obsidian) = value.get("obsidian") {
        let vault_path = obsidian
            .get("vault_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let auto_sync = obsidian
            .get("auto_sync")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let delete_removed = obsidian
            .get("delete_removed")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        Ok(ObsidianConfig {
            vault_path,
            auto_sync,
            delete_removed,
        })
    } else {
        Ok(ObsidianConfig::default())
    }
}

pub fn write_obsidian_config(paths: &AppPaths, config: &ObsidianConfig) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.data_dir)?;

    // Read existing config or start empty
    let existing = if paths.config_path.exists() {
        std::fs::read_to_string(&paths.config_path)?
    } else {
        String::new()
    };
    let mut value: toml::Value = if existing.is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        toml::from_str(&existing)?
    };

    // Build obsidian section
    let mut obsidian_table = toml::map::Map::new();
    if let Some(ref vp) = config.vault_path {
        obsidian_table.insert("vault_path".to_string(), toml::Value::String(vp.clone()));
    }
    obsidian_table.insert(
        "auto_sync".to_string(),
        toml::Value::Boolean(config.auto_sync),
    );
    obsidian_table.insert(
        "delete_removed".to_string(),
        toml::Value::Boolean(config.delete_removed),
    );

    if let Some(table) = value.as_table_mut() {
        table.insert(
            "obsidian".to_string(),
            toml::Value::Table(obsidian_table),
        );
    }

    let serialized = toml::to_string_pretty(&value)?;
    std::fs::write(&paths.config_path, serialized)?;
    Ok(())
}
```

### Step 3: Verify compilation

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest/src-tauri && cargo check`
Expected: Compiles successfully

### Step 4: Commit

```bash
git add src-tauri/src/config/settings.rs src-tauri/src/data/models.rs
git commit -m "feat(sync): add Obsidian config read/write to settings"
```

---

## Task 3: Vault Detection

**Files:**
- Create: `src-tauri/src/sync/mod.rs`
- Create: `src-tauri/src/sync/obsidian.rs`
- Modify: `src-tauri/src/lib.rs`

### Step 1: Create sync/mod.rs

Create `src-tauri/src/sync/mod.rs`:

```rust
pub mod obsidian;
pub mod renderer;
pub mod state;
```

### Step 2: Create sync/obsidian.rs

Create `src-tauri/src/sync/obsidian.rs`:

```rust
use crate::models::ObsidianVault;
use std::path::{Path, PathBuf};

/// Scan common locations for Obsidian vaults (directories containing .obsidian/).
pub fn detect_vaults() -> Vec<ObsidianVault> {
    let mut vaults = Vec::new();
    let home = match std::env::var("HOME") {
        Ok(h) => std::path::PathBuf::from(h),
        Err(_) => return vaults,
    };

    let search_roots: Vec<PathBuf> = vec![
        home.join("Library").join("Mobile Documents"),
        home.join("Documents"),
        home.join("Desktop"),
        home.clone(),
    ];

    for root in &search_roots {
        if root.exists() {
            scan_dir_for_vaults(root, &mut vaults, 0, 3);
        }
    }

    // Deduplicate by canonical path
    vaults.sort_by(|a, b| a.path.cmp(&b.path));
    vaults.dedup_by(|a, b| a.path == b.path);

    vaults
}

fn scan_dir_for_vaults(
    dir: &Path,
    vaults: &mut Vec<ObsidianVault>,
    depth: usize,
    max_depth: usize,
) {
    if depth > max_depth {
        return;
    }

    // Check if this directory is a vault
    if dir.join(".obsidian").exists() {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Vault".to_string());
        vaults.push(ObsidianVault {
            path: dir.to_string_lossy().to_string(),
            name,
        });
        return; // Don't recurse into vaults
    }

    // Recurse into subdirectories
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden directories and common non-vault dirs
            let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
            if dir_name.starts_with('.') || dir_name == "node_modules" || dir_name == "target" {
                continue;
            }
            scan_dir_for_vaults(&path, vaults, depth + 1, max_depth);
        }
    }
}

/// Validate that a path is a valid Obsidian vault.
pub fn validate_vault(path: &str) -> bool {
    let p = Path::new(path);
    p.exists() && p.join(".obsidian").exists()
}
```

### Step 3: Add module declaration to lib.rs

Add to `src-tauri/src/lib.rs` module declarations (after `pub mod services;`):

```rust
pub mod sync;
```

### Step 4: Verify compilation

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest/src-tauri && cargo check`
Expected: May need to create placeholder files for renderer.rs and state.rs

Create `src-tauri/src/sync/renderer.rs`:
```rust
// Placeholder - implemented in Task 4
```

Create `src-tauri/src/sync/state.rs`:
```rust
// Placeholder - implemented in Task 5
```

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest/src-tauri && cargo check`
Expected: Compiles successfully

### Step 5: Commit

```bash
git add src-tauri/src/sync/ src-tauri/src/lib.rs
git commit -m "feat(sync): add Obsidian vault auto-detection"
```

---

## Task 4: Obsidian Markdown Renderer

**Files:**
- Create: `src-tauri/src/sync/renderer.rs`

### Step 1: Implement the renderer

Replace the placeholder `src-tauri/src/sync/renderer.rs`:

```rust
use crate::data::models::{ProjectRecord, SessionRecord, TaskRecord};

/// Slugify a string for use as an Obsidian-safe filename.
pub fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = true;
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

/// Render a project note with wikilinks to its sessions and tasks.
pub fn render_project_note(
    project: &ProjectRecord,
    summary_body: &str,
    session_slugs: &[String],
    task_slugs: &[String],
) -> String {
    let mut frontmatter = vec![
        ("tags", "[kittynest/project]".to_string()),
        ("workdir", project.workdir.clone()),
        ("sources", format!("[{}]", project.sources.join(", "))),
    ];
    if let Some(ref reviewed) = project.last_reviewed_at {
        frontmatter.push(("last_reviewed_at", reviewed.clone()));
    }
    if let Some(ref last_session) = project.last_session_at {
        frontmatter.push(("last_session_at", last_session.clone()));
    }

    let mut body = format!("# {}\n\n", project.display_title);
    body.push_str(summary_body.trim());
    body.push('\n');

    if !session_slugs.is_empty() {
        body.push_str("\n## Sessions\n");
        for slug in session_slugs {
            body.push_str(&format!("![[{}]]\n", slug));
        }
    }

    if !task_slugs.is_empty() {
        body.push_str("\n## Tasks\n");
        for slug in task_slugs {
            body.push_str(&format!("![[{}]]\n", slug));
        }
    }

    crate::markdown::render_frontmatter_markdown(
        &frontmatter.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        &body,
    )
}

/// Render a session note with wikilinks to its project and entities.
pub fn render_session_note(
    session: &SessionRecord,
    project_slug: &str,
    entity_names: &[String],
) -> String {
    let title = session.title.as_deref().unwrap_or("Untitled Session");
    let summary = session.summary.as_deref().unwrap_or("");

    let mut frontmatter = vec![
        ("tags", "[kittynest/session]".to_string()),
        ("source", session.source.clone()),
        ("session_id", session.session_id.clone()),
        ("project", format!("[[{}]]", project_slug)),
        ("created_at", session.created_at.clone()),
    ];
    if let Some(ref task_slug) = session.task_slug {
        frontmatter.push(("task", format!("[[{}]]", task_slug)));
    }

    let mut body = format!("# {}\n\n", title);
    body.push_str(summary.trim());
    body.push('\n');

    // Link to memory
    body.push_str(&format!(
        "\n## Memory\n![[memory-{}]]\n",
        session_slug(session)
    ));

    // Link to entities
    if !entity_names.is_empty() {
        body.push_str("\n## Related Entities\n");
        for name in entity_names {
            body.push_str(&format!("- [[{}]]\n", slugify(name)));
        }
    }

    crate::markdown::render_frontmatter_markdown(
        &frontmatter.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        &body,
    )
}

/// Render a task note with wikilinks.
pub fn render_task_note(
    task: &TaskRecord,
    session_slugs: &[String],
) -> String {
    let mut frontmatter = vec![
        ("tags", "[kittynest/task]".to_string()),
        ("status", task.status.clone()),
        ("project", format!("[[{}]]", task.project_slug)),
        ("created_at", task.created_at.clone()),
    ];

    let mut body = format!("# {}\n\n", task.title);

    // Read description or brief
    if let Some(ref desc_path) = task.description_path {
        if let Ok(content) = std::fs::read_to_string(desc_path) {
            body.push_str(content.trim());
            body.push('\n');
        }
    } else {
        body.push_str(task.brief.trim());
        body.push('\n');
    }

    if !session_slugs.is_empty() {
        body.push_str("\n## Related Sessions\n");
        for slug in session_slugs {
            body.push_str(&format!("- [[{}]]\n", slug));
        }
    }

    crate::markdown::render_frontmatter_markdown(
        &frontmatter.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        &body,
    )
}

/// Render a memory note (per-session aggregation).
pub fn render_memory_note(
    session_slug: &str,
    project_slug: &str,
    memories: &[String],
    entity_names: &[String],
) -> String {
    let frontmatter = vec![
        ("tags", "[kittynest/memory]".to_string()),
        ("session", format!("[[{}]]", session_slug)),
        ("project", format!("[[{}]]", project_slug)),
    ];

    let mut body = format!("# Memory: {}\n\n", session_slug);
    for memory in memories {
        body.push_str(&format!("- {}\n", memory));
    }

    if !entity_names.is_empty() {
        body.push_str("\n## Related Entities\n");
        for name in entity_names {
            body.push_str(&format!("- [[{}]]\n", slugify(name)));
        }
    }

    crate::markdown::render_frontmatter_markdown(
        &frontmatter.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        &body,
    )
}

/// Render an entity MOC (Map of Content) index page.
pub fn render_entity_moc(
    entity_name: &str,
    entity_type: &str,
    session_slugs: &[String],
    memory_slugs: &[String],
) -> String {
    let frontmatter = vec![
        ("tags", "[kittynest/entity]".to_string()),
        ("entity_type", entity_type.to_string()),
    ];

    let mut body = format!("# {}\n\n", entity_name);

    if !session_slugs.is_empty() {
        body.push_str("## Sessions\n");
        for slug in session_slugs {
            body.push_str(&format!("- [[{}]]\n", slug));
        }
    }

    if !memory_slugs.is_empty() {
        body.push_str("\n## Memories\n");
        for slug in memory_slugs {
            body.push_str(&format!("- [[{}]]\n", slug));
        }
    }

    crate::markdown::render_frontmatter_markdown(
        &frontmatter.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        &body,
    )
}

/// Derive a session slug from a SessionRecord for use as a note name.
pub fn session_slug(session: &SessionRecord) -> String {
    let date_prefix = session.created_at[..10].replace('-', "");
    let title_part = session
        .title
        .as_deref()
        .unwrap_or(&session.session_id[..8.min(session.session_id.len())]);
    format!("{}-{}", date_prefix, slugify(title_part))
}

/// Derive a task slug from a TaskRecord for use as a note name.
pub fn task_slug(task: &TaskRecord) -> String {
    slugify(&task.slug)
}

/// Build the Obsidian relative path for a note kind.
pub fn obsidian_relative_path(kind: &str, project_slug: &str, note_name: &str) -> String {
    match kind {
        "project" => format!("KittyNest/projects/{}/{}.md", project_slug, note_name),
        "session" => format!(
            "KittyNest/projects/{}/sessions/{}.md",
            project_slug, note_name
        ),
        "task" => format!(
            "KittyNest/projects/{}/tasks/{}.md",
            project_slug, note_name
        ),
        "memory" => format!("KittyNest/memories/{}.md", note_name),
        "entity" => format!("KittyNest/entities/{}.md", note_name),
        _ => format!("KittyNest/{}.md", note_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("Build Fix!"), "build-fix");
        assert_eq!(slugify("  spaces  "), "spaces");
        assert_eq!(slugify("camelCase"), "camelcase");
    }

    #[test]
    fn test_session_slug() {
        let session = SessionRecord {
            source: "claude".to_string(),
            session_id: "abc-123".to_string(),
            raw_path: String::new(),
            project_slug: "test".to_string(),
            task_slug: None,
            title: Some("Build Fix".to_string()),
            summary: None,
            summary_path: None,
            created_at: "2026-05-02T06:40:45Z".to_string(),
            updated_at: String::new(),
            status: "analyzed".to_string(),
        };
        assert_eq!(session_slug(&session), "20260502-build-fix");
    }

    #[test]
    fn test_obsidian_relative_path() {
        assert_eq!(
            obsidian_relative_path("session", "kitty-nest", "20260502-build-fix"),
            "KittyNest/projects/kitty-nest/sessions/20260502-build-fix.md"
        );
        assert_eq!(
            obsidian_relative_path("memory", "kitty-nest", "memory-20260502-build-fix"),
            "KittyNest/memories/memory-20260502-build-fix.md"
        );
        assert_eq!(
            obsidian_relative_path("entity", "kitty-nest", "rusqlite"),
            "KittyNest/entities/rusqlite.md"
        );
    }
}
```

### Step 2: Run tests

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest/src-tauri && cargo test sync::renderer`
Expected: All 3 tests pass

### Step 3: Commit

```bash
git add src-tauri/src/sync/renderer.rs
git commit -m "feat(sync): add Obsidian Markdown renderer with wikilinks"
```

---

## Task 5: Sync State Manager

**Files:**
- Create: `src-tauri/src/sync/state.rs`

### Step 1: Implement sync state manager

Replace the placeholder `src-tauri/src/sync/state.rs`:

```rust
use crate::db;
use crate::models::AppPaths;
use std::path::Path;

/// Check if content has changed since last sync by comparing hashes.
pub fn needs_sync(
    connection: &rusqlite::Connection,
    kind: &str,
    source_id: &str,
    content: &str,
) -> anyhow::Result<bool> {
    let new_hash = db::compute_content_hash(content);
    match db::get_sync_state(connection, kind, source_id)? {
        Some(existing) => Ok(existing.content_hash != new_hash),
        None => Ok(true), // Never synced
    }
}

/// Record a successful sync.
pub fn record_sync(
    connection: &rusqlite::Connection,
    kind: &str,
    source_id: &str,
    content: &str,
    obsidian_path: &str,
) -> anyhow::Result<()> {
    let hash = db::compute_content_hash(content);
    db::upsert_sync_state(connection, kind, source_id, &hash, obsidian_path)
}

/// Get all synced paths for a kind, used to detect deletions.
pub fn synced_paths_for_kind(
    connection: &rusqlite::Connection,
    kind: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let rows = db::sync_states_by_kind(connection, kind)?;
    Ok(rows
        .into_iter()
        .map(|r| (r.source_id, r.obsidian_path))
        .collect())
}

/// Delete a sync state entry and optionally remove the file from vault.
pub fn remove_synced_file(
    connection: &rusqlite::Connection,
    kind: &str,
    source_id: &str,
    vault_path: &str,
    delete_file: bool,
) -> anyhow::Result<()> {
    if delete_file {
        if let Some(state) = db::get_sync_state(connection, kind, source_id)? {
            let full_path = Path::new(vault_path).join(&state.obsidian_path);
            if full_path.exists() {
                let _ = std::fs::remove_file(full_path);
            }
        }
    }
    db::delete_sync_state(connection, kind, source_id)?;
    Ok(())
}

/// Clear all sync state (for full resync).
pub fn clear_all(connection: &rusqlite::Connection) -> anyhow::Result<usize> {
    db::clear_all_sync_state(connection)
}

/// Count synced entries by kind.
pub fn count_by_kind(connection: &rusqlite::Connection) -> anyhow::Result<SyncKindCounts> {
    let projects = db::sync_states_by_kind(connection, "project")?.len();
    let sessions = db::sync_states_by_kind(connection, "session")?.len();
    let tasks = db::sync_states_by_kind(connection, "task")?.len();
    let memories = db::sync_states_by_kind(connection, "memory")?.len();
    let entities = db::sync_states_by_kind(connection, "entity")?.len();
    Ok(SyncKindCounts {
        projects,
        sessions,
        tasks,
        memories,
        entities,
    })
}

use crate::models::SyncKindCounts;
```

### Step 2: Verify compilation

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest/src-tauri && cargo check`
Expected: Compiles successfully

### Step 3: Commit

```bash
git add src-tauri/src/sync/state.rs
git commit -m "feat(sync): add sync state manager with hash-based incremental tracking"
```

---

## Task 6: Sync Manager

**Files:**
- Modify: `src-tauri/src/sync/mod.rs`

### Step 1: Implement SyncManager

Replace `src-tauri/src/sync/mod.rs`:

```rust
pub mod obsidian;
pub mod renderer;
pub mod state;

use crate::db;
use crate::graph;
use crate::models::{AppPaths, ObsidianConfig, SyncResult};
use renderer::*;
use std::path::Path;

/// Run a sync cycle. mode: "incremental" or "full".
pub fn run_sync(paths: &AppPaths, config: &ObsidianConfig, mode: &str) -> anyhow::Result<SyncResult> {
    let vault_path = config
        .vault_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No Obsidian vault configured"))?;

    if !obsidian::validate_vault(vault_path) {
        anyhow::bail!("Invalid Obsidian vault: {}", vault_path);
    }

    let connection = db::open(paths)?;
    db::migrate(&connection)?;

    // For full resync, clear all state first
    if mode == "full" {
        state::clear_all(&connection)?;
    }

    let mut created = 0usize;
    let mut updated = 0usize;
    let mut deleted = 0usize;
    let mut unchanged = 0usize;

    // --- Sync projects ---
    let projects = db::list_projects(&connection)?;
    let mut active_project_slugs = Vec::new();
    for project in &projects {
        active_project_slugs.push(project.slug.clone());

        // Get sessions and tasks for this project
        let sessions = db::all_project_sessions_by_project_slug(&connection, &project.slug, 1000)?;
        let session_slugs: Vec<String> = sessions.iter().map(|s| session_slug(s)).collect();

        let tasks = db::list_tasks(&connection)?;
        let project_tasks: Vec<_> = tasks.iter().filter(|t| t.project_slug == project.slug).collect();
        let task_slugs: Vec<String> = project_tasks.iter().map(|t| task_slug(t)).collect();

        // Read project summary body
        let summary_body = project
            .info_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();

        let content = render_project_note(project, &summary_body, &session_slugs, &task_slugs);
        let note_name = slugify(&project.slug);
        let rel_path = obsidian_relative_path("project", &project.slug, &note_name);

        if state::needs_sync(&connection, "project", &project.slug, &content)? {
            write_note(vault_path, &rel_path, &content)?;
            state::record_sync(&connection, "project", &project.slug, &content, &rel_path)?;
            if db::get_sync_state(&connection, "project", &project.slug)?.is_some() {
                updated += 1;
            } else {
                created += 1;
            }
        } else {
            unchanged += 1;
        }
    }

    // --- Sync sessions ---
    let all_sessions = db::list_sessions(&connection)?;
    let mut active_session_slugs = Vec::new();
    for session in &all_sessions {
        if session.status != "analyzed" {
            continue;
        }
        let slug = session_slug(session);
        active_session_slugs.push(slug.clone());

        // Get entities for this session
        let related = graph::related_sessions_for_session(paths, &session.session_id)?;
        let entity_names: Vec<String> = related
            .iter()
            .flat_map(|r| r.shared_entities.clone())
            .collect();

        let content = render_session_note(session, &session.project_slug, &entity_names);
        let rel_path = obsidian_relative_path("session", &session.project_slug, &slug);

        if state::needs_sync(&connection, "session", &session.session_id, &content)? {
            write_note(vault_path, &rel_path, &content)?;
            state::record_sync(&connection, "session", &session.session_id, &content, &rel_path)?;
            if db::get_sync_state(&connection, "session", &session.session_id)?.is_some() {
                updated += 1;
            } else {
                created += 1;
            }
        } else {
            unchanged += 1;
        }
    }

    // --- Sync tasks ---
    let all_tasks = db::list_tasks(&connection)?;
    let mut active_task_ids = Vec::new();
    for task in &all_tasks {
        let slug = task_slug(task);
        active_task_ids.push(slug.clone());

        // Find sessions linked to this task
        let task_sessions: Vec<String> = all_sessions
            .iter()
            .filter(|s| s.task_slug.as_deref() == Some(&task.slug))
            .map(|s| session_slug(s))
            .collect();

        let content = render_task_note(task, &task_sessions);
        let rel_path = obsidian_relative_path("task", &task.project_slug, &slug);

        let source_id = format!("{}:{}", task.project_slug, task.slug);
        if state::needs_sync(&connection, "task", &source_id, &content)? {
            write_note(vault_path, &rel_path, &content)?;
            state::record_sync(&connection, "task", &source_id, &content, &rel_path)?;
            if db::get_sync_state(&connection, "task", &source_id)?.is_some() {
                updated += 1;
            } else {
                created += 1;
            }
        } else {
            unchanged += 1;
        }
    }

    // --- Sync memories ---
    for session in &all_sessions {
        if session.status != "analyzed" {
            continue;
        }
        let slug = session_slug(session);
        let memory_note_name = format!("memory-{}", slug);

        let memories = db::session_memories_by_session_id(&connection, &session.session_id)?;
        if memories.is_empty() {
            continue;
        }

        let related = graph::related_sessions_for_session(paths, &session.session_id)?;
        let entity_names: Vec<String> = related
            .iter()
            .flat_map(|r| r.shared_entities.clone())
            .collect();

        let content = render_memory_note(
            &slug,
            &session.project_slug,
            &memories,
            &entity_names,
        );
        let rel_path = obsidian_relative_path("memory", &session.project_slug, &memory_note_name);

        let source_id = format!("memory:{}", session.session_id);
        if state::needs_sync(&connection, "memory", &source_id, &content)? {
            write_note(vault_path, &rel_path, &content)?;
            state::record_sync(&connection, "memory", &source_id, &content, &rel_path)?;
            if db::get_sync_state(&connection, "memory", &source_id)?.is_some() {
                updated += 1;
            } else {
                created += 1;
            }
        } else {
            unchanged += 1;
        }
    }

    // --- Sync entity MOC pages ---
    let entity_counts = graph::entity_session_counts(paths)?;
    for ec in &entity_counts {
        let entity_slug = slugify(&ec.entity);
        let related = graph::related_sessions_for_entity(paths, &ec.entity)?;

        // Build session slugs from actual session data
        let session_slugs: Vec<String> = related
            .iter()
            .filter_map(|r| {
                all_sessions
                    .iter()
                    .find(|s| s.session_id == r.session_id)
                    .map(|s| session_slug(s))
            })
            .collect();

        let memory_slugs: Vec<String> = session_slugs
            .iter()
            .map(|s| format!("memory-{}", s))
            .collect();

        let content = render_entity_moc(
            &ec.entity,
            &ec.entity_type,
            &session_slugs,
            &memory_slugs,
        );
        let rel_path = obsidian_relative_path("entity", "", &entity_slug);

        if state::needs_sync(&connection, "entity", &ec.entity, &content)? {
            write_note(vault_path, &rel_path, &content)?;
            state::record_sync(&connection, "entity", &ec.entity, &content, &rel_path)?;
            if db::get_sync_state(&connection, "entity", &ec.entity)?.is_some() {
                updated += 1;
            } else {
                created += 1;
            }
        } else {
            unchanged += 1;
        }
    }

    // --- Handle deletions ---
    if config.delete_removed {
        // Check for deleted sessions
        let synced_sessions = state::synced_paths_for_kind(&connection, "session")?;
        for (source_id, path) in synced_sessions {
            if !active_session_slugs.contains(&source_id) {
                state::remove_synced_file(&connection, "session", &source_id, vault_path, true)?;
                deleted += 1;
            }
        }

        // Check for deleted tasks
        let synced_tasks = state::synced_paths_for_kind(&connection, "task")?;
        for (source_id, path) in synced_tasks {
            let slug = source_id.split(':').last().unwrap_or("");
            if !all_tasks.iter().any(|t| t.slug == slug) {
                state::remove_synced_file(&connection, "task", &source_id, vault_path, true)?;
                deleted += 1;
            }
        }
    }

    // Update config with last sync time
    let mut updated_config = config.clone();
    let now = chrono::Utc::now().to_rfc3339();
    crate::config::write_obsidian_config(paths, &updated_config)?;

    Ok(SyncResult {
        created,
        updated,
        deleted,
        unchanged,
    })
}

/// Ensure the directory structure exists in the vault.
fn write_note(vault_path: &str, rel_path: &str, content: &str) -> anyhow::Result<()> {
    let full_path = Path::new(vault_path).join(rel_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&full_path, content)?;
    Ok(())
}
```

### Step 2: Verify compilation

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest/src-tauri && cargo check`
Expected: Compiles successfully (may need to adjust entity_count field names based on actual graph.rs API)

### Step 3: Commit

```bash
git add src-tauri/src/sync/
git commit -m "feat(sync): implement SyncManager with incremental sync logic"
```

---

## Task 7: Job Integration

**Files:**
- Modify: `src-tauri/src/analysis/jobs.rs`
- Modify: `src-tauri/src/data/db/jobs.rs` (if enqueue helper needed)

### Step 1: Add enqueue helper for sync job

Add to `src-tauri/src/data/db/jobs.rs` (at the end, following the existing enqueue pattern):

```rust
pub fn enqueue_sync_to_obsidian(connection: &rusqlite::Connection, mode: &str) -> anyhow::Result<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    connection.execute(
        "INSERT INTO jobs (kind, scope, status, message, started_at, updated_at)
         VALUES ('sync_to_obsidian', ?1, 'queued', '', ?2, ?2)",
        rusqlite::params![mode, now],
    )?;
    Ok(connection.last_insert_rowid())
}
```

### Step 2: Add sync_to_obsidian job handler

Add to `src-tauri/src/analysis/jobs.rs` in the `run_next_analysis_job` match block (before the default arm):

```rust
"sync_to_obsidian" => {
    let config = crate::config::read_obsidian_config(paths)?;
    let mode = job.scope.as_str(); // "incremental" or "full"
    match crate::sync::run_sync(paths, &config, mode) {
        Ok(result) => {
            let msg = format!(
                "Synced: {} created, {} updated, {} deleted, {} unchanged",
                result.created, result.updated, result.deleted, result.unchanged
            );
            crate::db::update_job_progress(&connection, job.id, 1, 1, 0, &msg)?;
            crate::db::complete_job(&connection, job.id)?;
        }
        Err(e) => {
            crate::db::fail_job(&connection, job.id, &e.to_string())?;
        }
    }
    return Ok(true);
}
```

### Step 3: Add auto-trigger after session analysis

In the `process_session_job` function in `analysis/jobs.rs`, after a session is successfully processed (around where `mark_session_processed` is called), add:

```rust
// Auto-trigger Obsidian sync if configured
if let Ok(config) = crate::config::read_obsidian_config(paths) {
    if config.auto_sync && config.vault_path.is_some() {
        let _ = crate::db::enqueue_sync_to_obsidian(&connection, "incremental");
    }
}
```

Similarly, in the `analyze_project` job handler, after the project analysis completes successfully, add the same auto-trigger block.

### Step 4: Verify compilation

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest/src-tauri && cargo check`
Expected: Compiles successfully

### Step 5: Commit

```bash
git add src-tauri/src/analysis/jobs.rs src-tauri/src/data/db/jobs.rs
git commit -m "feat(sync): integrate sync_to_obsidian job with auto-trigger"
```

---

## Task 8: Tauri Commands

**Files:**
- Modify: `src-tauri/src/commands/jobs.rs`
- Modify: `src-tauri/src/commands/mod.rs`

### Step 1: Add commands to jobs.rs

Add to the end of `src-tauri/src/commands/jobs.rs` (following the existing command pattern):

```rust
#[tauri::command]
pub fn detect_obsidian_vaults() -> CommandResult<serde_json::Value> {
    let vaults = crate::sync::obsidian::detect_vaults();
    Ok(serde_json::json!({ "vaults": vaults }))
}

#[tauri::command]
pub fn sync_to_obsidian(
    mode: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let config = crate::config::read_obsidian_config(&services.paths)
        .map_err(to_command_error)?;

    let result = crate::sync::run_sync(&services.paths, &config, &mode)
        .map_err(to_command_error)?;

    Ok(serde_json::json!({ "result": result }))
}

#[tauri::command]
pub fn get_sync_status(
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let config = crate::config::read_obsidian_config(&services.paths)
        .map_err(to_command_error)?;

    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    let kind_counts = crate::sync::state::count_by_kind(&connection)
        .map_err(to_command_error)?;

    let status = crate::models::SyncStatus {
        vault_path: config.vault_path,
        auto_sync: config.auto_sync,
        delete_removed: config.delete_removed,
        last_sync_at: None, // Could track this in config
        total_synced: kind_counts.projects
            + kind_counts.sessions
            + kind_counts.tasks
            + kind_counts.memories
            + kind_counts.entities,
        kind_counts,
    };

    Ok(serde_json::json!({ "status": status }))
}

#[tauri::command]
pub fn enqueue_sync_to_obsidian_cmd(
    mode: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    let job_id = crate::db::enqueue_sync_to_obsidian(&connection, &mode)
        .map_err(to_command_error)?;
    Ok(serde_json::json!({ "jobId": job_id }))
}

#[tauri::command]
pub fn save_obsidian_config(
    vault_path: Option<String>,
    auto_sync: bool,
    delete_removed: bool,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let config = crate::models::ObsidianConfig {
        vault_path,
        auto_sync,
        delete_removed,
    };
    crate::config::write_obsidian_config(&services.paths, &config)
        .map_err(to_command_error)?;
    Ok(serde_json::json!({ "saved": true }))
}
```

### Step 2: Register commands in mod.rs

Add the new command names to the `tauri::generate_handler![]` list in `src-tauri/src/commands/mod.rs`:

```rust
detect_obsidian_vaults,
sync_to_obsidian,
get_sync_status,
enqueue_sync_to_obsidian_cmd,
save_obsidian_config,
```

### Step 3: Verify compilation

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest/src-tauri && cargo check`
Expected: Compiles successfully

### Step 4: Commit

```bash
git add src-tauri/src/commands/jobs.rs src-tauri/src/commands/mod.rs
git commit -m "feat(sync): add Tauri commands for Obsidian sync"
```

---

## Task 9: Frontend Types & API

**Files:**
- Modify: `src/types.ts`
- Modify: `src/api.ts`

### Step 1: Add TypeScript interfaces

Add to `src/types.ts`:

```ts
export interface ObsidianVault {
  path: string;
  name: string;
}

export interface SyncStatus {
  vaultPath: string | null;
  autoSync: boolean;
  deleteRemoved: boolean;
  lastSyncAt: string | null;
  totalSynced: number;
  kindCounts: SyncKindCounts;
}

export interface SyncKindCounts {
  projects: number;
  sessions: number;
  tasks: number;
  memories: number;
  entities: number;
}

export interface SyncResult {
  created: number;
  updated: number;
  deleted: number;
  unchanged: number;
}
```

### Step 2: Add API wrapper functions

Add to `src/api.ts` (following the existing pattern):

```ts
export async function detectObsidianVaults(): Promise<{ vaults: ObsidianVault[] }> {
  if (!isTauriRuntime()) {
    return { vaults: [] };
  }
  return invoke<{ vaults: ObsidianVault[] }>("detect_obsidian_vaults");
}

export async function syncToObsidian(mode: string): Promise<{ result: SyncResult }> {
  if (!isTauriRuntime()) {
    return { result: { created: 0, updated: 0, deleted: 0, unchanged: 0 } };
  }
  return invoke<{ result: SyncResult }>("sync_to_obsidian", { mode });
}

export async function getSyncStatus(): Promise<{ status: SyncStatus }> {
  if (!isTauriRuntime()) {
    return {
      status: {
        vaultPath: null,
        autoSync: true,
        deleteRemoved: true,
        lastSyncAt: null,
        totalSynced: 0,
        kindCounts: { projects: 0, sessions: 0, tasks: 0, memories: 0, entities: 0 },
      },
    };
  }
  return invoke<{ status: SyncStatus }>("get_sync_status");
}

export async function enqueueSyncToObsidian(mode: string): Promise<{ jobId: number }> {
  if (!isTauriRuntime()) {
    return { jobId: 0 };
  }
  return invoke<{ jobId: number }>("enqueue_sync_to_obsidian_cmd", { mode });
}

export async function saveObsidianConfig(
  vaultPath: string | null,
  autoSync: boolean,
  deleteRemoved: boolean,
): Promise<{ saved: boolean }> {
  if (!isTauriRuntime()) {
    return { saved: true };
  }
  return invoke<{ saved: boolean }>("save_obsidian_config", {
    vaultPath,
    autoSync,
    deleteRemoved,
  });
}
```

### Step 3: Import new types in api.ts

Add to the import block at the top of `src/api.ts`:

```ts
import type { ..., ObsidianVault, SyncStatus, SyncResult } from "./types";
```

### Step 4: Verify frontend builds

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest && npx tsc --noEmit`
Expected: No type errors

### Step 5: Commit

```bash
git add src/types.ts src/api.ts
git commit -m "feat(sync): add frontend types and API wrappers for Obsidian sync"
```

---

## Task 10: Settings UI

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

### Step 1: Add ObsidianSyncPanel component

Add a new component in `src/App.tsx` (after the `SettingsView` component, around line 1892):

```tsx
function ObsidianSyncPanel({
  busy,
  onSync,
  onSaveConfig,
}: {
  busy: string | null;
  onSync: (mode: string) => void;
  onSaveConfig: (vaultPath: string | null, autoSync: boolean, deleteRemoved: boolean) => void;
}) {
  const [vaults, setVaults] = React.useState<{ path: string; name: string }[]>([]);
  const [selectedVault, setSelectedVault] = React.useState<string>("");
  const [autoSync, setAutoSync] = React.useState(true);
  const [deleteRemoved, setDeleteRemoved] = React.useState(true);
  const [syncStatus, setSyncStatus] = React.useState<any>(null);
  const [detecting, setDetecting] = React.useState(false);

  React.useEffect(() => {
    loadStatus();
  }, []);

  async function loadStatus() {
    try {
      const { status } = await getSyncStatus();
      setSyncStatus(status);
      if (status.vaultPath) setSelectedVault(status.vaultPath);
      setAutoSync(status.autoSync);
      setDeleteRemoved(status.deleteRemoved);
    } catch {}
  }

  async function handleDetect() {
    setDetecting(true);
    try {
      const { vaults: found } = await detectObsidianVaults();
      setVaults(found);
      if (found.length === 1) {
        setSelectedVault(found[0].path);
      }
    } finally {
      setDetecting(false);
    }
  }

  function handleSave() {
    onSaveConfig(selectedVault || null, autoSync, deleteRemoved);
  }

  return (
    <div className="panel obsidian-sync-panel">
      <h3>Obsidian Sync</h3>

      <label className="settings-form-row">
        <span>Vault</span>
        <div className="vault-selector">
          <select
            value={selectedVault}
            onChange={(e) => setSelectedVault(e.target.value)}
          >
            <option value="">Select a vault...</option>
            {vaults.map((v) => (
              <option key={v.path} value={v.path}>
                {v.name} ({v.path})
              </option>
            ))}
            {selectedVault && !vaults.find((v) => v.path === selectedVault) && (
              <option value={selectedVault}>{selectedVault}</option>
            )}
          </select>
          <IconButton
            label={detecting ? "Detecting..." : "Detect"}
            icon={<Search size={14} />}
            onClick={handleDetect}
            busy={detecting}
          />
        </div>
      </label>

      <label className="settings-form-row">
        <span>Auto-sync after analysis</span>
        <input
          type="checkbox"
          checked={autoSync}
          onChange={(e) => setAutoSync(e.target.checked)}
        />
      </label>

      <label className="settings-form-row">
        <span>Delete removed notes</span>
        <input
          type="checkbox"
          checked={deleteRemoved}
          onChange={(e) => setDeleteRemoved(e.target.checked)}
        />
      </label>

      <div className="obsidian-sync-actions">
        <IconButton
          label="Save Config"
          icon={<Save size={14} />}
          onClick={handleSave}
          busy={busy === "Save Obsidian Config"}
        />
        <IconButton
          label="Sync Now"
          icon={<RefreshCw size={14} />}
          onClick={() => onSync("incremental")}
          busy={busy === "Sync Obsidian"}
        />
        <IconButton
          label="Full Resync"
          icon={<RefreshCw size={14} />}
          onClick={() => onSync("full")}
          busy={busy === "Sync Obsidian Full"}
        />
      </div>

      {syncStatus && (
        <div className="obsidian-sync-status">
          <span>
            Synced: {syncStatus.totalSynced} notes
            ({syncStatus.kindCounts.projects} projects, {syncStatus.kindCounts.sessions} sessions,
            {syncStatus.kindCounts.tasks} tasks, {syncStatus.kindCounts.memories} memories,
            {syncStatus.kindCounts.entities} entities)
          </span>
        </div>
      )}
    </div>
  );
}
```

### Step 2: Add ObsidianSyncPanel to SettingsView

Modify the `SettingsView` component to include the new panel. Add new props:

```ts
onSyncObsidian: (mode: string) => void;
onSaveObsidianConfig: (vaultPath: string | null, autoSync: boolean, deleteRemoved: boolean) => void;
```

Add inside the `settings-grid` section (after the reset panel):

```tsx
<ObsidianSyncPanel
  busy={busy}
  onSync={onSyncObsidian}
  onSaveConfig={onSaveObsidianConfig}
/>
```

### Step 3: Wire up handlers in App component

In the `App` component where `<SettingsView>` is rendered (around line 429), add:

```ts
onSyncObsidian={(mode) => {
  const label = mode === "full" ? "Sync Obsidian Full" : "Sync Obsidian";
  runAction(label, async () => {
    const { result } = await syncToObsidian(mode);
    setNotice(`Sync complete: ${result.created} created, ${result.updated} updated, ${result.deleted} deleted`);
  });
}}
onSaveObsidianConfig={(vaultPath, autoSync, deleteRemoved) => {
  runAction("Save Obsidian Config", async () => {
    await saveObsidianConfig(vaultPath, autoSync, deleteRemoved);
    setNotice("Obsidian config saved");
  });
}}
```

### Step 4: Add CSS styles

Add to `src/styles.css`:

```css
.obsidian-sync-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.obsidian-sync-panel h3 {
  margin: 0 0 8px;
}

.vault-selector {
  display: flex;
  gap: 8px;
  align-items: center;
  width: 100%;
}

.vault-selector select {
  flex: 1;
}

.obsidian-sync-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.obsidian-sync-status {
  font-size: 12px;
  color: var(--text-secondary);
  padding: 8px;
  background: var(--bg-secondary);
  border-radius: 6px;
}
```

### Step 5: Verify frontend builds

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest && npx tsc --noEmit`
Expected: No type errors

### Step 6: Commit

```bash
git add src/App.tsx src/styles.css
git commit -m "feat(sync): add Obsidian Sync settings panel to UI"
```

---

## Task 11: Full Build & Integration Test

### Step 1: Build the Tauri app

Run: `cd /Users/yejiming/Desktop/kittlabs/KittyNest && npm run tauri build 2>&1 | tail -20`
Expected: Build succeeds

### Step 2: Manual integration test

1. Launch the app
2. Go to Settings → Obsidian Sync panel
3. Click "Detect" to find vaults
4. Select a vault
5. Click "Save Config"
6. Click "Sync Now"
7. Verify files are created in `<vault>/KittyNest/` with correct structure
8. Open the vault in Obsidian, check Graph View for wikilink connections

### Step 3: Final commit (if any fixes needed)

```bash
git add -A
git commit -m "fix(sync): address integration test findings"
```
