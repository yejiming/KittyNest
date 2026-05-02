pub fn upsert_raw_sessions(
    connection: &mut rusqlite::Connection,
    sessions: &[RawSession],
) -> anyhow::Result<usize> {
    let tx = connection.transaction()?;
    let mut changed_count = 0;
    for session in sessions {
        let project_id = ensure_project_for_workdir_tx(
            &tx,
            &session.workdir,
            &session.source,
            &session.updated_at,
        )?;
        let messages_json = serde_json::to_string(&session.messages)?;
        let existing: Option<(i64, String, String, String)> = tx
            .query_row(
                r#"
                SELECT id, updated_at, raw_path, messages_json
                FROM sessions
                WHERE source = ?1 AND session_id = ?2
                "#,
                params![session.source, session.session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;

        if let Some((id, updated_at, raw_path, existing_messages_json)) = existing {
            if updated_at != session.updated_at
                || raw_path != session.raw_path
                || existing_messages_json != messages_json
            {
                tx.execute(
                    r#"
                    UPDATE sessions
                    SET workdir = ?1,
                        task_id = CASE WHEN project_id = ?2 THEN task_id ELSE NULL END,
                        project_id = ?2,
                        title = NULL,
                        summary = NULL, summary_path = NULL, raw_path = ?3,
                        updated_at = ?4, processed_at = NULL, analysis_status = 'pending',
                        analysis_error = NULL, messages_json = ?5
                    WHERE id = ?6
                    "#,
                    params![
                        session.workdir,
                        project_id,
                        session.raw_path,
                        session.updated_at,
                        messages_json,
                        id
                    ],
                )?;
                changed_count += 1;
            }
        } else {
            tx.execute(
                r#"
                INSERT INTO sessions
                  (source, session_id, workdir, project_id, raw_path, created_at, updated_at, messages_json)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    session.source,
                    session.session_id,
                    session.workdir,
                    project_id,
                    session.raw_path,
                    session.created_at,
                    session.updated_at,
                    messages_json,
                ],
            )?;
            changed_count += 1;
        }
    }
    tx.commit()?;
    Ok(changed_count)
}

pub fn delete_sessions_missing_from_scan(
    connection: &rusqlite::Connection,
    scanned: &[RawSession],
    sources: &[&str],
) -> anyhow::Result<Vec<StoredSession>> {
    let scanned_sources = sources
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let scanned_keys = scanned
        .iter()
        .map(|session| (session.source.clone(), session.session_id.clone()))
        .collect::<std::collections::BTreeSet<_>>();
    let removed = stored_sessions_for_where(connection, "1 = 1", [])?
        .into_iter()
        .filter(|session| {
            scanned_sources.contains(session.source.as_str())
                && !scanned_keys.contains(&(session.source.clone(), session.session_id.clone()))
        })
        .collect::<Vec<_>>();
    for session in &removed {
        delete_session_memories(connection, session)?;
        connection.execute("DELETE FROM sessions WHERE id = ?1", params![session.id])?;
    }
    Ok(removed)
}

pub fn list_sessions(connection: &rusqlite::Connection) -> anyhow::Result<Vec<SessionRecord>> {
    let mut statement = connection.prepare(
        r#"
        SELECT s.source, s.session_id, s.raw_path, p.slug, t.slug, s.title, s.summary,
               s.summary_path, s.created_at, s.updated_at, s.analysis_status
        FROM sessions s
        JOIN projects p ON p.id = s.project_id
        LEFT JOIN tasks t ON t.id = s.task_id
        ORDER BY s.updated_at DESC, s.created_at DESC
        LIMIT 100
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok(SessionRecord {
            source: row.get(0)?,
            session_id: row.get(1)?,
            raw_path: row.get(2)?,
            project_slug: row.get(3)?,
            task_slug: row.get(4)?,
            title: row.get(5)?,
            summary: row.get(6)?,
            summary_path: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            status: row.get(10)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn sessions_by_session_ids(
    connection: &rusqlite::Connection,
    session_ids: &[String],
) -> anyhow::Result<Vec<SessionRecord>> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = std::iter::repeat("?")
        .take(session_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
        SELECT s.source, s.session_id, s.raw_path, p.slug, t.slug, s.title, s.summary,
               s.summary_path, s.created_at, s.updated_at, s.analysis_status
        FROM sessions s
        JOIN projects p ON p.id = s.project_id
        LEFT JOIN tasks t ON t.id = s.task_id
        WHERE s.session_id IN ({placeholders})
        "#
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(session_ids.iter()), |row| {
        Ok(SessionRecord {
            source: row.get(0)?,
            session_id: row.get(1)?,
            raw_path: row.get(2)?,
            project_slug: row.get(3)?,
            task_slug: row.get(4)?,
            title: row.get(5)?,
            summary: row.get(6)?,
            summary_path: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            status: row.get(10)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn dashboard_stats(connection: &rusqlite::Connection) -> anyhow::Result<DashboardStats> {
    let active_projects = count(connection, "SELECT COUNT(*) FROM projects")?;
    let open_tasks = count(
        connection,
        "SELECT COUNT(*) FROM tasks WHERE status != 'done'",
    )?;
    let sessions = count(connection, "SELECT COUNT(*) FROM sessions")?;
    let unprocessed_sessions = count(
        connection,
        "SELECT COUNT(*) FROM sessions WHERE analysis_status = 'pending'",
    )?;
    let memories = count(connection, "SELECT COUNT(*) FROM session_memories")?;
    Ok(DashboardStats {
        active_projects,
        open_tasks,
        sessions,
        unprocessed_sessions,
        memories,
        entities: 0,
    })
}

pub fn record_llm_provider_call(
    connection: &rusqlite::Connection,
    provider: &str,
) -> anyhow::Result<()> {
    let provider = provider.trim();
    if provider.is_empty() {
        return Ok(());
    }
    connection.execute(
        r#"
        INSERT INTO llm_provider_calls (provider, calls)
        VALUES (?1, 1)
        ON CONFLICT(provider) DO UPDATE SET calls = calls + 1
        "#,
        params![provider],
    )?;
    Ok(())
}

pub fn record_llm_provider_call_for_paths(paths: &AppPaths, provider: &str) {
    let Ok(connection) = open(paths) else {
        return;
    };
    if migrate(&connection).is_ok() {
        let _ = record_llm_provider_call(&connection, provider);
    }
}

pub fn unprocessed_sessions(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<StoredSession>> {
    stored_sessions_for_where(connection, "s.analysis_status IN ('pending', 'failed')", [])
}

pub fn unprocessed_sessions_updated_after(
    connection: &rusqlite::Connection,
    updated_after: &str,
) -> anyhow::Result<Vec<StoredSession>> {
    stored_sessions_for_where(
        connection,
        "s.analysis_status IN ('pending', 'failed') AND s.updated_at >= ?1",
        params![updated_after],
    )
}

pub fn unprocessed_sessions_by_project_slug(
    connection: &rusqlite::Connection,
    project_slug: &str,
) -> anyhow::Result<Vec<StoredSession>> {
    stored_sessions_for_where(
        connection,
        "s.analysis_status IN ('pending', 'failed') AND p.slug = ?1",
        params![project_slug],
    )
}

pub fn project_sessions_needing_analysis_limited(
    connection: &rusqlite::Connection,
    project_slug: &str,
    limit: usize,
) -> anyhow::Result<Vec<StoredSession>> {
    let sql = format!(
        r#"
        SELECT s.id, s.source, s.session_id, s.project_id, p.slug, s.task_id, s.workdir,
               s.created_at, s.updated_at, s.messages_json
        FROM (
          SELECT s.id, s.source, s.session_id, s.project_id, s.task_id, s.workdir,
                 s.created_at, s.updated_at, s.messages_json, s.analysis_status
          FROM sessions s
          JOIN projects p ON p.id = s.project_id
          WHERE p.slug = ?1
          ORDER BY s.updated_at DESC, s.created_at DESC, s.id DESC
          LIMIT {limit}
        ) s
        JOIN projects p ON p.id = s.project_id
        WHERE s.analysis_status IN ('pending', 'failed')
        ORDER BY s.updated_at DESC, s.created_at DESC, s.id DESC
        LIMIT {limit}
        "#
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params![project_slug], stored_session_from_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn unprocessed_session_by_session_id(
    connection: &rusqlite::Connection,
    session_id: &str,
) -> anyhow::Result<Vec<StoredSession>> {
    stored_sessions_for_where(
        connection,
        "s.session_id = ?1 AND s.analysis_status IN ('pending', 'failed')",
        params![session_id],
    )
}

pub fn session_is_unprocessed(
    connection: &rusqlite::Connection,
    session_row_id: i64,
) -> anyhow::Result<bool> {
    let unprocessed = connection.query_row(
        "SELECT COUNT(*) FROM sessions WHERE id = ?1 AND analysis_status IN ('pending', 'failed')",
        params![session_row_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(unprocessed > 0)
}

pub fn analyzed_session_summaries_by_project_slug(
    connection: &rusqlite::Connection,
    project_slug: &str,
    limit: usize,
) -> anyhow::Result<Vec<ProjectSessionSummary>> {
    let sql = format!(
        r#"
        SELECT s.session_id, COALESCE(s.title, s.session_id), COALESCE(s.summary, ''),
               t.slug, s.created_at, s.updated_at
        FROM (
          SELECT s.id, s.session_id, s.title, s.summary, s.task_id, s.created_at, s.updated_at, s.analysis_status
          FROM sessions s
          JOIN projects p ON p.id = s.project_id
          WHERE p.slug = ?1
          ORDER BY s.updated_at DESC, s.created_at DESC, s.id DESC
          LIMIT {limit}
        ) s
        LEFT JOIN tasks t ON t.id = s.task_id
        WHERE s.analysis_status = 'analyzed'
        ORDER BY s.created_at ASC, s.updated_at ASC, s.id ASC
        "#
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params![project_slug], |row| {
        Ok(ProjectSessionSummary {
            session_id: row.get(0)?,
            title: row.get(1)?,
            summary: row.get(2)?,
            task_slug: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn analyzed_sessions(connection: &rusqlite::Connection) -> anyhow::Result<Vec<StoredSession>> {
    stored_sessions_for_where(connection, "s.analysis_status = 'analyzed'", [])
}

pub fn project_slugs_for_recent_startup_analyzed_sessions(
    connection: &rusqlite::Connection,
    updated_after: &str,
    processed_after: &str,
) -> anyhow::Result<Vec<String>> {
    let mut statement = connection.prepare(
        r#"
        SELECT DISTINCT p.slug
        FROM sessions s
        JOIN projects p ON p.id = s.project_id
        WHERE s.analysis_status = 'analyzed'
          AND s.updated_at >= ?1
          AND s.processed_at IS NOT NULL
          AND s.processed_at >= ?2
        ORDER BY p.slug COLLATE NOCASE ASC
        "#,
    )?;
    let rows = statement.query_map(params![updated_after, processed_after], |row| row.get(0))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn sessions_needing_memory_rebuild(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<StoredSession>> {
    stored_sessions_for_where(
        connection,
        r#"
        s.analysis_status = 'analyzed'
        AND s.processed_at IS NOT NULL
        AND COALESCE((SELECT MAX(m.created_at) FROM session_memories m WHERE m.session_row_id = s.id), '1970-01-01T00:00:00Z') < s.processed_at
        "#,
        [],
    )
}

pub fn project_sessions_by_project_slug(
    connection: &rusqlite::Connection,
    project_slug: &str,
    limit: usize,
) -> anyhow::Result<Vec<StoredSession>> {
    let sql = format!(
        r#"
        SELECT s.id, s.source, s.session_id, s.project_id, p.slug, s.task_id, s.workdir,
               s.created_at, s.updated_at, s.messages_json
        FROM sessions s
        JOIN projects p ON p.id = s.project_id
        WHERE p.slug = ?1
        ORDER BY s.updated_at DESC, s.created_at DESC, s.id DESC
        LIMIT {limit}
        "#
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params![project_slug], stored_session_from_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn all_project_sessions_by_project_slug(
    connection: &rusqlite::Connection,
    project_slug: &str,
) -> anyhow::Result<Vec<StoredSession>> {
    stored_sessions_for_where(connection, "p.slug = ?1", params![project_slug])
}

pub fn project_session_count(
    connection: &rusqlite::Connection,
    project_slug: &str,
) -> anyhow::Result<usize> {
    let value = connection.query_row(
        r#"
        SELECT COUNT(s.id)
        FROM projects p
        LEFT JOIN sessions s ON s.project_id = p.id
        WHERE p.slug = ?1
        "#,
        params![project_slug],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(value as usize)
}

fn stored_sessions_for_where<P>(
    connection: &rusqlite::Connection,
    where_sql: &str,
    params: P,
) -> anyhow::Result<Vec<StoredSession>>
where
    P: rusqlite::Params,
{
    let sql = format!(
        r#"
        SELECT s.id, s.source, s.session_id, s.project_id, p.slug, s.task_id, s.workdir,
               s.created_at, s.updated_at, s.messages_json
        FROM sessions s
        JOIN projects p ON p.id = s.project_id
        WHERE {where_sql}
        ORDER BY s.updated_at DESC, s.created_at DESC, s.id DESC
        "#
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params, stored_session_from_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn stored_session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredSession> {
    let messages_json: String = row.get(9)?;
    let messages: Vec<RawMessage> = serde_json::from_str(&messages_json).unwrap_or_default();
    Ok(StoredSession {
        id: row.get(0)?,
        source: row.get(1)?,
        session_id: row.get(2)?,
        project_id: row.get(3)?,
        project_slug: row.get(4)?,
        task_id: row.get(5)?,
        workdir: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        messages,
    })
}

pub fn mark_session_processed(
    connection: &rusqlite::Connection,
    session_id: i64,
    task_id: i64,
    title: &str,
    summary: &str,
    summary_path: &str,
) -> anyhow::Result<()> {
    connection.execute(
        r#"
        UPDATE sessions
        SET task_id = ?1, title = ?2, summary = ?3, summary_path = ?4,
            processed_at = ?5, analysis_status = 'analyzed', analysis_error = NULL
        WHERE id = ?6
        "#,
        params![
            task_id,
            title,
            summary,
            summary_path,
            crate::utils::now_rfc3339(),
            session_id
        ],
    )?;
    Ok(())
}

pub fn mark_session_processed_with_optional_task(
    connection: &rusqlite::Connection,
    session_id: i64,
    task_id: Option<i64>,
    title: &str,
    summary: &str,
    summary_path: &str,
) -> anyhow::Result<()> {
    mark_session_processed_with_optional_task_at(
        connection,
        session_id,
        task_id,
        title,
        summary,
        summary_path,
        &crate::utils::now_rfc3339(),
    )
}

pub fn mark_session_processed_with_optional_task_at(
    connection: &rusqlite::Connection,
    session_id: i64,
    task_id: Option<i64>,
    title: &str,
    summary: &str,
    summary_path: &str,
    processed_at: &str,
) -> anyhow::Result<()> {
    connection.execute(
        r#"
        UPDATE sessions
        SET task_id = ?1, title = ?2, summary = ?3, summary_path = ?4,
            processed_at = ?5, analysis_status = 'analyzed', analysis_error = NULL
        WHERE id = ?6
        "#,
        params![
            task_id,
            title,
            summary,
            summary_path,
            processed_at,
            session_id
        ],
    )?;
    Ok(())
}

pub fn session_processed_at(
    connection: &rusqlite::Connection,
    session_id: i64,
) -> anyhow::Result<Option<String>> {
    connection
        .query_row(
            "SELECT processed_at FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
}

pub fn mark_session_failed(
    connection: &rusqlite::Connection,
    session_id: i64,
    error: &str,
) -> anyhow::Result<()> {
    connection.execute(
        r#"
        UPDATE sessions
        SET analysis_status = 'failed', analysis_error = ?1
        WHERE id = ?2
        "#,
        params![error, session_id],
    )?;
    Ok(())
}

pub fn reset_all_sessions(connection: &rusqlite::Connection) -> anyhow::Result<usize> {
    connection
        .execute("DELETE FROM sessions", [])
        .map_err(Into::into)
}

fn count_pending_sessions(
    connection: &rusqlite::Connection,
    updated_after: Option<&str>,
) -> anyhow::Result<usize> {
    let value: i64 = match updated_after {
        Some(updated_after) => connection.query_row(
            "SELECT COUNT(*) FROM sessions WHERE analysis_status IN ('pending', 'failed') AND updated_at >= ?1",
            params![updated_after],
            |row| row.get(0),
        )?,
        None => connection.query_row(
            "SELECT COUNT(*) FROM sessions WHERE analysis_status IN ('pending', 'failed')",
            [],
            |row| row.get(0),
        )?,
    };
    Ok(value as usize)
}

fn count_memory_rebuild_sessions(connection: &rusqlite::Connection) -> anyhow::Result<usize> {
    let value: i64 = connection.query_row(
        r#"
        SELECT COUNT(*)
        FROM sessions s
        WHERE s.analysis_status = 'analyzed'
          AND s.processed_at IS NOT NULL
          AND COALESCE((SELECT MAX(m.created_at) FROM session_memories m WHERE m.session_row_id = s.id), '1970-01-01T00:00:00Z') < s.processed_at
        "#,
        [],
        |row| row.get(0),
    )?;
    Ok(value as usize)
}

fn count_project_sessions_needing_analysis(
    connection: &rusqlite::Connection,
    project_slug: &str,
) -> anyhow::Result<usize> {
    let value: i64 = connection.query_row(
        &format!(
            r#"
        SELECT COUNT(*)
        FROM (
          SELECT s.analysis_status
          FROM sessions s
          JOIN projects p ON p.id = s.project_id
          WHERE p.slug = ?1
          ORDER BY s.updated_at DESC, s.created_at DESC, s.id DESC
          LIMIT {}
        ) recent
        WHERE recent.analysis_status IN ('pending', 'failed')
        "#,
            PROJECT_ANALYZE_SESSION_LIMIT
        ),
        params![project_slug],
        |row| row.get(0),
    )?;
    Ok(value as usize)
}

fn merge_sources(existing: &str, next: &str) -> String {
    let mut sources = split_sources(existing);
    if !sources.iter().any(|source| source == next) {
        sources.push(next.to_string());
    }
    sources.join(",")
}

fn split_sources(sources: &str) -> Vec<String> {
    sources
        .split(',')
        .map(str::trim)
        .filter(|source| !source.is_empty())
        .map(ToString::to_string)
        .collect()
}
