pub fn list_tasks(connection: &rusqlite::Connection) -> anyhow::Result<Vec<TaskRecord>> {
    let mut statement = connection.prepare(
        r#"
        SELECT p.slug, t.slug, t.title, t.brief, t.status,
               t.summary_path,
               CASE WHEN t.summary_path LIKE '%/description.md' THEN t.summary_path ELSE NULL END AS description_path,
               CASE WHEN t.summary_path LIKE '%/description.md'
                    THEN substr(t.summary_path, 1, length(t.summary_path) - length('/description.md')) || '/session.json'
                    ELSE NULL
               END AS session_path,
               COUNT(s.id) AS session_count,
               t.created_at,
               t.updated_at
        FROM tasks t
        JOIN projects p ON p.id = t.project_id
        LEFT JOIN sessions s ON s.task_id = t.id
        GROUP BY t.id
        ORDER BY t.updated_at DESC, t.title ASC
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        let session_count: i64 = row.get(8)?;
        Ok(TaskRecord {
            project_slug: row.get(0)?,
            slug: row.get(1)?,
            title: row.get(2)?,
            brief: row.get(3)?,
            status: row.get(4)?,
            summary_path: row.get(5)?,
            description_path: row.get(6)?,
            session_path: row.get(7)?,
            session_count: session_count as usize,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
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
        INSERT INTO tasks (project_id, slug, title, brief, status, summary_path, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
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

pub fn update_task_status(
    connection: &rusqlite::Connection,
    project_slug: &str,
    task_slug: &str,
    status: &str,
) -> anyhow::Result<bool> {
    let task_info: Option<(i64, String)> = connection
        .query_row(
            r#"
            SELECT COUNT(s.id), t.summary_path
            FROM tasks t
            JOIN projects p ON p.id = t.project_id
            LEFT JOIN sessions s ON s.task_id = t.id
            WHERE t.slug = ?1 AND p.slug = ?2
            GROUP BY t.id, t.summary_path
            "#,
            params![task_slug, project_slug],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (session_count, summary_path) = task_info.unwrap_or((0, String::new()));
    let saved_agent_task = summary_path.ends_with("/description.md")
        && std::path::Path::new(&summary_path)
            .with_file_name("session.json")
            .exists();
    if session_count == 0 && status != "discussing" && !saved_agent_task {
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

pub fn reset_all_tasks(connection: &rusqlite::Connection) -> anyhow::Result<usize> {
    connection
        .execute("DELETE FROM tasks", [])
        .map_err(Into::into)
}

