pub mod obsidian;
pub mod renderer;
pub mod state;

use crate::db;
use crate::graph;
use crate::models::{AppPaths, ObsidianConfig, SyncResult};
use renderer::*;
use std::collections::HashSet;
use std::path::Path;

/// Run a sync cycle. mode: "incremental" or "full".
pub fn run_sync(
    paths: &AppPaths,
    config: &ObsidianConfig,
    mode: &str,
) -> anyhow::Result<SyncResult> {
    let vault_path = config
        .vault_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No Obsidian vault configured"))?;

    if !obsidian::validate_vault(vault_path) {
        anyhow::bail!("Invalid Obsidian vault: {}", vault_path);
    }
    configure_obsidian_graph_colors(vault_path)?;

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
    let all_sessions = db::list_sessions(&connection)?;
    let all_tasks = db::list_tasks(&connection)?;
    let mut active_paths = HashSet::new();

    let mut active_project_slugs = HashSet::new();
    for project in &projects {
        active_project_slugs.insert(project.slug.clone());

        // Filter sessions and tasks for this project
        let project_sessions: Vec<_> = all_sessions
            .iter()
            .filter(|s| s.project_slug == project.slug && s.status == "analyzed")
            .collect();
        let session_slugs: Vec<String> = project_sessions.iter().map(|s| session_slug(s)).collect();

        let project_tasks: Vec<_> = all_tasks
            .iter()
            .filter(|t| t.project_slug == project.slug)
            .collect();
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
        active_paths.insert(rel_path.clone());

        let is_update = db::get_sync_state(&connection, "project", &project.slug)?.is_some();
        if note_needs_sync(
            &connection,
            vault_path,
            "project",
            &project.slug,
            &content,
            &rel_path,
        )? {
            write_note(vault_path, &rel_path, &content)?;
            state::record_sync(&connection, "project", &project.slug, &content, &rel_path)?;
            if is_update {
                updated += 1;
            } else {
                created += 1;
            }
        } else {
            unchanged += 1;
        }
    }

    // --- Sync sessions ---
    let mut active_session_ids = HashSet::new();
    for session in &all_sessions {
        if session.status != "analyzed" {
            continue;
        }
        let slug = session_slug(session);
        active_session_ids.insert(session.session_id.clone());

        // Get entities for this session
        let related = graph::related_sessions_for_session(paths, &session.session_id)?;
        let entity_names: Vec<String> = related
            .iter()
            .flat_map(|r| r.shared_entities.clone())
            .collect();

        let content = render_session_note(session, &session.project_slug, &entity_names);
        let rel_path = obsidian_relative_path("session", &session.project_slug, &slug);
        active_paths.insert(rel_path.clone());

        let is_update = db::get_sync_state(&connection, "session", &session.session_id)?.is_some();
        if note_needs_sync(
            &connection,
            vault_path,
            "session",
            &session.session_id,
            &content,
            &rel_path,
        )? {
            write_note(vault_path, &rel_path, &content)?;
            state::record_sync(
                &connection,
                "session",
                &session.session_id,
                &content,
                &rel_path,
            )?;
            if is_update {
                updated += 1;
            } else {
                created += 1;
            }
        } else {
            unchanged += 1;
        }
    }

    // --- Sync tasks ---
    let mut active_task_ids = HashSet::new();
    for task in &all_tasks {
        let slug = task_slug(task);
        let source_id = format!("{}:{}", task.project_slug, task.slug);
        active_task_ids.insert(source_id.clone());

        // Find sessions linked to this task
        let task_sessions: Vec<String> = all_sessions
            .iter()
            .filter(|s| s.task_slug.as_deref() == Some(&task.slug) && s.status == "analyzed")
            .map(|s| session_slug(s))
            .collect();

        let content = render_task_note(task, &task_sessions);
        let rel_path = obsidian_relative_path("task", &task.project_slug, &slug);
        active_paths.insert(rel_path.clone());

        let is_update = db::get_sync_state(&connection, "task", &source_id)?.is_some();
        if note_needs_sync(
            &connection,
            vault_path,
            "task",
            &source_id,
            &content,
            &rel_path,
        )? {
            write_note(vault_path, &rel_path, &content)?;
            state::record_sync(&connection, "task", &source_id, &content, &rel_path)?;
            if is_update {
                updated += 1;
            } else {
                created += 1;
            }
        } else {
            unchanged += 1;
        }
    }

    // --- Sync memories ---
    let mut active_memory_ids = HashSet::new();
    let mut session_ids_with_memory_notes = HashSet::new();
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

        let content = render_memory_note(&slug, &session.project_slug, &memories, &entity_names);
        let rel_path = obsidian_relative_path("memory", &session.project_slug, &memory_note_name);

        let source_id = format!("memory:{}", session.session_id);
        active_memory_ids.insert(source_id.clone());
        active_paths.insert(rel_path.clone());
        session_ids_with_memory_notes.insert(session.session_id.clone());
        let is_update = db::get_sync_state(&connection, "memory", &source_id)?.is_some();
        if note_needs_sync(
            &connection,
            vault_path,
            "memory",
            &source_id,
            &content,
            &rel_path,
        )? {
            write_note(vault_path, &rel_path, &content)?;
            state::record_sync(&connection, "memory", &source_id, &content, &rel_path)?;
            if is_update {
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
    let mut active_entity_ids = HashSet::new();
    for ec in &entity_counts {
        let entity_slug = slugify(&ec.entity);
        active_entity_ids.insert(ec.entity.clone());
        let related = graph::related_sessions_for_entity(paths, &ec.entity)?;

        // Build session slugs from actual session data
        let related_sessions: Vec<_> = related
            .iter()
            .filter_map(|r| {
                all_sessions
                    .iter()
                    .find(|s| s.session_id == r.session_id && s.status == "analyzed")
            })
            .collect();
        let session_slugs: Vec<String> = related_sessions.iter().map(|s| session_slug(s)).collect();

        let memory_slugs: Vec<String> = related_sessions
            .iter()
            .filter(|s| session_ids_with_memory_notes.contains(&s.session_id))
            .map(|s| format!("memory-{}", session_slug(s)))
            .collect();

        let content = render_entity_moc(&ec.entity, &ec.entity_type, &session_slugs, &memory_slugs);
        let rel_path = obsidian_relative_path("entity", "", &entity_slug);
        active_paths.insert(rel_path.clone());

        let is_update = db::get_sync_state(&connection, "entity", &ec.entity)?.is_some();
        if note_needs_sync(
            &connection,
            vault_path,
            "entity",
            &ec.entity,
            &content,
            &rel_path,
        )? {
            write_note(vault_path, &rel_path, &content)?;
            state::record_sync(&connection, "entity", &ec.entity, &content, &rel_path)?;
            if is_update {
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
        deleted += remove_stale_sync_states(
            &connection,
            "project",
            &active_project_slugs,
            &active_paths,
            vault_path,
        )?;
        deleted += remove_stale_sync_states(
            &connection,
            "session",
            &active_session_ids,
            &active_paths,
            vault_path,
        )?;
        deleted += remove_stale_sync_states(
            &connection,
            "task",
            &active_task_ids,
            &active_paths,
            vault_path,
        )?;
        deleted += remove_stale_sync_states(
            &connection,
            "memory",
            &active_memory_ids,
            &active_paths,
            vault_path,
        )?;
        deleted += remove_stale_sync_states(
            &connection,
            "entity",
            &active_entity_ids,
            &active_paths,
            vault_path,
        )?;
        deleted += cleanup_untracked_managed_notes(vault_path, &active_paths)?;
    }

    Ok(SyncResult {
        created,
        updated,
        deleted,
        unchanged,
    })
}

/// Ensure the directory structure exists in the vault and write a note file.
fn write_note(vault_path: &str, rel_path: &str, content: &str) -> anyhow::Result<()> {
    let full_path = Path::new(vault_path).join(rel_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&full_path, content)?;
    Ok(())
}

fn configure_obsidian_graph_colors(vault_path: &str) -> anyhow::Result<()> {
    let graph_path = Path::new(vault_path).join(".obsidian/graph.json");
    let mut graph_json = if graph_path.exists() {
        std::fs::read_to_string(&graph_path)
            .ok()
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
            .unwrap_or_else(|| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    if !graph_json.is_object() {
        graph_json = serde_json::json!({});
    }
    let graph = graph_json
        .as_object_mut()
        .expect("graph_json object checked");
    graph.insert("hideUnresolved".into(), serde_json::Value::Bool(true));
    graph.insert("showOrphans".into(), serde_json::Value::Bool(false));

    let mut existing_groups = graph
        .remove("colorGroups")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    let mut kittynest_groups = Vec::new();
    for (query, rgb) in [
        ("tag:#kittynest/project", 3_003_583u64),
        ("tag:#kittynest/session", 6_333_946u64),
        ("tag:#kittynest/task", 16_096_779u64),
        ("tag:#kittynest/memory", 10_980_346u64),
        ("tag:#kittynest/entity", 16_020_150u64),
    ] {
        if existing_groups
            .iter()
            .any(|group| group.get("query").and_then(|value| value.as_str()) == Some(query))
        {
            continue;
        }
        kittynest_groups.push(serde_json::json!({
            "query": query,
            "color": {
                "a": 1,
                "rgb": rgb
            }
        }));
    }
    kittynest_groups.append(&mut existing_groups);
    graph.insert(
        "colorGroups".into(),
        serde_json::Value::Array(kittynest_groups),
    );

    std::fs::write(&graph_path, serde_json::to_string_pretty(&graph_json)?)?;
    Ok(())
}

fn note_needs_sync(
    connection: &rusqlite::Connection,
    vault_path: &str,
    kind: &str,
    source_id: &str,
    content: &str,
    rel_path: &str,
) -> anyhow::Result<bool> {
    let file_missing = !Path::new(vault_path).join(rel_path).exists();
    let state_path_changed = db::get_sync_state(connection, kind, source_id)?
        .is_some_and(|state| state.obsidian_path != rel_path);
    Ok(state::needs_sync(connection, kind, source_id, content)?
        || file_missing
        || state_path_changed)
}

fn remove_stale_sync_states(
    connection: &rusqlite::Connection,
    kind: &str,
    active_source_ids: &HashSet<String>,
    active_paths: &HashSet<String>,
    vault_path: &str,
) -> anyhow::Result<usize> {
    let mut deleted = 0usize;
    for (source_id, rel_path) in state::synced_paths_for_kind(connection, kind)? {
        if active_source_ids.contains(&source_id) {
            continue;
        }
        if !active_paths.contains(&rel_path) && remove_vault_file_if_exists(vault_path, &rel_path)?
        {
            deleted += 1;
        }
        db::delete_sync_state(connection, kind, &source_id)?;
    }
    Ok(deleted)
}

fn cleanup_untracked_managed_notes(
    vault_path: &str,
    active_paths: &HashSet<String>,
) -> anyhow::Result<usize> {
    let root = Path::new(vault_path).join("KittyNest");
    if !root.exists() {
        return Ok(0);
    }
    let mut deleted = 0usize;
    cleanup_managed_dir(vault_path, &root, "KittyNest", active_paths, &mut deleted)?;
    Ok(deleted)
}

fn cleanup_managed_dir(
    vault_path: &str,
    dir: &Path,
    rel_prefix: &str,
    active_paths: &HashSet<String>,
    deleted: &mut usize,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let rel_path = format!("{rel_prefix}/{name}");
        if path.is_dir() {
            cleanup_managed_dir(vault_path, &path, &rel_path, active_paths, deleted)?;
            continue;
        }
        if is_managed_note_path(&rel_path)
            && !active_paths.contains(&rel_path)
            && remove_vault_file_if_exists(vault_path, &rel_path)?
        {
            *deleted += 1;
        }
    }
    Ok(())
}

fn is_managed_note_path(rel_path: &str) -> bool {
    rel_path.ends_with(".md")
        && (rel_path.starts_with("KittyNest/memories/memory-")
            || rel_path.starts_with("KittyNest/entities/")
            || (rel_path.starts_with("KittyNest/projects/")
                && (rel_path.contains("/sessions/") || rel_path.contains("/tasks/"))))
}

fn remove_vault_file_if_exists(vault_path: &str, rel_path: &str) -> anyhow::Result<bool> {
    let full_path = Path::new(vault_path).join(rel_path);
    if full_path.exists() {
        std::fs::remove_file(full_path)?;
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::run_sync;
    use crate::models::{AppPaths, ObsidianConfig, RawMessage, RawSession};

    #[test]
    fn run_sync_keeps_active_session_notes_when_delete_removed_is_enabled() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(vault.join(".obsidian")).unwrap();
        let mut connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "sync-session".into(),
                workdir: "/Users/kc/SyncProject".into(),
                created_at: "2026-04-27T00:00:00Z".into(),
                updated_at: "2026-04-27T00:05:00Z".into(),
                raw_path: "/tmp/sync-session.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Sync this session".into(),
                }],
            }],
        )
        .unwrap();
        let stored = crate::db::unprocessed_session_by_session_id(&connection, "sync-session")
            .unwrap()
            .remove(0);
        crate::db::mark_session_processed_with_optional_task_at(
            &connection,
            stored.id,
            None,
            "Synced Session",
            "Session summary.",
            "/tmp/sync-session/summary.md",
            "2026-04-27T10:00:00Z",
        )
        .unwrap();
        let config = ObsidianConfig {
            vault_path: Some(vault.to_string_lossy().to_string()),
            auto_sync: true,
            delete_removed: true,
        };

        let result = run_sync(&paths, &config, "incremental").unwrap();

        assert_eq!(result.deleted, 0);
        assert!(vault
            .join("KittyNest/projects/SyncProject/sessions/20260427-synced-session.md")
            .exists());
        assert!(
            crate::db::get_sync_state(&connection, "session", "sync-session")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn run_sync_recreates_session_note_when_current_file_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(vault.join(".obsidian")).unwrap();
        let mut connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "missing-session-note".into(),
                workdir: "/Users/kc/SyncProject".into(),
                created_at: "2026-04-27T00:00:00Z".into(),
                updated_at: "2026-04-27T00:05:00Z".into(),
                raw_path: "/tmp/missing-session-note.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Sync the missing session file".into(),
                }],
            }],
        )
        .unwrap();
        let stored =
            crate::db::unprocessed_session_by_session_id(&connection, "missing-session-note")
                .unwrap()
                .remove(0);
        crate::db::mark_session_processed_with_optional_task_at(
            &connection,
            stored.id,
            None,
            "Missing Session Current Title",
            "Session summary.",
            "/tmp/missing-session-note/summary.md",
            "2026-04-27T10:00:00Z",
        )
        .unwrap();
        let config = ObsidianConfig {
            vault_path: Some(vault.to_string_lossy().to_string()),
            auto_sync: true,
            delete_removed: true,
        };
        let expected =
            "KittyNest/projects/SyncProject/sessions/20260427-missing-session-current-title.md";

        run_sync(&paths, &config, "incremental").unwrap();
        std::fs::remove_file(vault.join(expected)).unwrap();
        connection
            .execute(
                "UPDATE sync_state SET obsidian_path = ?1 WHERE kind = 'session' AND source_id = ?2",
                rusqlite::params![
                    "KittyNest/projects/SyncProject/sessions/20260427-stale-title.md",
                    "missing-session-note"
                ],
            )
            .unwrap();

        let result = run_sync(&paths, &config, "incremental").unwrap();
        let state = crate::db::get_sync_state(&connection, "session", "missing-session-note")
            .unwrap()
            .unwrap();

        assert_eq!(result.updated, 1);
        assert!(vault.join(expected).exists());
        assert_eq!(state.obsidian_path, expected);
    }

    #[test]
    fn run_sync_removes_untracked_managed_notes_when_delete_removed_is_enabled() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(vault.join(".obsidian")).unwrap();
        std::fs::create_dir_all(vault.join("KittyNest/memories")).unwrap();
        std::fs::create_dir_all(vault.join("KittyNest/entities")).unwrap();
        std::fs::write(
            vault.join("KittyNest/memories/memory-20260502-obsidian-sync.md"),
            "stale",
        )
        .unwrap();
        std::fs::write(vault.join("KittyNest/entities/sqlite.md"), "stale").unwrap();
        std::fs::write(vault.join("20260502-obsidian-sync.md"), "").unwrap();
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        let config = ObsidianConfig {
            vault_path: Some(vault.to_string_lossy().to_string()),
            auto_sync: true,
            delete_removed: true,
        };

        let result = run_sync(&paths, &config, "incremental").unwrap();

        assert_eq!(result.deleted, 2);
        assert!(!vault
            .join("KittyNest/memories/memory-20260502-obsidian-sync.md")
            .exists());
        assert!(!vault.join("KittyNest/entities/sqlite.md").exists());
        assert!(vault.join("20260502-obsidian-sync.md").exists());
    }

    #[test]
    fn run_sync_configures_obsidian_graph_colors() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(vault.join(".obsidian")).unwrap();
        std::fs::write(
            vault.join(".obsidian/graph.json"),
            r#"{"hideUnresolved":false,"colorGroups":[]}"#,
        )
        .unwrap();
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        let config = ObsidianConfig {
            vault_path: Some(vault.to_string_lossy().to_string()),
            auto_sync: true,
            delete_removed: true,
        };

        run_sync(&paths, &config, "incremental").unwrap();
        let graph_json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(vault.join(".obsidian/graph.json")).unwrap(),
        )
        .unwrap();
        let groups = graph_json["colorGroups"].as_array().unwrap();
        let queries = groups
            .iter()
            .map(|group| group["query"].as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(graph_json["hideUnresolved"], true);
        assert_eq!(graph_json["showOrphans"], false);
        assert!(queries.contains(&"tag:#kittynest/project"));
        assert!(queries.contains(&"tag:#kittynest/session"));
        assert!(queries.contains(&"tag:#kittynest/task"));
        assert!(queries.contains(&"tag:#kittynest/memory"));
        assert!(queries.contains(&"tag:#kittynest/entity"));
    }
}
