use tauri::State;

use crate::{
    errors::{to_command_error, CommandResult},
    models::{AppStateDto, LlmSettings, SourceStatus},
    services::AppServices,
};

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

fn read_markdown_file_inner(paths: &crate::models::AppPaths, path: &str) -> anyhow::Result<String> {
    let requested = std::path::PathBuf::from(path);
    let canonical = requested.canonicalize()?;
    let projects_dir = paths.projects_dir.canonicalize()?;
    if !canonical.starts_with(projects_dir) {
        anyhow::bail!("Markdown path is outside KittyNest project store");
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

#[cfg(test)]
mod tests {
    use super::{
        get_app_state_with_roots, get_cached_app_state_with_roots, read_markdown_file_inner,
        reset_projects_inner, reset_sessions_inner, reset_tasks_inner,
    };
    use crate::models::AppPaths;

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
}
