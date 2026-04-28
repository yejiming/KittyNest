use std::sync::OnceLock;

use tauri::{Emitter, State};

use crate::{
    errors::{to_command_error, CommandResult},
    models::{AppStateDto, LlmSettings, SourceStatus},
    services::AppServices,
};

#[derive(Clone)]
pub struct TauriAgentEmitter {
    app: tauri::AppHandle,
}

impl crate::assistant::AgentEventEmitter for TauriAgentEmitter {
    fn emit(&self, event: &crate::assistant::AgentEvent) {
        let _ = self.app.emit("agent://event", event);
    }
}

fn assistant_registry(app: tauri::AppHandle) -> &'static crate::assistant::AgentRegistry<TauriAgentEmitter> {
    static REGISTRY: OnceLock<crate::assistant::AgentRegistry<TauriAgentEmitter>> = OnceLock::new();
    REGISTRY.get_or_init(|| crate::assistant::AgentRegistry::new(TauriAgentEmitter { app }))
}

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

#[tauri::command]
pub fn start_agent_run(
    app: tauri::AppHandle,
    session_id: String,
    project_slug: String,
    message: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let project_root =
        assistant_project_root(&services.paths, &project_slug).map_err(to_command_error)?;
    let settings = crate::config::resolve_llm_settings(
        &crate::config::read_llm_settings(&services.paths).map_err(to_command_error)?,
        crate::config::LlmScenario::Assistant,
    );
    assistant_registry(app).start_run(session_id, project_root, settings, message);
    Ok(serde_json::json!({"started": true}))
}

#[tauri::command]
pub fn stop_agent_run(
    app: tauri::AppHandle,
    session_id: String,
    _services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let stopped = assistant_registry(app).stop_run(&session_id);
    Ok(serde_json::json!({ "stopped": stopped }))
}

#[tauri::command]
pub fn resolve_agent_permission(
    app: tauri::AppHandle,
    session_id: String,
    request_id: String,
    value: String,
    supplemental_info: String,
    _services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let resolved = assistant_registry(app).resolve_permission(
        &session_id,
        &request_id,
        &value,
        &supplemental_info,
    );
    Ok(serde_json::json!({ "resolved": resolved }))
}

#[tauri::command]
pub fn resolve_agent_ask_user(
    app: tauri::AppHandle,
    session_id: String,
    request_id: String,
    answers: serde_json::Value,
    _services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let resolved = assistant_registry(app).resolve_ask_user(&session_id, &request_id, answers);
    Ok(serde_json::json!({ "resolved": resolved }))
}

#[tauri::command]
pub fn read_markdown_file(
    path: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    read_markdown_file_inner(&services.paths, &path)
        .map(|content| serde_json::json!({ "content": content }))
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

#[tauri::command]
pub fn save_llm_settings(
    settings: LlmSettings,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    crate::config::write_llm_settings(&services.paths, &settings)
        .map(|_| serde_json::json!({"saved": true}))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn update_task_status(
    project_slug: String,
    task_slug: String,
    status: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let connection = crate::db::open(&services.paths).map_err(to_command_error)?;
    crate::db::migrate(&connection).map_err(to_command_error)?;
    crate::db::update_task_status(&connection, &project_slug, &task_slug, &status)
        .map(|updated| serde_json::json!({ "updated": updated }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn create_task(
    project_slug: String,
    user_prompt: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    crate::analysis::create_manual_task(&services.paths, &project_slug, &user_prompt)
        .map(|result| serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({})))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn delete_task(
    project_slug: String,
    task_slug: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    delete_task_inner(&services.paths, &project_slug, &task_slug)
        .map(|deleted| serde_json::json!({ "deleted": deleted }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn reset_sessions(services: State<'_, AppServices>) -> CommandResult<serde_json::Value> {
    reset_sessions_inner(&services.paths)
        .map(|reset| serde_json::json!({ "reset": reset }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn reset_projects(services: State<'_, AppServices>) -> CommandResult<serde_json::Value> {
    reset_projects_inner(&services.paths)
        .map(|reset| serde_json::json!({ "reset": reset }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn reset_tasks(services: State<'_, AppServices>) -> CommandResult<serde_json::Value> {
    reset_tasks_inner(&services.paths)
        .map(|reset| serde_json::json!({ "reset": reset }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn reset_memories(services: State<'_, AppServices>) -> CommandResult<serde_json::Value> {
    reset_memories_inner(&services.paths)
        .map(|reset| serde_json::json!({ "reset": reset }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn rebuild_memories(services: State<'_, AppServices>) -> CommandResult<serde_json::Value> {
    enqueue_rebuild_memories(services)
}

pub(crate) fn delete_task_inner(
    paths: &crate::models::AppPaths,
    project_slug: &str,
    task_slug: &str,
) -> anyhow::Result<bool> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let deleted = crate::db::delete_task_if_empty(&connection, project_slug, task_slug)?;
    if deleted {
        let task_dir = paths
            .projects_dir
            .join(project_slug)
            .join("tasks")
            .join(task_slug);
        if task_dir.exists() {
            std::fs::remove_dir_all(task_dir)?;
        }
    }
    Ok(deleted)
}

pub(crate) fn reset_sessions_inner(paths: &crate::models::AppPaths) -> anyhow::Result<usize> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let reset = crate::db::reset_all_sessions(&connection)?;
    remove_project_child_dirs(paths, "sessions")?;
    Ok(reset)
}

pub(crate) fn reset_projects_inner(paths: &crate::models::AppPaths) -> anyhow::Result<usize> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let reset = crate::db::reset_all_projects(&connection)?;
    for project_dir in project_dirs(paths)? {
        std::fs::remove_dir_all(project_dir)?;
    }
    Ok(reset)
}

pub(crate) fn reset_tasks_inner(paths: &crate::models::AppPaths) -> anyhow::Result<usize> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let reset = crate::db::reset_all_tasks(&connection)?;
    remove_project_child_dirs(paths, "tasks")?;
    Ok(reset)
}

pub(crate) fn reset_memories_inner(paths: &crate::models::AppPaths) -> anyhow::Result<usize> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let reset = crate::db::reset_all_memories(&connection)?;
    let session_memories_dir = paths.memories_dir.join("sessions");
    if session_memories_dir.exists() {
        std::fs::remove_dir_all(session_memories_dir)?;
    }
    crate::graph::reset_graph(paths)?;
    Ok(reset)
}

fn remove_project_child_dirs(paths: &crate::models::AppPaths, child: &str) -> anyhow::Result<()> {
    for project_dir in project_dirs(paths)? {
        let child_dir = project_dir.join(child);
        if child_dir.exists() {
            std::fs::remove_dir_all(child_dir)?;
        }
    }
    Ok(())
}

fn project_dirs(paths: &crate::models::AppPaths) -> anyhow::Result<Vec<std::path::PathBuf>> {
    if !paths.projects_dir.exists() {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(&paths.projects_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            dirs.push(entry.path());
        }
    }
    Ok(dirs)
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
    let mut connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    scan_sources_into_db(&mut connection, &claude_root, &codex_sessions_root)?;
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
    Ok(AppStateDto {
        data_dir: paths.data_dir.to_string_lossy().to_string(),
        llm_settings: crate::config::read_llm_settings(paths)?,
        llm_provider_calls: crate::db::list_llm_provider_calls(connection)?,
        provider_presets: crate::presets::provider_presets(),
        source_statuses: source_statuses_with_roots(claude_root, codex_sessions_root),
        stats: crate::db::dashboard_stats(&connection)?,
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
        scan_sources_into_db(&mut connection, &claude_root, &codex_root)?;
    Ok(serde_json::json!({
        "found": codex_found + claude_found,
        "inserted": inserted,
        "codexFound": codex_found,
        "claudeFound": claude_found
    }))
}

fn scan_sources_into_db(
    connection: &mut rusqlite::Connection,
    claude_root: &std::path::Path,
    codex_root: &std::path::Path,
) -> anyhow::Result<(usize, usize, usize)> {
    let mut codex_sessions = crate::scanner::scan_codex_sessions(codex_root)?;
    let mut claude_sessions = crate::scanner::scan_claude_sessions(claude_root)?;
    let codex_found = codex_sessions.len();
    let claude_found = claude_sessions.len();
    codex_sessions.append(&mut claude_sessions);
    let inserted = crate::db::upsert_raw_sessions(connection, &codex_sessions)?;
    Ok((codex_found, claude_found, inserted))
}

pub(crate) fn assistant_project_root(
    paths: &crate::models::AppPaths,
    project_slug: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let Some((_project_id, project)) = crate::db::get_project_by_slug(&connection, project_slug)?
    else {
        anyhow::bail!("Project not found: {project_slug}");
    };
    if project.review_status != "reviewed" {
        anyhow::bail!("Task Assistant requires a reviewed project");
    }
    Ok(std::path::PathBuf::from(project.workdir))
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
    let titles = crate::db::list_sessions(&connection)?
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

fn read_memory_lines(path: &std::path::Path) -> anyhow::Result<Vec<String>> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string)
            .collect()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
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

#[cfg(test)]
mod tests {
    use super::{
        get_app_state_with_roots, get_cached_app_state_with_roots, read_markdown_file_inner,
        reset_memories_inner, reset_projects_inner, reset_sessions_inner, reset_tasks_inner,
    };
    use crate::{
        memory::MemoryEntity,
        models::{AppPaths, RawMessage, RawSession},
    };

    #[test]
    fn get_app_state_discovers_existing_claude_and_codex_sessions_on_load() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));

        let claude_project_dir = temp.path().join("claude/projects/project-a");
        std::fs::create_dir_all(&claude_project_dir).unwrap();
        std::fs::write(
            claude_project_dir.join("claude-1.jsonl"),
            r#"{"uuid":"claude-1","timestamp":"2026-04-26T01:59:00Z","type":"summary","summary":"ignored"}"#
                .to_owned()
                + "\n"
                + r#"{"uuid":"claude-1","timestamp":"2026-04-26T02:00:00Z","type":"user","cwd":"/Users/kc/ClaudeProject","message":{"role":"user","content":"Find sessions"}}"#
                + "\n"
                + r#"{"uuid":"claude-1","timestamp":"2026-04-26T02:01:00Z","type":"assistant","message":{"role":"assistant","content":"Found"}}"#,
        )
        .unwrap();

        let codex_sessions_dir = temp.path().join("codex/sessions");
        std::fs::create_dir_all(&codex_sessions_dir).unwrap();
        std::fs::write(
            codex_sessions_dir.join("codex-1.jsonl"),
            r#"{"type":"session_meta","payload":{"id":"codex-1","cwd":"/Users/kc/CodexProject","timestamp":"2026-04-26T03:00:00Z"}}"#
                .to_owned()
                + "\n"
                + r#"{"id":"codex-1","timestamp":"2026-04-26T03:01:00Z","message":{"role":"user","content":"Scan Codex"}}"#
                + "\n"
                + r#"{"id":"codex-1","timestamp":"2026-04-26T03:02:00Z","message":{"role":"assistant","content":"Scanned"}}"#,
        )
        .unwrap();

        let state =
            get_app_state_with_roots(&paths, temp.path().join("claude"), codex_sessions_dir)
                .unwrap();

        assert_eq!(state.stats.sessions, 2);
        assert_eq!(state.stats.active_projects, 2);
        assert!(state
            .projects
            .iter()
            .any(|project| project.slug == "ClaudeProject"));
        assert!(state
            .projects
            .iter()
            .any(|project| project.slug == "CodexProject"));
    }

    #[test]
    fn assistant_project_root_requires_reviewed_project() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let mut connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        let project_dir = temp.path().join("app");
        std::fs::create_dir_all(&project_dir).unwrap();
        crate::db::upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "assistant-project-root".into(),
                workdir: project_dir.to_string_lossy().to_string(),
                created_at: "2026-04-28T00:00:00Z".into(),
                updated_at: "2026-04-28T00:00:01Z".into(),
                raw_path: temp.path().join("session.jsonl").to_string_lossy().to_string(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "hello".into(),
                }],
            }],
        )
        .unwrap();

        let error = super::assistant_project_root(&paths, "app")
            .unwrap_err()
            .to_string();

        assert!(error.contains("reviewed"));
    }

    #[test]
    fn get_app_state_includes_active_jobs() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::enqueue_analyze_sessions(&connection, None).unwrap();

        let state = get_app_state_with_roots(
            &paths,
            temp.path().join("claude"),
            temp.path().join("codex"),
        )
        .unwrap();

        assert_eq!(state.jobs.len(), 1);
        assert_eq!(state.jobs[0].status, "queued");
    }

    #[test]
    fn get_cached_app_state_does_not_scan_sources() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let codex_sessions_dir = temp.path().join("codex/sessions");
        std::fs::create_dir_all(&codex_sessions_dir).unwrap();
        std::fs::write(
            codex_sessions_dir.join("codex-cached.jsonl"),
            r#"{"type":"session_meta","payload":{"id":"codex-cached","cwd":"/Users/kc/Cached","timestamp":"2026-04-26T03:00:00Z"}}"#
                .to_owned()
                + "\n"
                + r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Do not scan"}]}}"#,
        )
        .unwrap();

        let cached = get_cached_app_state_with_roots(
            &paths,
            temp.path().join("claude"),
            codex_sessions_dir.clone(),
        )
        .unwrap();
        let scanned =
            get_app_state_with_roots(&paths, temp.path().join("claude"), codex_sessions_dir)
                .unwrap();

        assert_eq!(cached.stats.sessions, 0);
        assert_eq!(scanned.stats.sessions, 1);
    }

    #[test]
    fn read_markdown_file_rejects_paths_outside_project_store() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let outside = temp.path().join("outside.md");
        std::fs::write(&outside, "# Outside").unwrap();

        let result = read_markdown_file_inner(&paths, &outside.to_string_lossy());

        assert!(result.is_err());
    }

    #[test]
    fn read_markdown_file_allows_memory_store_paths() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let memory_dir = paths.memories_dir.join("sessions/session-1");
        std::fs::create_dir_all(&memory_dir).unwrap();
        let memory_path = memory_dir.join("memory.md");
        std::fs::write(&memory_path, "memory line\n").unwrap();

        let content = read_markdown_file_inner(&paths, &memory_path.to_string_lossy()).unwrap();

        assert_eq!(content, "memory line\n");
    }

    #[test]
    fn session_memory_detail_includes_path_lines_and_related_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session = seed_command_session_with_memory(
            &paths,
            "detail-session",
            "MemoryProject",
            "SQLite memory",
        );
        crate::graph::write_session_graph(
            &paths,
            &session,
            &[crate::memory::MemoryEntity {
                name: "SQLite".into(),
                entity_type: "technology".into(),
            }],
        )
        .unwrap();

        let detail = super::get_session_memory_inner(&paths, "detail-session").unwrap();

        assert!(detail
            .memory_path
            .ends_with("memories/sessions/detail-session/memory.md"));
        assert_eq!(detail.memories, vec!["SQLite memory".to_string()]);
    }

    #[test]
    fn reset_tasks_inner_deletes_task_directories() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let task_dir = paths.projects_dir.join("KittyNest/tasks/session-ingest");
        std::fs::create_dir_all(&task_dir).unwrap();
        std::fs::write(task_dir.join("summary.md"), "{}").unwrap();

        reset_tasks_inner(&paths).unwrap();

        assert!(!task_dir.exists());
    }

    #[test]
    fn reset_sessions_inner_deletes_session_directories() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let session_dir = paths.projects_dir.join("KittyNest/sessions/session-1");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(session_dir.join("summary.md"), "# Session").unwrap();

        reset_sessions_inner(&paths).unwrap();

        assert!(!session_dir.exists());
    }

    #[test]
    fn reset_memories_inner_deletes_memory_files_records_and_graph() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let mut connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "reset-memory".into(),
                workdir: "/tmp/reset-memory".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/reset-memory.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Remember reset".into(),
                }],
            }],
        )
        .unwrap();
        let stored = crate::db::unprocessed_session_by_session_id(&connection, "reset-memory")
            .unwrap()
            .remove(0);
        crate::db::mark_session_processed_with_optional_task_at(
            &connection,
            stored.id,
            None,
            "Reset Memory",
            "Summary",
            "/tmp/reset-memory/summary.md",
            "2026-04-27T10:00:00Z",
        )
        .unwrap();
        crate::db::replace_session_memories(
            &connection,
            &stored,
            &["memory to delete".to_string()],
        )
        .unwrap();
        crate::graph::write_session_graph(
            &paths,
            &stored,
            &[MemoryEntity {
                name: "Memory".into(),
                entity_type: "concept".into(),
            }],
        )
        .unwrap();
        let memory_dir = paths.memories_dir.join("sessions/reset-memory");
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("memory.md"), "memory to delete\n").unwrap();

        let reset = reset_memories_inner(&paths).unwrap();

        assert_eq!(reset, 1);
        assert_eq!(
            crate::db::enqueue_rebuild_memories(&connection)
                .unwrap()
                .total,
            2
        );
        assert!(!paths.memories_dir.join("sessions").exists());
        assert!(
            crate::db::session_memories_by_session_id(&connection, "reset-memory")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            crate::graph::graph_counts(&paths).unwrap(),
            crate::graph::GraphCounts { entities: 0 }
        );
    }

    #[test]
    fn reset_projects_inner_deletes_all_project_directories() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let project_dir = paths.projects_dir.join("KittyNest");
        std::fs::create_dir_all(project_dir.join("tasks/session-ingest")).unwrap();
        std::fs::write(project_dir.join("summary.md"), "# Summary").unwrap();
        std::fs::write(project_dir.join("progress.md"), "# Progress").unwrap();
        std::fs::write(
            project_dir.join("tasks/session-ingest/summary.md"),
            "# Task",
        )
        .unwrap();

        reset_projects_inner(&paths).unwrap();

        assert!(!project_dir.exists());
    }

    fn seed_command_session_with_memory(
        paths: &AppPaths,
        session_id: &str,
        project_slug: &str,
        memory: &str,
    ) -> crate::models::StoredSession {
        crate::config::initialize_workspace(paths).unwrap();
        let mut connection = crate::db::open(paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: session_id.into(),
                workdir: format!("/Users/kc/{project_slug}"),
                created_at: "2026-04-27T00:00:00Z".into(),
                updated_at: "2026-04-27T00:00:01Z".into(),
                raw_path: format!("/tmp/{session_id}.jsonl"),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Remember SQLite".into(),
                }],
            }],
        )
        .unwrap();
        let session = crate::db::unprocessed_session_by_session_id(&connection, session_id)
            .unwrap()
            .remove(0);
        crate::db::mark_session_processed_with_optional_task(
            &connection,
            session.id,
            None,
            session_id,
            "Summary",
            &format!("/tmp/{session_id}/summary.md"),
        )
        .unwrap();
        crate::db::replace_session_memories(&connection, &session, &[memory.to_string()]).unwrap();
        let memory_dir = paths
            .memories_dir
            .join("sessions")
            .join(crate::utils::slugify_lower(session_id));
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("memory.md"), format!("{memory}\n")).unwrap();
        session
    }
}
