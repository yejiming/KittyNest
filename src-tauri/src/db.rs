use rusqlite::{params, OptionalExtension};

use crate::models::{
    AppPaths, DashboardStats, EnqueueJobResult, JobRecord, ProjectRecord, ProjectSessionSummary,
    RawMessage, RawSession, SessionRecord, StoredSession, TaskRecord,
};

pub fn open(paths: &AppPaths) -> anyhow::Result<rusqlite::Connection> {
    std::fs::create_dir_all(&paths.data_dir)?;
    Ok(rusqlite::Connection::open(&paths.db_path)?)
}

pub fn migrate(connection: &rusqlite::Connection) -> anyhow::Result<()> {
    connection.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS projects (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          slug TEXT NOT NULL UNIQUE,
          display_title TEXT NOT NULL,
          workdir TEXT NOT NULL UNIQUE,
          sources TEXT NOT NULL DEFAULT '',
          info_path TEXT,
          progress_path TEXT,
          review_status TEXT NOT NULL DEFAULT 'not_reviewed',
          last_reviewed_at TEXT,
          last_session_at TEXT,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tasks (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
          slug TEXT NOT NULL,
          title TEXT NOT NULL,
          brief TEXT NOT NULL,
          status TEXT NOT NULL,
          summary_path TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          UNIQUE(project_id, slug)
        );

        CREATE TABLE IF NOT EXISTS sessions (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          source TEXT NOT NULL,
          session_id TEXT NOT NULL,
          workdir TEXT NOT NULL,
          project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
          task_id INTEGER REFERENCES tasks(id) ON DELETE SET NULL,
          title TEXT,
          summary TEXT,
          summary_path TEXT,
          raw_path TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          processed_at TEXT,
          analysis_status TEXT NOT NULL DEFAULT 'pending',
          analysis_error TEXT,
          messages_json TEXT NOT NULL,
          UNIQUE(source, session_id)
        );

        CREATE TABLE IF NOT EXISTS jobs (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          kind TEXT NOT NULL,
          project_slug TEXT,
          task_slug TEXT,
          scope TEXT NOT NULL DEFAULT 'all_unprocessed',
          session_id TEXT,
          updated_after TEXT,
          status TEXT NOT NULL,
          total INTEGER NOT NULL DEFAULT 0,
          completed INTEGER NOT NULL DEFAULT 0,
          failed INTEGER NOT NULL DEFAULT 0,
          message TEXT NOT NULL,
          started_at TEXT NOT NULL,
          updated_at TEXT NOT NULL DEFAULT '',
          completed_at TEXT
        );
        "#,
    )?;
    add_column_if_missing(
        connection,
        "jobs",
        "scope",
        "scope TEXT NOT NULL DEFAULT 'all_unprocessed'",
    )?;
    add_column_if_missing(connection, "jobs", "session_id", "session_id TEXT")?;
    add_column_if_missing(connection, "jobs", "project_slug", "project_slug TEXT")?;
    add_column_if_missing(connection, "jobs", "task_slug", "task_slug TEXT")?;
    add_column_if_missing(
        connection,
        "jobs",
        "total",
        "total INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        connection,
        "jobs",
        "completed",
        "completed INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        connection,
        "jobs",
        "failed",
        "failed INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        connection,
        "jobs",
        "updated_at",
        "updated_at TEXT NOT NULL DEFAULT ''",
    )?;
    add_column_if_missing(
        connection,
        "sessions",
        "analysis_status",
        "analysis_status TEXT NOT NULL DEFAULT 'pending'",
    )?;
    add_column_if_missing(
        connection,
        "sessions",
        "analysis_error",
        "analysis_error TEXT",
    )?;
    add_column_if_missing(connection, "jobs", "updated_after", "updated_after TEXT")?;
    connection.execute(
        "UPDATE sessions SET analysis_status = 'analyzed' WHERE processed_at IS NOT NULL AND analysis_status = 'pending'",
        [],
    )?;
    Ok(())
}

fn add_column_if_missing(
    connection: &rusqlite::Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> anyhow::Result<()> {
    let columns = connection
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|name| name == column) {
        connection.execute(&format!("ALTER TABLE {table} ADD COLUMN {definition}"), [])?;
    }
    Ok(())
}

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

pub fn ensure_project_for_workdir(
    connection: &rusqlite::Connection,
    workdir: &str,
    source: &str,
    last_session_at: &str,
) -> anyhow::Result<i64> {
    ensure_project_for_workdir_tx(connection, workdir, source, last_session_at)
}

fn ensure_project_for_workdir_tx(
    connection: &rusqlite::Connection,
    workdir: &str,
    source: &str,
    last_session_at: &str,
) -> anyhow::Result<i64> {
    let now = crate::utils::now_rfc3339();
    let existing: Option<(i64, String)> = connection
        .query_row(
            "SELECT id, sources FROM projects WHERE workdir = ?1",
            params![workdir],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    if let Some((id, sources)) = existing {
        let merged = merge_sources(&sources, source);
        connection.execute(
            "UPDATE projects SET sources = ?1, last_session_at = ?2, updated_at = ?3 WHERE id = ?4",
            params![merged, last_session_at, now, id],
        )?;
        return Ok(id);
    }

    let slug = unique_project_slug(
        connection,
        &crate::utils::project_slug_from_workdir(workdir),
    )?;
    connection.execute(
        r#"
        INSERT INTO projects (slug, display_title, workdir, sources, last_session_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![slug, slug, workdir, source, last_session_at, now],
    )?;
    Ok(connection.last_insert_rowid())
}

fn unique_project_slug(
    connection: &rusqlite::Connection,
    base_slug: &str,
) -> anyhow::Result<String> {
    let existing: Option<i64> = connection
        .query_row(
            "SELECT id FROM projects WHERE slug = ?1",
            params![base_slug],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_none() {
        return Ok(base_slug.to_string());
    }

    let mut suffix = 2;
    loop {
        let candidate = format!("{base_slug}-{suffix}");
        let exists: Option<i64> = connection
            .query_row(
                "SELECT id FROM projects WHERE slug = ?1",
                params![candidate],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Ok(candidate);
        }
        suffix += 1;
    }
}

pub fn list_projects(connection: &rusqlite::Connection) -> anyhow::Result<Vec<ProjectRecord>> {
    let mut statement = connection.prepare(
        r#"
        SELECT slug, display_title, workdir, sources, info_path, progress_path,
               review_status, last_reviewed_at, last_session_at
        FROM projects
        ORDER BY COALESCE(last_session_at, updated_at) DESC, display_title ASC
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        let sources: String = row.get(3)?;
        Ok(ProjectRecord {
            slug: row.get(0)?,
            display_title: row.get(1)?,
            workdir: row.get(2)?,
            sources: split_sources(&sources),
            info_path: row.get(4)?,
            progress_path: row.get(5)?,
            review_status: row.get(6)?,
            last_reviewed_at: row.get(7)?,
            last_session_at: row.get(8)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get_project_by_slug(
    connection: &rusqlite::Connection,
    slug: &str,
) -> anyhow::Result<Option<(i64, ProjectRecord)>> {
    connection
        .query_row(
            r#"
            SELECT id, slug, display_title, workdir, sources, info_path, progress_path,
                   review_status, last_reviewed_at, last_session_at
            FROM projects
            WHERE slug = ?1
            "#,
            params![slug],
            |row| {
                let sources: String = row.get(4)?;
                Ok((
                    row.get(0)?,
                    ProjectRecord {
                        slug: row.get(1)?,
                        display_title: row.get(2)?,
                        workdir: row.get(3)?,
                        sources: split_sources(&sources),
                        info_path: row.get(5)?,
                        progress_path: row.get(6)?,
                        review_status: row.get(7)?,
                        last_reviewed_at: row.get(8)?,
                        last_session_at: row.get(9)?,
                    },
                ))
            },
        )
        .optional()
        .map_err(Into::into)
}

pub fn list_tasks(connection: &rusqlite::Connection) -> anyhow::Result<Vec<TaskRecord>> {
    let mut statement = connection.prepare(
        r#"
        SELECT p.slug, t.slug, t.title, t.brief,
               CASE WHEN COUNT(s.id) = 0 THEN 'discussing' ELSE t.status END AS status,
               t.summary_path, t.updated_at,
               COUNT(s.id) AS session_count
        FROM tasks t
        JOIN projects p ON p.id = t.project_id
        LEFT JOIN sessions s ON s.task_id = t.id
        GROUP BY t.id
        ORDER BY t.updated_at DESC, t.title ASC
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        let session_count: i64 = row.get(7)?;
        Ok(TaskRecord {
            project_slug: row.get(0)?,
            slug: row.get(1)?,
            title: row.get(2)?,
            brief: row.get(3)?,
            status: row.get(4)?,
            summary_path: row.get(5)?,
            updated_at: row.get(6)?,
            session_count: session_count as usize,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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
    Ok(DashboardStats {
        active_projects,
        open_tasks,
        sessions,
        unprocessed_sessions,
        memories: 0,
    })
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
    let total = pending.min(20) + 2;
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

pub fn unprocessed_sessions(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Vec<StoredSession>> {
    stored_sessions_for_where(connection, "s.analysis_status = 'pending'", [])
}

pub fn unprocessed_sessions_updated_after(
    connection: &rusqlite::Connection,
    updated_after: &str,
) -> anyhow::Result<Vec<StoredSession>> {
    stored_sessions_for_where(
        connection,
        "s.analysis_status = 'pending' AND s.updated_at >= ?1",
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
        FROM sessions s
        JOIN projects p ON p.id = s.project_id
        WHERE s.analysis_status IN ('pending', 'failed') AND p.slug = ?1
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
) -> anyhow::Result<Vec<ProjectSessionSummary>> {
    let mut statement = connection.prepare(
        r#"
        SELECT s.session_id, COALESCE(s.title, s.session_id), COALESCE(s.summary, ''),
               t.slug, s.created_at, s.updated_at
        FROM sessions s
        JOIN projects p ON p.id = s.project_id
        LEFT JOIN tasks t ON t.id = s.task_id
        WHERE p.slug = ?1 AND s.analysis_status = 'analyzed'
        ORDER BY s.created_at ASC, s.updated_at ASC, s.id ASC
        "#,
    )?;
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

pub fn task_status_by_slug(
    connection: &rusqlite::Connection,
    project_id: i64,
    slug: &str,
) -> anyhow::Result<Option<(i64, String)>> {
    connection
        .query_row(
            r#"
            SELECT t.id, CASE WHEN COUNT(s.id) = 0 THEN 'discussing' ELSE t.status END
            FROM tasks t
            LEFT JOIN sessions s ON s.task_id = t.id
            WHERE t.project_id = ?1 AND t.slug = ?2
            GROUP BY t.id
            "#,
            params![project_id, slug],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(Into::into)
}

pub fn upsert_task(
    connection: &rusqlite::Connection,
    project_id: i64,
    slug: &str,
    title: &str,
    brief: &str,
    status: &str,
    summary_path: &str,
) -> anyhow::Result<(i64, bool)> {
    let now = crate::utils::now_rfc3339();
    let existing: Option<i64> = connection
        .query_row(
            "SELECT id FROM tasks WHERE project_id = ?1 AND slug = ?2",
            params![project_id, slug],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        connection.execute(
            r#"
            UPDATE tasks
            SET title = ?1, brief = ?2, status = ?3, summary_path = ?4, updated_at = ?5
            WHERE id = ?6
            "#,
            params![title, brief, status, summary_path, now, id],
        )?;
        return Ok((id, false));
    }

    connection.execute(
        r#"
        INSERT INTO tasks (project_id, slug, title, brief, status, summary_path, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![project_id, slug, title, brief, status, summary_path, now],
    )?;
    Ok((connection.last_insert_rowid(), true))
}

pub fn unique_task_slug(
    connection: &rusqlite::Connection,
    project_id: i64,
    base_slug: &str,
) -> anyhow::Result<String> {
    let base = if base_slug.trim().is_empty() {
        "task"
    } else {
        base_slug
    };
    let existing: Option<i64> = connection
        .query_row(
            "SELECT id FROM tasks WHERE project_id = ?1 AND slug = ?2",
            params![project_id, base],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_none() {
        return Ok(base.to_string());
    }

    let mut suffix = 2;
    loop {
        let candidate = format!("{base}-{suffix}");
        let exists: Option<i64> = connection
            .query_row(
                "SELECT id FROM tasks WHERE project_id = ?1 AND slug = ?2",
                params![project_id, candidate],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Ok(candidate);
        }
        suffix += 1;
    }
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

pub fn mark_session_failed(
    connection: &rusqlite::Connection,
    session_id: i64,
    error: &str,
) -> anyhow::Result<()> {
    connection.execute(
        r#"
        UPDATE sessions
        SET task_id = NULL,
            title = NULL,
            summary = NULL,
            summary_path = NULL,
            processed_at = NULL,
            analysis_status = 'failed',
            analysis_error = ?1
        WHERE id = ?2
        "#,
        params![error, session_id],
    )?;
    Ok(())
}

pub fn update_project_review(
    connection: &rusqlite::Connection,
    project_id: i64,
    info_path: &str,
) -> anyhow::Result<()> {
    let now = crate::utils::now_rfc3339();
    connection.execute(
        r#"
        UPDATE projects
        SET info_path = ?1, review_status = 'reviewed', last_reviewed_at = ?2, updated_at = ?2
        WHERE id = ?3
        "#,
        params![info_path, now, project_id],
    )?;
    Ok(())
}

pub fn update_project_progress(
    connection: &rusqlite::Connection,
    project_slug: &str,
    progress_path: &str,
) -> anyhow::Result<()> {
    connection.execute(
        "UPDATE projects SET progress_path = ?1, updated_at = ?2 WHERE slug = ?3",
        params![progress_path, crate::utils::now_rfc3339(), project_slug],
    )?;
    Ok(())
}

pub fn update_task_status(
    connection: &rusqlite::Connection,
    project_slug: &str,
    task_slug: &str,
    status: &str,
) -> anyhow::Result<bool> {
    let session_count: i64 = connection
        .query_row(
            r#"
            SELECT COUNT(s.id)
            FROM tasks t
            JOIN projects p ON p.id = t.project_id
            LEFT JOIN sessions s ON s.task_id = t.id
            WHERE t.slug = ?1 AND p.slug = ?2
            "#,
            params![task_slug, project_slug],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0);
    if session_count == 0 && status != "discussing" {
        anyhow::bail!("empty task can only be discussing");
    }
    let changed = connection.execute(
        r#"
        UPDATE tasks
        SET status = ?1, updated_at = ?2
        WHERE slug = ?3 AND project_id = (SELECT id FROM projects WHERE slug = ?4)
        "#,
        params![status, crate::utils::now_rfc3339(), task_slug, project_slug],
    )?;
    Ok(changed > 0)
}

pub fn delete_task_if_empty(
    connection: &rusqlite::Connection,
    project_slug: &str,
    task_slug: &str,
) -> anyhow::Result<bool> {
    let task: Option<(i64, i64)> = connection
        .query_row(
            r#"
            SELECT t.id, COUNT(s.id)
            FROM tasks t
            JOIN projects p ON p.id = t.project_id
            LEFT JOIN sessions s ON s.task_id = t.id
            WHERE p.slug = ?1 AND t.slug = ?2
            GROUP BY t.id
            "#,
            params![project_slug, task_slug],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((task_id, session_count)) = task else {
        return Ok(false);
    };
    if session_count > 0 {
        anyhow::bail!("task has sessions");
    }
    let changed = connection.execute("DELETE FROM tasks WHERE id = ?1", params![task_id])?;
    Ok(changed > 0)
}

pub fn reset_all_sessions(connection: &rusqlite::Connection) -> anyhow::Result<usize> {
    connection
        .execute("DELETE FROM sessions", [])
        .map_err(Into::into)
}

pub fn reset_all_projects(connection: &rusqlite::Connection) -> anyhow::Result<usize> {
    connection.execute("DELETE FROM sessions", [])?;
    connection.execute("DELETE FROM tasks", [])?;
    connection
        .execute("DELETE FROM projects", [])
        .map_err(Into::into)
}

pub fn reset_all_tasks(connection: &rusqlite::Connection) -> anyhow::Result<usize> {
    connection
        .execute("DELETE FROM tasks", [])
        .map_err(Into::into)
}

fn count(connection: &rusqlite::Connection, sql: &str) -> anyhow::Result<usize> {
    let value: i64 = connection.query_row(sql, [], |row| row.get(0))?;
    Ok(value as usize)
}

fn count_pending_sessions(
    connection: &rusqlite::Connection,
    updated_after: Option<&str>,
) -> anyhow::Result<usize> {
    let value: i64 = match updated_after {
        Some(updated_after) => connection.query_row(
            "SELECT COUNT(*) FROM sessions WHERE analysis_status = 'pending' AND updated_at >= ?1",
            params![updated_after],
            |row| row.get(0),
        )?,
        None => connection.query_row(
            "SELECT COUNT(*) FROM sessions WHERE analysis_status = 'pending'",
            [],
            |row| row.get(0),
        )?,
    };
    Ok(value as usize)
}

fn count_project_sessions_needing_analysis(
    connection: &rusqlite::Connection,
    project_slug: &str,
) -> anyhow::Result<usize> {
    let value: i64 = connection.query_row(
        r#"
        SELECT COUNT(*)
        FROM sessions s
        JOIN projects p ON p.id = s.project_id
        WHERE s.analysis_status IN ('pending', 'failed') AND p.slug = ?1
        "#,
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

#[cfg(test)]
mod tests {
    use super::{
        cancel_job, claim_next_job, delete_task_if_empty, enqueue_analyze_project_sessions,
        enqueue_analyze_session, enqueue_analyze_sessions, enqueue_review_project,
        enqueue_scan_sources, list_active_jobs, list_projects, list_sessions, list_tasks,
        mark_session_processed, mark_stale_running_jobs_queued, migrate, open, reset_all_projects,
        reset_all_sessions, reset_all_tasks, unprocessed_session_by_session_id,
        unprocessed_sessions,
        update_job_progress, update_project_progress, update_project_review, upsert_raw_sessions,
        upsert_task,
    };
    use crate::models::{AppPaths, RawMessage, RawSession};

    #[test]
    fn upsert_raw_sessions_deduplicates_by_source_and_session_id() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let session = RawSession {
            source: "codex".into(),
            session_id: "abc".into(),
            workdir: "/tmp/demo".into(),
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:01Z".into(),
            raw_path: "/tmp/session.jsonl".into(),
            messages: vec![RawMessage {
                role: "user".into(),
                content: "Build dashboard".into(),
            }],
        };

        assert_eq!(
            upsert_raw_sessions(&mut connection, &[session.clone()]).unwrap(),
            1
        );
        assert_eq!(upsert_raw_sessions(&mut connection, &[session]).unwrap(), 0);
    }

    #[test]
    fn upsert_raw_sessions_updates_modified_existing_session_and_marks_pending() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let session = RawSession {
            source: "codex".into(),
            session_id: "abc".into(),
            workdir: "/tmp/demo".into(),
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:01Z".into(),
            raw_path: "/tmp/session.jsonl".into(),
            messages: vec![RawMessage {
                role: "user".into(),
                content: "Build dashboard".into(),
            }],
        };
        upsert_raw_sessions(&mut connection, &[session.clone()]).unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "abc")
            .unwrap()
            .remove(0);
        let (task_id, _) = upsert_task(
            &connection,
            stored.project_id,
            "old-task",
            "Old Task",
            "Old brief",
            "developing",
            "/tmp/old-task.md",
        )
        .unwrap();
        mark_session_processed(
            &connection,
            stored.id,
            task_id,
            "Old title",
            "Old summary",
            "/tmp/old-session.md",
        )
        .unwrap();

        let mut changed = session;
        changed.updated_at = "2026-04-26T00:10:00Z".into();
        changed.raw_path = "/tmp/session-updated.jsonl".into();
        changed.messages = vec![RawMessage {
            role: "user".into(),
            content: "Build dashboard and session updater".into(),
        }];

        assert_eq!(upsert_raw_sessions(&mut connection, &[changed]).unwrap(), 1);
        let session = list_sessions(&connection)
            .unwrap()
            .into_iter()
            .find(|session| session.session_id == "abc")
            .unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "abc")
            .unwrap()
            .remove(0);

        assert_eq!(session.status, "pending");
        assert_eq!(session.updated_at, "2026-04-26T00:10:00Z");
        assert_eq!(session.raw_path, "/tmp/session-updated.jsonl");
        assert_eq!(session.task_slug.as_deref(), Some("old-task"));
        assert!(session.summary.is_none());
        assert_eq!(stored.task_id, Some(task_id));
        assert_eq!(
            stored.messages[0].content,
            "Build dashboard and session updater"
        );
    }

    #[test]
    fn upsert_raw_sessions_disambiguates_projects_with_same_directory_name() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let first = RawSession {
            source: "codex".into(),
            session_id: "first".into(),
            workdir: "/tmp/one/Demo".into(),
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:01Z".into(),
            raw_path: "/tmp/first.jsonl".into(),
            messages: vec![RawMessage {
                role: "user".into(),
                content: "First project".into(),
            }],
        };
        let mut second = first.clone();
        second.session_id = "second".into();
        second.workdir = "/tmp/two/Demo".into();
        second.raw_path = "/tmp/second.jsonl".into();

        assert_eq!(
            upsert_raw_sessions(&mut connection, &[first, second]).unwrap(),
            2
        );

        let projects = list_projects(&connection).unwrap();
        let mut slugs = projects
            .iter()
            .map(|project| project.slug.as_str())
            .collect::<Vec<_>>();
        slugs.sort_unstable();
        assert_eq!(projects.len(), 2);
        assert_ne!(slugs[0], slugs[1]);
        assert!(slugs.contains(&"Demo"));
    }

    #[test]
    fn enqueue_analyze_sessions_persists_total_and_active_job() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "abc".into(),
                workdir: "/tmp/demo".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/session.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Analyze me".into(),
                }],
            }],
        )
        .unwrap();

        let result = enqueue_analyze_sessions(&connection, None).unwrap();
        let jobs = list_active_jobs(&connection).unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, result.job_id);
        assert_eq!(jobs[0].kind, "analyze_sessions");
        assert_eq!(jobs[0].scope, "all_unprocessed");
        assert_eq!(jobs[0].status, "queued");
        assert_eq!(jobs[0].total, 1);
        assert_eq!(jobs[0].completed, 0);
        assert_eq!(jobs[0].pending, 1);
    }

    #[test]
    fn enqueue_analyze_sessions_filters_by_updated_after() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "recent".into(),
                    workdir: "/tmp/demo".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:00:01Z".into(),
                    raw_path: "/tmp/recent.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Analyze recent".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "old".into(),
                    workdir: "/tmp/demo".into(),
                    created_at: "2026-04-01T00:00:00Z".into(),
                    updated_at: "2026-04-01T00:00:01Z".into(),
                    raw_path: "/tmp/old.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Skip old".into(),
                    }],
                },
            ],
        )
        .unwrap();

        let result = enqueue_analyze_sessions(&connection, Some("2026-04-19T00:00:00Z")).unwrap();
        let claimed = claim_next_job(&connection).unwrap().unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(
            claimed.updated_after.as_deref(),
            Some("2026-04-19T00:00:00Z")
        );
    }

    #[test]
    fn enqueue_analyze_session_persists_single_session_job() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "single".into(),
                workdir: "/tmp/demo".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/session.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Analyze one".into(),
                }],
            }],
        )
        .unwrap();

        let result = enqueue_analyze_session(&connection, "single").unwrap();
        let jobs = list_active_jobs(&connection).unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(jobs[0].kind, "analyze_session");
        assert_eq!(jobs[0].scope, "single_session");
        assert_eq!(jobs[0].session_id.as_deref(), Some("single"));
    }

    #[test]
    fn enqueue_analyze_project_sessions_counts_only_that_project() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "target".into(),
                    workdir: "/tmp/target".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:00:01Z".into(),
                    raw_path: "/tmp/target.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Analyze target".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "other".into(),
                    workdir: "/tmp/other".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:00:01Z".into(),
                    raw_path: "/tmp/other.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Analyze other".into(),
                    }],
                },
            ],
        )
        .unwrap();
        let target = list_projects(&connection)
            .unwrap()
            .into_iter()
            .find(|project| project.workdir == "/tmp/target")
            .unwrap();

        let result = enqueue_analyze_project_sessions(&connection, &target.slug).unwrap();
        let jobs = list_active_jobs(&connection).unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(jobs[0].kind, "analyze_project_sessions");
        assert_eq!(jobs[0].scope, "project_unprocessed");
        assert_eq!(jobs[0].project_slug.as_deref(), Some(target.slug.as_str()));
    }

    #[test]
    fn enqueue_review_project_persists_project_job() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let session = RawSession {
            source: "codex".into(),
            session_id: "review".into(),
            workdir: "/tmp/review-project".into(),
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:01Z".into(),
            raw_path: "/tmp/review.jsonl".into(),
            messages: vec![RawMessage {
                role: "user".into(),
                content: "Review project".into(),
            }],
        };
        upsert_raw_sessions(&mut connection, &[session]).unwrap();
        let project = list_projects(&connection).unwrap().remove(0);

        let result = enqueue_review_project(&connection, &project.slug).unwrap();
        let jobs = list_active_jobs(&connection).unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].kind, "review_project");
        assert_eq!(jobs[0].scope, "project_summary");
        assert_eq!(jobs[0].project_slug.as_deref(), Some(project.slug.as_str()));
    }

    #[test]
    fn enqueue_scan_sources_persists_source_scan_job() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let connection = open(&paths).unwrap();
        migrate(&connection).unwrap();

        let result = enqueue_scan_sources(&connection).unwrap();
        let jobs = list_active_jobs(&connection).unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].kind, "scan_sources");
        assert_eq!(jobs[0].scope, "source_scan");
    }

    #[test]
    fn list_sessions_exposes_updated_at_and_status() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let session = RawSession {
            source: "codex".into(),
            session_id: "status".into(),
            workdir: "/tmp/status-project".into(),
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:01Z".into(),
            raw_path: "/tmp/status.jsonl".into(),
            messages: vec![RawMessage {
                role: "user".into(),
                content: "Check status".into(),
            }],
        };
        upsert_raw_sessions(&mut connection, &[session]).unwrap();

        let sessions = list_sessions(&connection).unwrap();

        assert_eq!(sessions[0].updated_at, "2026-04-26T00:00:01Z");
        assert_eq!(sessions[0].status, "pending");
    }

    #[test]
    fn unprocessed_sessions_returns_newest_updated_first() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "oldest".into(),
                    workdir: "/tmp/order-project".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:01:00Z".into(),
                    raw_path: "/tmp/oldest.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Oldest".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "newest".into(),
                    workdir: "/tmp/order-project".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:03:00Z".into(),
                    raw_path: "/tmp/newest.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Newest".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "middle".into(),
                    workdir: "/tmp/order-project".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:02:00Z".into(),
                    raw_path: "/tmp/middle.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Middle".into(),
                    }],
                },
            ],
        )
        .unwrap();

        let sessions = unprocessed_sessions(&connection).unwrap();

        assert_eq!(
            sessions
                .iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["newest", "middle", "oldest"]
        );
    }

    #[test]
    fn reset_all_sessions_deletes_session_records() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "reset-session".into(),
                workdir: "/tmp/reset-project".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/reset-session.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Reset session".into(),
                }],
            }],
        )
        .unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "reset-session")
            .unwrap()
            .remove(0);
        let (task_id, _) = upsert_task(
            &connection,
            stored.project_id,
            "reset-task",
            "Reset Task",
            "Brief",
            "done",
            "/tmp/reset-task.md",
        )
        .unwrap();
        mark_session_processed(
            &connection,
            stored.id,
            task_id,
            "Analyzed",
            "Summary",
            "/tmp/reset-session.md",
        )
        .unwrap();

        assert_eq!(reset_all_sessions(&connection).unwrap(), 1);

        assert!(list_sessions(&connection).unwrap().is_empty());
    }

    #[test]
    fn reset_all_projects_deletes_projects_and_related_records() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "project-reset".into(),
                workdir: "/tmp/project-reset".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/project-reset.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Reset project".into(),
                }],
            }],
        )
        .unwrap();
        let (project_id, project) = super::get_project_by_slug(
            &connection,
            &list_projects(&connection).unwrap().remove(0).slug,
        )
        .unwrap()
        .unwrap();
        update_project_review(&connection, project_id, "/tmp/info.md").unwrap();
        update_project_progress(&connection, &project.slug, "/tmp/progress.md").unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "project-reset")
            .unwrap()
            .remove(0);
        let (task_id, _) = upsert_task(
            &connection,
            stored.project_id,
            "project-reset",
            "Project Reset",
            "Brief",
            "developing",
            "/tmp/project-reset.md",
        )
        .unwrap();
        mark_session_processed(
            &connection,
            stored.id,
            task_id,
            "Project Reset",
            "Summary",
            "/tmp/project-reset-session.md",
        )
        .unwrap();

        assert_eq!(reset_all_projects(&connection).unwrap(), 1);

        assert!(list_projects(&connection).unwrap().is_empty());
        assert!(list_sessions(&connection).unwrap().is_empty());
        assert!(list_tasks(&connection).unwrap().is_empty());
    }

    #[test]
    fn reset_all_tasks_deletes_task_records() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "task-reset".into(),
                workdir: "/tmp/task-reset".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/task-reset.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Reset task".into(),
                }],
            }],
        )
        .unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "task-reset")
            .unwrap()
            .remove(0);
        upsert_task(
            &connection,
            stored.project_id,
            "task-reset",
            "Task Reset",
            "Brief",
            "done",
            "/tmp/task-reset.md",
        )
        .unwrap();

        assert_eq!(reset_all_tasks(&connection).unwrap(), 1);

        assert!(super::list_tasks(&connection).unwrap().is_empty());
    }

    #[test]
    fn update_task_status_rejects_non_discussing_empty_tasks() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "empty-task".into(),
                workdir: "/tmp/empty-task".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/empty-task.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Create empty task".into(),
                }],
            }],
        )
        .unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "empty-task")
            .unwrap()
            .remove(0);
        upsert_task(
            &connection,
            stored.project_id,
            "empty-task",
            "Empty Task",
            "Brief",
            "discussing",
            "/tmp/empty-task.md",
        )
        .unwrap();

        let error = super::update_task_status(&connection, "empty-task", "empty-task", "done")
            .unwrap_err()
            .to_string();
        let task = super::list_tasks(&connection).unwrap().remove(0);

        assert!(error.contains("empty task"));
        assert_eq!(task.status, "discussing");
    }

    #[test]
    fn delete_task_if_empty_deletes_only_zero_session_tasks() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "linked-task".into(),
                    workdir: "/tmp/delete-task".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:00:01Z".into(),
                    raw_path: "/tmp/linked-task.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Linked task".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "empty-task".into(),
                    workdir: "/tmp/delete-task".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:00:01Z".into(),
                    raw_path: "/tmp/empty-task.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Empty task".into(),
                    }],
                },
            ],
        )
        .unwrap();
        let linked = unprocessed_session_by_session_id(&connection, "linked-task")
            .unwrap()
            .remove(0);
        let empty = unprocessed_session_by_session_id(&connection, "empty-task")
            .unwrap()
            .remove(0);
        let (linked_task_id, _) = upsert_task(
            &connection,
            linked.project_id,
            "linked-task",
            "Linked Task",
            "Brief",
            "developing",
            "/tmp/linked-task.md",
        )
        .unwrap();
        upsert_task(
            &connection,
            empty.project_id,
            "empty-task",
            "Empty Task",
            "Brief",
            "discussing",
            "/tmp/empty-task.md",
        )
        .unwrap();
        mark_session_processed(
            &connection,
            linked.id,
            linked_task_id,
            "Linked",
            "Summary",
            "/tmp/linked-session.md",
        )
        .unwrap();

        let linked_error = delete_task_if_empty(&connection, "delete-task", "linked-task")
            .unwrap_err()
            .to_string();
        let deleted = delete_task_if_empty(&connection, "delete-task", "empty-task").unwrap();
        let remaining = super::list_tasks(&connection).unwrap();

        assert!(linked_error.contains("has sessions"));
        assert!(deleted);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].slug, "linked-task");
    }

    #[test]
    fn active_jobs_omits_completed_and_recovers_running_jobs() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let connection = open(&paths).unwrap();
        migrate(&connection).unwrap();

        let first = enqueue_analyze_sessions(&connection, None).unwrap();
        let claimed = claim_next_job(&connection).unwrap().unwrap();
        assert_eq!(claimed.id, first.job_id);
        update_job_progress(&connection, claimed.id, 0, 0, "waiting").unwrap();

        mark_stale_running_jobs_queued(&connection).unwrap();
        let reclaimed = claim_next_job(&connection).unwrap().unwrap();
        assert_eq!(reclaimed.status, "running");

        super::complete_job(&connection, reclaimed.id, "completed").unwrap();
        assert!(list_active_jobs(&connection).unwrap().is_empty());
    }

    #[test]
    fn cancel_job_hides_active_job() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let connection = open(&paths).unwrap();
        migrate(&connection).unwrap();

        let job = enqueue_analyze_sessions(&connection, None).unwrap();

        assert!(cancel_job(&connection, job.job_id).unwrap());
        assert!(list_active_jobs(&connection).unwrap().is_empty());
    }
}
