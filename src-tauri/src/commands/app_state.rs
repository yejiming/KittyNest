#[tauri::command]
pub fn get_app_state(services: State<'_, AppServices>) -> CommandResult<AppStateDto> {
    get_app_state_inner(&services.paths).map_err(to_command_error)
}

#[tauri::command]
pub fn get_cached_app_state(services: State<'_, AppServices>) -> CommandResult<AppStateDto> {
    get_cached_app_state_inner(&services.paths).map_err(to_command_error)
}

#[tauri::command]
pub fn scan_sources(services: State<'_, AppServices>) -> CommandResult<serde_json::Value> {
    scan_sources_inner(&services.paths).map_err(to_command_error)
}

fn get_app_state_inner(paths: &crate::models::AppPaths) -> anyhow::Result<AppStateDto> {
    get_app_state_with_roots(
        paths,
        home_dir().join(".claude"),
        codex_home_dir().join("sessions"),
    )
}

fn get_cached_app_state_inner(paths: &crate::models::AppPaths) -> anyhow::Result<AppStateDto> {
    get_cached_app_state_with_roots(
        paths,
        home_dir().join(".claude"),
        codex_home_dir().join("sessions"),
    )
}

pub fn get_app_state_with_roots(
    paths: &crate::models::AppPaths,
    claude_root: std::path::PathBuf,
    codex_sessions_root: std::path::PathBuf,
) -> anyhow::Result<AppStateDto> {
    crate::config::initialize_workspace(paths)?;
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    app_state_from_db(paths, &connection, &claude_root, &codex_sessions_root)
}

pub fn get_cached_app_state_with_roots(
    paths: &crate::models::AppPaths,
    claude_root: std::path::PathBuf,
    codex_sessions_root: std::path::PathBuf,
) -> anyhow::Result<AppStateDto> {
    crate::config::initialize_workspace(paths)?;
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    app_state_from_db(paths, &connection, &claude_root, &codex_sessions_root)
}

fn app_state_from_db(
    paths: &crate::models::AppPaths,
    connection: &rusqlite::Connection,
    claude_root: &std::path::Path,
    codex_sessions_root: &std::path::Path,
) -> anyhow::Result<AppStateDto> {
    let mut stats = crate::db::dashboard_stats(connection)?;
    stats.entities = crate::graph::graph_counts(paths)?.entities;

    Ok(AppStateDto {
        data_dir: paths.data_dir.to_string_lossy().to_string(),
        llm_settings: crate::config::read_llm_settings(paths)?,
        llm_provider_calls: crate::db::list_llm_provider_calls(connection)?,
        provider_presets: crate::presets::provider_presets(),
        source_statuses: source_statuses_with_roots(claude_root, codex_sessions_root),
        stats,
        projects: crate::db::list_projects(&connection)?,
        tasks: crate::db::list_tasks(&connection)?,
        sessions: crate::db::list_sessions(&connection)?,
        jobs: crate::db::list_active_jobs(&connection)?,
    })
}

pub(crate) fn scan_sources_inner(
    paths: &crate::models::AppPaths,
) -> anyhow::Result<serde_json::Value> {
    crate::config::initialize_workspace(paths)?;
    let mut connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let codex_root = codex_home_dir().join("sessions");
    let claude_root = home_dir().join(".claude");
    let (codex_found, claude_found, inserted) =
        scan_sources_into_db(paths, &mut connection, &claude_root, &codex_root)?;
    Ok(serde_json::json!({
        "found": codex_found + claude_found,
        "inserted": inserted,
        "codexFound": codex_found,
        "claudeFound": claude_found
    }))
}

fn scan_sources_into_db(
    paths: &crate::models::AppPaths,
    connection: &mut rusqlite::Connection,
    claude_root: &std::path::Path,
    codex_root: &std::path::Path,
) -> anyhow::Result<(usize, usize, usize)> {
    let mut codex_sessions = crate::scanner::scan_codex_sessions(codex_root)?;
    let mut claude_sessions = crate::scanner::scan_claude_sessions(claude_root)?;
    let codex_found = codex_sessions.len();
    let claude_found = claude_sessions.len();
    codex_sessions.append(&mut claude_sessions);
    let removed_sessions = crate::db::delete_sessions_missing_from_scan(
        connection,
        &codex_sessions,
        &["codex", "claude"],
    )?;
    for session in &removed_sessions {
        remove_session_artifacts(paths, session)?;
    }
    let inserted = crate::db::upsert_raw_sessions(connection, &codex_sessions)?;
    remove_deleted_projects(paths, connection)?;
    Ok((codex_found, claude_found, inserted))
}

fn remove_deleted_projects(
    paths: &crate::models::AppPaths,
    connection: &rusqlite::Connection,
) -> anyhow::Result<()> {
    for project in crate::db::list_projects(connection)? {
        let workdir_missing = !std::path::Path::new(&project.workdir).exists();
        let empty_project = crate::db::project_session_count(connection, &project.slug)? == 0;
        if !workdir_missing && !empty_project {
            continue;
        }
        let Some((project_id, _)) = crate::db::get_project_by_slug(connection, &project.slug)?
        else {
            continue;
        };
        let sessions = crate::db::all_project_sessions_by_project_slug(connection, &project.slug)?;
        crate::db::delete_project_cascade(connection, project_id)?;
        for session in &sessions {
            remove_session_artifacts(paths, session)?;
        }
        let project_dir = paths.projects_dir.join(&project.slug);
        if project_dir.exists() {
            std::fs::remove_dir_all(project_dir)?;
        }
    }
    Ok(())
}

pub(crate) fn assistant_project_paths(
    paths: &crate::models::AppPaths,
    project_slug: &str,
) -> anyhow::Result<(std::path::PathBuf, std::path::PathBuf)> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let Some((_project_id, project)) = crate::db::get_project_by_slug(&connection, project_slug)?
    else {
        anyhow::bail!("Project not found: {project_slug}");
    };
    if project.review_status != "reviewed" {
        anyhow::bail!("Task Assistant requires a reviewed project");
    }
    let summary_root = paths.projects_dir.join(project_slug);
    std::fs::create_dir_all(&summary_root)?;
    Ok((std::path::PathBuf::from(project.workdir), summary_root))
}

pub(crate) fn enqueue_save_agent_session_inner(
    paths: &crate::models::AppPaths,
    session_id: &str,
    project_slug: &str,
    timeline: crate::models::AgentTimelinePayload,
    llm_messages: Vec<serde_json::Value>,
) -> anyhow::Result<crate::models::EnqueueJobResult> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let (_, project) = crate::db::get_project_by_slug(&connection, project_slug)?
        .ok_or_else(|| anyhow::anyhow!("Project not found: {project_slug}"))?;
    if project.review_status != "reviewed" {
        anyhow::bail!("Task Assistant requires a reviewed project");
    }
    let job = crate::db::prepare_save_agent_session_job(&connection, session_id, project_slug)?;
    let payload = QueuedAgentSessionSavePayload {
        version: 1,
        session_id: session_id.to_string(),
        project_slug: project.slug,
        timeline,
        llm_messages,
    };
    let payload_path = save_agent_session_payload_path(paths, job.job_id);
    if let Some(parent) = payload_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Err(error) = std::fs::write(&payload_path, serde_json::to_string_pretty(&payload)?) {
        let _ = crate::db::fail_job(&connection, job.job_id, &format!("Assistant session save failed: {error}"));
        return Err(error.into());
    }
    crate::db::queue_prepared_job(&connection, job.job_id)?;
    Ok(job)
}

pub(crate) fn run_save_agent_session_job(
    paths: &crate::models::AppPaths,
    job_id: i64,
    session_id: &str,
    project_slug: &str,
) -> anyhow::Result<crate::models::TaskRecord> {
    run_save_agent_session_job_with_metadata(paths, job_id, session_id, project_slug, |settings, messages| {
        crate::assistant::llm::request_openai_json(settings, messages)
    })
}

fn read_markdown_file_inner(paths: &crate::models::AppPaths, path: &str) -> anyhow::Result<String> {
    let requested = std::path::PathBuf::from(path);
    let canonical = requested.canonicalize()?;
    let allowed_roots = [
        paths.projects_dir.canonicalize()?,
        paths.memories_dir.canonicalize()?,
    ];
    if !allowed_roots.iter().any(|root| canonical.starts_with(root)) {
        anyhow::bail!("Markdown path is outside KittyNest stores");
    }
    Ok(std::fs::read_to_string(canonical)?)
}

fn source_statuses_with_roots(
    claude_root: &std::path::Path,
    codex_root: &std::path::Path,
) -> Vec<SourceStatus> {
    [
        ("Claude Code", claude_root.to_path_buf()),
        ("Codex", codex_root.to_path_buf()),
    ]
    .into_iter()
    .map(|(source, path)| SourceStatus {
        source: source.into(),
        exists: path.exists(),
        path: path.to_string_lossy().to_string(),
    })
    .collect()
}

fn home_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

fn codex_home_dir() -> std::path::PathBuf {
    std::env::var("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".codex"))
}
