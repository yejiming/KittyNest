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
        SELECT slug, display_title, workdir, sources, info_path, progress_path, user_preference_path,
               agents_path, review_status, last_reviewed_at, last_session_at
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
            user_preference_path: row.get(6)?,
            agents_path: row.get(7)?,
            review_status: row.get(8)?,
            last_reviewed_at: row.get(9)?,
            last_session_at: row.get(10)?,
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
            SELECT id, slug, display_title, workdir, sources, info_path, progress_path, user_preference_path,
                   agents_path, review_status, last_reviewed_at, last_session_at
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
                        user_preference_path: row.get(7)?,
                        agents_path: row.get(8)?,
                        review_status: row.get(9)?,
                        last_reviewed_at: row.get(10)?,
                        last_session_at: row.get(11)?,
                    },
                ))
            },
        )
        .optional()
        .map_err(Into::into)
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

pub fn update_project_user_preference(
    connection: &rusqlite::Connection,
    project_slug: &str,
    user_preference_path: &str,
) -> anyhow::Result<()> {
    connection.execute(
        "UPDATE projects SET user_preference_path = ?1, updated_at = ?2 WHERE slug = ?3",
        params![
            user_preference_path,
            crate::utils::now_rfc3339(),
            project_slug
        ],
    )?;
    Ok(())
}

pub fn update_project_agents(
    connection: &rusqlite::Connection,
    project_slug: &str,
    agents_path: &str,
) -> anyhow::Result<()> {
    connection.execute(
        "UPDATE projects SET agents_path = ?1, updated_at = ?2 WHERE slug = ?3",
        params![agents_path, crate::utils::now_rfc3339(), project_slug],
    )?;
    Ok(())
}

pub fn reset_all_projects(connection: &rusqlite::Connection) -> anyhow::Result<usize> {
    connection.execute("DELETE FROM sessions", [])?;
    connection.execute("DELETE FROM tasks", [])?;
    connection
        .execute("DELETE FROM projects", [])
        .map_err(Into::into)
}

pub fn delete_project_cascade(
    connection: &rusqlite::Connection,
    project_id: i64,
) -> anyhow::Result<usize> {
    connection.execute(
        r#"
        DELETE FROM session_memories
        WHERE session_row_id IN (SELECT id FROM sessions WHERE project_id = ?1)
        "#,
        params![project_id],
    )?;
    connection.execute("DELETE FROM sessions WHERE project_id = ?1", params![project_id])?;
    connection.execute("DELETE FROM tasks WHERE project_id = ?1", params![project_id])?;
    connection
        .execute("DELETE FROM projects WHERE id = ?1", params![project_id])
        .map_err(Into::into)
}

