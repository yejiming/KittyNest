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
