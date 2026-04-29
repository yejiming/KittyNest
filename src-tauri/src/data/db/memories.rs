pub fn replace_session_memories(
    connection: &rusqlite::Connection,
    session: &StoredSession,
    memories: &[String],
) -> anyhow::Result<usize> {
    replace_session_memories_at(connection, session, memories, &crate::utils::now_rfc3339())
}

pub fn replace_session_memories_at(
    connection: &rusqlite::Connection,
    session: &StoredSession,
    memories: &[String],
    created_at: &str,
) -> anyhow::Result<usize> {
    connection.execute(
        "DELETE FROM session_memories WHERE session_row_id = ?1",
        params![session.id],
    )?;
    let mut inserted = 0usize;
    for (index, memory) in memories
        .iter()
        .map(|memory| memory.trim())
        .filter(|memory| !memory.is_empty())
        .enumerate()
    {
        connection.execute(
            r#"
            INSERT INTO session_memories
              (session_row_id, source_session, project_slug, memory, ordinal, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                session.id,
                session.session_id.as_str(),
                session.project_slug.as_str(),
                memory,
                index as i64,
                created_at
            ],
        )?;
        inserted += 1;
    }
    Ok(inserted)
}

pub fn delete_session_memories(
    connection: &rusqlite::Connection,
    session: &StoredSession,
) -> anyhow::Result<usize> {
    connection
        .execute(
            "DELETE FROM session_memories WHERE session_row_id = ?1 OR source_session = ?2",
            params![session.id, session.session_id.as_str()],
        )
        .map_err(Into::into)
}

pub fn session_memories_by_session_id(
    connection: &rusqlite::Connection,
    session_id: &str,
) -> anyhow::Result<Vec<String>> {
    let mut statement = connection.prepare(
        r#"
        SELECT memory
        FROM session_memories
        WHERE source_session = ?1
        ORDER BY ordinal ASC, id ASC
        "#,
    )?;
    let rows = statement.query_map(params![session_id], |row| row.get::<_, String>(0))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn session_memories_for_sessions(
    connection: &rusqlite::Connection,
    session_ids: &[String],
) -> anyhow::Result<Vec<MemorySearchResultRecord>> {
    let mut records = Vec::new();
    let mut statement = connection.prepare(
        r#"
        SELECT m.source_session, COALESCE(s.title, m.source_session), m.project_slug, m.memory, m.ordinal
        FROM session_memories m
        LEFT JOIN sessions s ON s.id = m.session_row_id
        WHERE m.source_session = ?1
        ORDER BY m.source_session ASC, m.ordinal ASC, m.id ASC
        "#,
    )?;
    for session_id in session_ids {
        let rows = statement.query_map(params![session_id], |row| {
            let ordinal: i64 = row.get(4)?;
            Ok(MemorySearchResultRecord {
                source_session: row.get(0)?,
                session_title: row.get(1)?,
                project_slug: row.get(2)?,
                memory: row.get(3)?,
                ordinal: ordinal as usize,
            })
        })?;
        for row in rows {
            records.push(row?);
        }
    }
    Ok(records)
}

pub fn create_memory_search(
    connection: &rusqlite::Connection,
    job_id: i64,
    query: &str,
) -> anyhow::Result<i64> {
    let now = crate::utils::now_rfc3339();
    connection.execute(
        r#"
        INSERT INTO memory_searches (job_id, query, status, message, created_at, updated_at)
        VALUES (?1, ?2, 'queued', 'Queued for analysis', ?3, ?3)
        "#,
        params![job_id, query.trim(), now],
    )?;
    Ok(connection.last_insert_rowid())
}

pub fn memory_search_for_job(
    connection: &rusqlite::Connection,
    job_id: i64,
) -> anyhow::Result<Option<MemorySearchRecord>> {
    memory_search_for_where(connection, "job_id = ?1", params![job_id])
}

pub fn latest_memory_search(
    connection: &rusqlite::Connection,
) -> anyhow::Result<Option<MemorySearchRecord>> {
    memory_search_for_where(connection, "1 = 1", [])
}

fn memory_search_for_where<P>(
    connection: &rusqlite::Connection,
    where_sql: &str,
    params: P,
) -> anyhow::Result<Option<MemorySearchRecord>>
where
    P: rusqlite::Params,
{
    let sql = format!(
        r#"
        SELECT id, job_id, query, status, message, created_at, updated_at
        FROM memory_searches
        WHERE {where_sql}
        ORDER BY updated_at DESC, id DESC
        LIMIT 1
        "#
    );
    let Some(mut search) = connection
        .query_row(&sql, params, |row| {
            Ok(MemorySearchRecord {
                id: row.get(0)?,
                job_id: row.get(1)?,
                query: row.get(2)?,
                status: row.get(3)?,
                message: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                results: Vec::new(),
            })
        })
        .optional()?
    else {
        return Ok(None);
    };
    search.results = memory_search_results(connection, search.id)?;
    Ok(Some(search))
}

fn memory_search_results(
    connection: &rusqlite::Connection,
    search_id: i64,
) -> anyhow::Result<Vec<MemorySearchResultRecord>> {
    let mut statement = connection.prepare(
        r#"
        SELECT source_session, session_title, project_slug, memory, ordinal
        FROM memory_search_results
        WHERE search_id = ?1
        ORDER BY ordinal ASC, id ASC
        "#,
    )?;
    let rows = statement.query_map(params![search_id], |row| {
        let ordinal: i64 = row.get(4)?;
        Ok(MemorySearchResultRecord {
            source_session: row.get(0)?,
            session_title: row.get(1)?,
            project_slug: row.get(2)?,
            memory: row.get(3)?,
            ordinal: ordinal as usize,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn replace_memory_search_results(
    connection: &rusqlite::Connection,
    search_id: i64,
    status: &str,
    message: &str,
    results: &[MemorySearchResultRecord],
) -> anyhow::Result<()> {
    connection.execute(
        "DELETE FROM memory_search_results WHERE search_id = ?1",
        params![search_id],
    )?;
    for (index, result) in results.iter().enumerate() {
        connection.execute(
            r#"
            INSERT INTO memory_search_results
              (search_id, source_session, session_title, project_slug, memory, ordinal)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                search_id,
                result.source_session.as_str(),
                result.session_title.as_str(),
                result.project_slug.as_str(),
                result.memory.as_str(),
                index as i64,
            ],
        )?;
    }
    connection.execute(
        r#"
        UPDATE memory_searches
        SET status = ?1, message = ?2, updated_at = ?3
        WHERE id = ?4
        "#,
        params![status, message, crate::utils::now_rfc3339(), search_id],
    )?;
    Ok(())
}

pub fn reset_all_memories(connection: &rusqlite::Connection) -> anyhow::Result<usize> {
    connection
        .execute("DELETE FROM session_memories", [])
        .map_err(Into::into)
}

fn count(connection: &rusqlite::Connection, sql: &str) -> anyhow::Result<usize> {
    let value: i64 = connection.query_row(sql, [], |row| row.get(0))?;
    Ok(value as usize)
}

