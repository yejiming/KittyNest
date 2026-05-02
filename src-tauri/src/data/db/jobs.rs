pub fn list_llm_provider_calls(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<ProviderCallCount>> {
    let mut statement = connection.prepare(
        r#"
        SELECT provider, calls
        FROM llm_provider_calls
        WHERE calls > 0
        ORDER BY calls DESC, provider COLLATE NOCASE ASC
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        let calls: i64 = row.get(1)?;
        Ok(ProviderCallCount {
            provider: row.get(0)?,
            calls: calls as usize,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn enqueue_analyze_sessions(
    connection: &rusqlite::Connection,
    updated_after: Option<&str>,
) -> anyhow::Result<EnqueueJobResult> {
    let total = count_pending_sessions(connection, updated_after)?;
    enqueue_job(
        connection,
        "analyze_sessions",
        "all_unprocessed",
        None,
        None,
        None,
        updated_after,
        total,
    )
}

pub fn enqueue_analyze_project_sessions(
    connection: &rusqlite::Connection,
    project_slug: &str,
) -> anyhow::Result<EnqueueJobResult> {
    let total = connection.query_row(
        r#"
        SELECT COUNT(*)
        FROM sessions s
        JOIN projects p ON p.id = s.project_id
        WHERE s.analysis_status = 'pending' AND p.slug = ?1
        "#,
        params![project_slug],
        |row| row.get::<_, i64>(0),
    )? as usize;
    enqueue_job(
        connection,
        "analyze_project_sessions",
        "project_unprocessed",
        None,
        Some(project_slug),
        None,
        None,
        total,
    )
}

pub fn enqueue_analyze_project(
    connection: &rusqlite::Connection,
    project_slug: &str,
) -> anyhow::Result<EnqueueJobResult> {
    let pending = count_project_sessions_needing_analysis(connection, project_slug)?;
    let total = pending + 4;
    enqueue_job(
        connection,
        "analyze_project",
        "project_analysis",
        None,
        Some(project_slug),
        None,
        None,
        total,
    )
}

pub fn enqueue_analyze_session(
    connection: &rusqlite::Connection,
    session_id: &str,
) -> anyhow::Result<EnqueueJobResult> {
    let total = connection.query_row(
        "SELECT COUNT(*) FROM sessions WHERE session_id = ?1",
        params![session_id],
        |row| row.get::<_, i64>(0),
    )? as usize;
    enqueue_job(
        connection,
        "analyze_session",
        "single_session",
        Some(session_id),
        None,
        None,
        None,
        total,
    )
}

pub fn enqueue_review_project(
    connection: &rusqlite::Connection,
    project_slug: &str,
) -> anyhow::Result<EnqueueJobResult> {
    let exists = connection.query_row(
        "SELECT COUNT(*) FROM projects WHERE slug = ?1",
        params![project_slug],
        |row| row.get::<_, i64>(0),
    )?;
    if exists == 0 {
        anyhow::bail!("project not found: {project_slug}");
    }
    enqueue_job(
        connection,
        "review_project",
        "project_summary",
        None,
        Some(project_slug),
        None,
        None,
        1,
    )
}

pub fn enqueue_scan_sources(connection: &rusqlite::Connection) -> anyhow::Result<EnqueueJobResult> {
    enqueue_job(
        connection,
        "scan_sources",
        "source_scan",
        None,
        None,
        None,
        None,
        1,
    )
}

pub fn prepare_save_agent_session_job(
    connection: &rusqlite::Connection,
    session_id: &str,
    project_slug: &str,
) -> anyhow::Result<EnqueueJobResult> {
    let now = crate::utils::now_rfc3339();
    connection.execute(
        r#"
        INSERT INTO jobs
          (kind, scope, session_id, project_slug, task_slug, updated_after, status, total, completed, failed, message, started_at, updated_at)
        VALUES ('save_agent_session', 'assistant_session_save', ?1, ?2, NULL, NULL, 'preparing', 1, 0, 0, 'Preparing assistant session save', ?3, ?3)
        "#,
        params![session_id, project_slug, now],
    )?;
    Ok(EnqueueJobResult {
        job_id: connection.last_insert_rowid(),
        total: 1,
    })
}

pub fn queue_prepared_job(connection: &rusqlite::Connection, job_id: i64) -> anyhow::Result<()> {
    connection.execute(
        r#"
        UPDATE jobs
        SET status = 'queued', message = 'Queued for analysis', updated_at = ?1
        WHERE id = ?2 AND status = 'preparing'
        "#,
        params![crate::utils::now_rfc3339(), job_id],
    )?;
    Ok(())
}

pub fn enqueue_generate_task_prompt(
    connection: &rusqlite::Connection,
    project_slug: &str,
    task_slug: &str,
) -> anyhow::Result<EnqueueJobResult> {
    enqueue_job(
        connection,
        "generate_task_prompt",
        "task_prompt",
        None,
        Some(project_slug),
        Some(task_slug),
        None,
        1,
    )
}

pub fn enqueue_rebuild_memories(
    connection: &rusqlite::Connection,
) -> anyhow::Result<EnqueueJobResult> {
    let total = count_memory_rebuild_sessions(connection)? + 1;
    enqueue_job(
        connection,
        "rebuild_memories",
        "memory_rebuild",
        None,
        None,
        None,
        None,
        total,
    )
}

pub fn enqueue_search_memories(
    connection: &rusqlite::Connection,
    query: &str,
) -> anyhow::Result<EnqueueJobResult> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        anyhow::bail!("memory search query cannot be empty");
    }
    let job = enqueue_job(
        connection,
        "search_memories",
        "memory_search",
        None,
        None,
        None,
        None,
        1,
    )?;
    create_memory_search(connection, job.job_id, trimmed)?;
    Ok(job)
}

fn enqueue_job(
    connection: &rusqlite::Connection,
    kind: &str,
    scope: &str,
    session_id: Option<&str>,
    project_slug: Option<&str>,
    task_slug: Option<&str>,
    updated_after: Option<&str>,
    total: usize,
) -> anyhow::Result<EnqueueJobResult> {
    let now = crate::utils::now_rfc3339();
    connection.execute(
        r#"
        INSERT INTO jobs
          (kind, scope, session_id, project_slug, task_slug, updated_after, status, total, completed, failed, message, started_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued', ?7, 0, 0, ?8, ?9, ?9)
        "#,
        params![
            kind,
            scope,
            session_id,
            project_slug,
            task_slug,
            updated_after,
            total as i64,
            "Queued for analysis",
            now
        ],
    )?;
    Ok(EnqueueJobResult {
        job_id: connection.last_insert_rowid(),
        total,
    })
}

pub fn list_active_jobs(connection: &rusqlite::Connection) -> anyhow::Result<Vec<JobRecord>> {
    let mut statement = connection.prepare(
        r#"
        SELECT id, kind, scope, session_id, project_slug, task_slug, updated_after, status, total, completed, failed,
               message, started_at, updated_at, completed_at
        FROM jobs
        WHERE status IN ('queued', 'running', 'failed')
        ORDER BY started_at ASC, id ASC
        "#,
    )?;
    let rows = statement.query_map([], job_from_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn mark_stale_running_jobs_queued(connection: &rusqlite::Connection) -> anyhow::Result<()> {
    connection.execute(
        r#"
        UPDATE jobs
        SET status = 'queued', message = 'Recovered after restart', updated_at = ?1
        WHERE status = 'running'
        "#,
        params![crate::utils::now_rfc3339()],
    )?;
    Ok(())
}

pub fn claim_next_job(connection: &rusqlite::Connection) -> anyhow::Result<Option<JobRecord>> {
    loop {
        let job_id: Option<i64> = connection
            .query_row(
                "SELECT id FROM jobs WHERE status = 'queued' ORDER BY started_at ASC, id ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(job_id) = job_id else {
            return Ok(None);
        };
        let now = crate::utils::now_rfc3339();
        let changed = connection.execute(
            r#"
            UPDATE jobs
            SET status = 'running', message = 'Analyzing sessions', updated_at = ?1
            WHERE id = ?2 AND status = 'queued'
            "#,
            params![now, job_id],
        )?;
        if changed > 0 {
            return get_job(connection, job_id);
        }
    }
}

pub fn update_job_progress(
    connection: &rusqlite::Connection,
    job_id: i64,
    completed: usize,
    failed: usize,
    message: &str,
) -> anyhow::Result<()> {
    connection.execute(
        r#"
        UPDATE jobs
        SET completed = ?1, failed = ?2, message = ?3, updated_at = ?4
        WHERE id = ?5
        "#,
        params![
            completed as i64,
            failed as i64,
            message,
            crate::utils::now_rfc3339(),
            job_id
        ],
    )?;
    Ok(())
}

pub fn complete_job(
    connection: &rusqlite::Connection,
    job_id: i64,
    message: &str,
) -> anyhow::Result<()> {
    let now = crate::utils::now_rfc3339();
    connection.execute(
        r#"
        UPDATE jobs
        SET status = 'completed', message = ?1, updated_at = ?2, completed_at = ?2
        WHERE id = ?3
        "#,
        params![message, now, job_id],
    )?;
    Ok(())
}

pub fn fail_job(
    connection: &rusqlite::Connection,
    job_id: i64,
    message: &str,
) -> anyhow::Result<()> {
    let now = crate::utils::now_rfc3339();
    connection.execute(
        r#"
        UPDATE jobs
        SET status = 'failed', message = ?1, updated_at = ?2, completed_at = ?2
        WHERE id = ?3
        "#,
        params![message, now, job_id],
    )?;
    Ok(())
}

pub fn cancel_job(connection: &rusqlite::Connection, job_id: i64) -> anyhow::Result<bool> {
    let now = crate::utils::now_rfc3339();
    let changed = connection.execute(
        r#"
        UPDATE jobs
        SET status = 'canceled', message = 'Stopped', updated_at = ?1, completed_at = ?1
        WHERE id = ?2 AND status IN ('queued', 'running', 'failed')
        "#,
        params![now, job_id],
    )?;
    Ok(changed > 0)
}

pub fn job_is_active(connection: &rusqlite::Connection, job_id: i64) -> anyhow::Result<bool> {
    let active = connection.query_row(
        "SELECT COUNT(*) FROM jobs WHERE id = ?1 AND status IN ('queued', 'running')",
        params![job_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(active > 0)
}

fn get_job(connection: &rusqlite::Connection, job_id: i64) -> anyhow::Result<Option<JobRecord>> {
    connection
        .query_row(
            r#"
            SELECT id, kind, scope, session_id, project_slug, task_slug, updated_after, status, total, completed, failed,
                   message, started_at, updated_at, completed_at
            FROM jobs
            WHERE id = ?1
            "#,
            params![job_id],
            job_from_row,
        )
        .optional()
        .map_err(Into::into)
}

pub fn enqueue_sync_to_obsidian(
    connection: &rusqlite::Connection,
    mode: &str,
) -> anyhow::Result<i64> {
    let now = crate::utils::now_rfc3339();
    connection.execute(
        "INSERT INTO jobs (kind, scope, status, message, started_at, updated_at)
         VALUES ('sync_to_obsidian', ?1, 'queued', '', ?2, ?2)",
        rusqlite::params![mode, now],
    )?;
    Ok(connection.last_insert_rowid())
}

fn job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobRecord> {
    let total: i64 = row.get(8)?;
    let completed: i64 = row.get(9)?;
    let failed: i64 = row.get(10)?;
    let pending = (total - completed - failed).max(0) as usize;
    Ok(JobRecord {
        id: row.get(0)?,
        kind: row.get(1)?,
        scope: row.get(2)?,
        session_id: row.get(3)?,
        project_slug: row.get(4)?,
        task_slug: row.get(5)?,
        updated_after: row.get(6)?,
        status: row.get(7)?,
        total: total as usize,
        completed: completed as usize,
        failed: failed as usize,
        pending,
        message: row.get(11)?,
        started_at: row.get(12)?,
        updated_at: row.get(13)?,
        completed_at: row.get(14)?,
    })
}

