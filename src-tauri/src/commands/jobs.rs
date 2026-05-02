#[tauri::command]
pub fn review_project(
    project_slug: String,
    _services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    crate::analysis::review_project(&_services.paths, &project_slug)
        .map(|info_path| serde_json::json!({ "infoPath": info_path }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn import_historical_sessions(
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    enqueue_analyze_sessions(None, services)
}

#[tauri::command]
pub fn enqueue_analyze_sessions(
    updated_after: Option<String>,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    crate::db::migrate(&connection).map_err(to_command_error)?;
    crate::db::enqueue_analyze_sessions(&connection, updated_after.as_deref())
        .map(|result| serde_json::json!({ "jobId": result.job_id, "total": result.total }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn enqueue_scan_sources(services: State<'_, AppServices>) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    crate::db::migrate(&connection).map_err(to_command_error)?;
    crate::db::enqueue_scan_sources(&connection)
        .map(|result| serde_json::json!({ "jobId": result.job_id, "total": result.total }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn enqueue_analyze_project_sessions(
    project_slug: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    crate::db::migrate(&connection).map_err(to_command_error)?;
    crate::db::enqueue_analyze_project_sessions(&connection, &project_slug)
        .map(|result| serde_json::json!({ "jobId": result.job_id, "total": result.total }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn enqueue_analyze_project(
    project_slug: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    crate::db::migrate(&connection).map_err(to_command_error)?;
    crate::db::enqueue_analyze_project(&connection, &project_slug)
        .map(|result| serde_json::json!({ "jobId": result.job_id, "total": result.total }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn enqueue_analyze_session(
    session_id: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    crate::db::migrate(&connection).map_err(to_command_error)?;
    crate::db::enqueue_analyze_session(&connection, &session_id)
        .map(|result| serde_json::json!({ "jobId": result.job_id, "total": result.total }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn enqueue_review_project(
    project_slug: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    crate::db::migrate(&connection).map_err(to_command_error)?;
    crate::db::enqueue_review_project(&connection, &project_slug)
        .map(|result| serde_json::json!({ "jobId": result.job_id, "total": result.total }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn enqueue_rebuild_memories(
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    crate::db::migrate(&connection).map_err(to_command_error)?;
    crate::db::enqueue_rebuild_memories(&connection)
        .map(|result| serde_json::json!({ "jobId": result.job_id, "total": result.total }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn enqueue_search_memories(
    query: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    crate::db::migrate(&connection).map_err(to_command_error)?;
    crate::db::enqueue_search_memories(&connection, &query)
        .map(|result| serde_json::json!({ "jobId": result.job_id, "total": result.total }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn get_memory_search(services: State<'_, AppServices>) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    crate::db::migrate(&connection).map_err(to_command_error)?;
    crate::db::latest_memory_search(&connection)
        .map(|search| serde_json::json!({ "search": search }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn get_session_memory(
    session_id: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    get_session_memory_inner(&services.paths, &session_id)
        .map(|detail| serde_json::to_value(detail).unwrap_or_else(|_| serde_json::json!({})))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn list_memory_entities(services: State<'_, AppServices>) -> CommandResult<serde_json::Value> {
    crate::graph::entity_session_counts(&services.paths)
        .map(|entities| serde_json::json!({ "entities": entities }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn list_entity_sessions(
    entity: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    list_entity_sessions_inner(&services.paths, &entity)
        .map(|sessions| serde_json::json!({ "sessions": sessions }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn get_active_jobs(services: State<'_, AppServices>) -> CommandResult<serde_json::Value> {
    get_active_jobs_inner(&services.paths)
        .map(|jobs| serde_json::json!({ "jobs": jobs }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn stop_job(job_id: i64, services: State<'_, AppServices>) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    crate::db::migrate(&connection).map_err(to_command_error)?;
    crate::db::cancel_job(&connection, job_id)
        .map(|stopped| serde_json::json!({ "stopped": stopped }))
        .map_err(to_command_error)
}

fn get_active_jobs_inner(
    paths: &crate::models::AppPaths,
) -> anyhow::Result<Vec<crate::models::JobRecord>> {
    crate::config::initialize_workspace(paths)?;
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    crate::db::list_active_jobs(&connection)
}

fn get_session_memory_inner(
    paths: &crate::models::AppPaths,
    session_id: &str,
) -> anyhow::Result<crate::models::SessionMemoryDetail> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let memory_path = paths
        .memories_dir
        .join("sessions")
        .join(crate::utils::slugify_lower(session_id))
        .join("memory.md");
    let mut memories = crate::db::session_memories_by_session_id(&connection, session_id)?;
    if memories.is_empty() {
        memories = read_memory_lines(&memory_path)?;
    }
    let related = crate::graph::related_sessions_for_session(paths, session_id)?;
    Ok(crate::models::SessionMemoryDetail {
        session_id: session_id.to_string(),
        memory_path: memory_path.to_string_lossy().to_string(),
        memories,
        related_sessions: hydrate_related_sessions(paths, related)?,
    })
}

fn list_entity_sessions_inner(
    paths: &crate::models::AppPaths,
    entity: &str,
) -> anyhow::Result<Vec<crate::models::MemoryRelatedSession>> {
    let related = crate::graph::related_sessions_for_entity(paths, entity)?;
    hydrate_related_sessions(paths, related)
}

fn hydrate_related_sessions(
    paths: &crate::models::AppPaths,
    related: Vec<crate::graph::RelatedSession>,
) -> anyhow::Result<Vec<crate::models::MemoryRelatedSession>> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let session_ids = related
        .iter()
        .map(|session| session.session_id.clone())
        .collect::<Vec<_>>();
    let titles = crate::db::sessions_by_session_ids(&connection, &session_ids)?
        .into_iter()
        .map(|session| {
            (
                session.session_id,
                session.title.unwrap_or_else(|| session.raw_path),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    Ok(related
        .into_iter()
        .map(|session| crate::models::MemoryRelatedSession {
            title: titles
                .get(&session.session_id)
                .cloned()
                .unwrap_or_else(|| session.session_id.clone()),
            session_id: session.session_id,
            project_slug: session.project_slug,
            shared_entities: session.shared_entities,
        })
        .collect())
}

#[tauri::command]
pub fn detect_obsidian_vaults() -> CommandResult<serde_json::Value> {
    let vaults = crate::sync::obsidian::detect_vaults();
    Ok(serde_json::json!({ "vaults": vaults }))
}

#[tauri::command]
pub fn sync_to_obsidian(
    mode: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let config = crate::config::read_obsidian_config(&services.paths)
        .map_err(to_command_error)?;

    let result = crate::sync::run_sync(&services.paths, &config, &mode)
        .map_err(to_command_error)?;

    Ok(serde_json::json!({ "result": result }))
}

#[tauri::command]
pub fn get_sync_status(
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let config = crate::config::read_obsidian_config(&services.paths)
        .map_err(to_command_error)?;

    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    let kind_counts = crate::sync::state::count_by_kind(&connection)
        .map_err(to_command_error)?;

    let status = crate::models::SyncStatus {
        vault_path: config.vault_path,
        auto_sync: config.auto_sync,
        delete_removed: config.delete_removed,
        last_sync_at: None,
        total_synced: kind_counts.projects
            + kind_counts.sessions
            + kind_counts.tasks
            + kind_counts.memories
            + kind_counts.entities,
        kind_counts,
    };

    Ok(serde_json::json!({ "status": status }))
}

#[tauri::command]
pub fn enqueue_sync_to_obsidian_cmd(
    mode: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    let job_id = crate::db::enqueue_sync_to_obsidian(&connection, &mode)
        .map_err(to_command_error)?;
    Ok(serde_json::json!({ "jobId": job_id }))
}

#[tauri::command]
pub fn save_obsidian_config(
    vault_path: Option<String>,
    auto_sync: bool,
    delete_removed: bool,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let config = crate::models::ObsidianConfig {
        vault_path,
        auto_sync,
        delete_removed,
    };
    crate::config::write_obsidian_config(&services.paths, &config)
        .map_err(to_command_error)?;
    Ok(serde_json::json!({ "saved": true }))
}
