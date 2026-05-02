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
          user_preference_path TEXT,
          agents_path TEXT,
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
          created_at TEXT NOT NULL DEFAULT '',
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

        CREATE TABLE IF NOT EXISTS session_memories (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          session_row_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
          source_session TEXT NOT NULL,
          project_slug TEXT NOT NULL,
          memory TEXT NOT NULL,
          ordinal INTEGER NOT NULL,
          created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS memory_searches (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          job_id INTEGER NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
          query TEXT NOT NULL,
          status TEXT NOT NULL,
          message TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS memory_search_results (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          search_id INTEGER NOT NULL REFERENCES memory_searches(id) ON DELETE CASCADE,
          source_session TEXT NOT NULL,
          session_title TEXT NOT NULL,
          project_slug TEXT NOT NULL,
          memory TEXT NOT NULL,
          ordinal INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS llm_provider_calls (
          provider TEXT PRIMARY KEY,
          calls INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS sync_state (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          kind TEXT NOT NULL,
          source_id TEXT NOT NULL,
          content_hash TEXT NOT NULL,
          last_synced_at TEXT NOT NULL,
          obsidian_path TEXT NOT NULL,
          UNIQUE(kind, source_id)
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
    add_column_if_missing(
        connection,
        "projects",
        "user_preference_path",
        "user_preference_path TEXT",
    )?;
    add_column_if_missing(connection, "projects", "agents_path", "agents_path TEXT")?;
    add_column_if_missing(
        connection,
        "tasks",
        "created_at",
        "created_at TEXT NOT NULL DEFAULT ''",
    )?;
    connection.execute(
        "UPDATE tasks SET created_at = updated_at WHERE created_at = ''",
        [],
    )?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_session_memories_source_session ON session_memories(source_session, ordinal)",
        [],
    )?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_memory_searches_job_id ON memory_searches(job_id)",
        [],
    )?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_memory_search_results_search_id ON memory_search_results(search_id, ordinal)",
        [],
    )?;
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
