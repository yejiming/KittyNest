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
    let all_sessions = db::list_sessions(&connection)?;
    let all_tasks = db::list_tasks(&connection)?;

    let mut active_project_slugs = Vec::new();
    for project in &projects {
        active_project_slugs.push(project.slug.clone());

        // Filter sessions and tasks for this project
        let project_sessions: Vec<_> = all_sessions
            .iter()
            .filter(|s| s.project_slug == project.slug)
            .collect();
        let session_slugs: Vec<String> = project_sessions.iter().map(|s| session_slug(s)).collect();

        let project_tasks: Vec<_> = all_tasks.iter().filter(|t| t.project_slug == project.slug).collect();
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

        let is_update = db::get_sync_state(&connection, "project", &project.slug)?.is_some();
        if state::needs_sync(&connection, "project", &project.slug, &content)? {
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

        let is_update = db::get_sync_state(&connection, "session", &session.session_id)?.is_some();
        if state::needs_sync(&connection, "session", &session.session_id, &content)? {
            write_note(vault_path, &rel_path, &content)?;
            state::record_sync(&connection, "session", &session.session_id, &content, &rel_path)?;
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
        let is_update = db::get_sync_state(&connection, "task", &source_id)?.is_some();
        if state::needs_sync(&connection, "task", &source_id, &content)? {
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
        let is_update = db::get_sync_state(&connection, "memory", &source_id)?.is_some();
        if state::needs_sync(&connection, "memory", &source_id, &content)? {
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

        let is_update = db::get_sync_state(&connection, "entity", &ec.entity)?.is_some();
        if state::needs_sync(&connection, "entity", &ec.entity, &content)? {
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
        // Check for deleted sessions
        let synced_sessions = state::synced_paths_for_kind(&connection, "session")?;
        for (source_id, _path) in synced_sessions {
            if !active_session_slugs.contains(&source_id) {
                state::remove_synced_file(&connection, "session", &source_id, vault_path, true)?;
                deleted += 1;
            }
        }

        // Check for deleted tasks
        let synced_tasks = state::synced_paths_for_kind(&connection, "task")?;
        for (source_id, _path) in synced_tasks {
            let slug = source_id.split(':').last().unwrap_or("");
            if !all_tasks.iter().any(|t| t.slug == slug) {
                state::remove_synced_file(&connection, "task", &source_id, vault_path, true)?;
                deleted += 1;
            }
        }
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
