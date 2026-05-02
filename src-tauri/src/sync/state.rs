use crate::db;
use crate::models::SyncKindCounts;
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
