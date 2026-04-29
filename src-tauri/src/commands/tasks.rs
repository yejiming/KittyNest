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
    remove_all_session_memory_dirs(paths)?;
    crate::graph::reset_graph(paths)?;
    Ok(reset)
}

pub(crate) fn reset_projects_inner(paths: &crate::models::AppPaths) -> anyhow::Result<usize> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let reset = crate::db::reset_all_projects(&connection)?;
    for project_dir in project_dirs(paths)? {
        std::fs::remove_dir_all(project_dir)?;
    }
    remove_all_session_memory_dirs(paths)?;
    crate::graph::reset_graph(paths)?;
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

fn remove_all_session_memory_dirs(paths: &crate::models::AppPaths) -> anyhow::Result<()> {
    let session_memories_dir = paths.memories_dir.join("sessions");
    if session_memories_dir.exists() {
        std::fs::remove_dir_all(session_memories_dir)?;
    }
    Ok(())
}

fn remove_session_artifacts(
    paths: &crate::models::AppPaths,
    session: &crate::models::StoredSession,
) -> anyhow::Result<()> {
    let session_dir = paths
        .projects_dir
        .join(&session.project_slug)
        .join("sessions")
        .join(crate::utils::slugify_lower(&session.session_id));
    if session_dir.exists() {
        std::fs::remove_dir_all(session_dir)?;
    }
    let memory_dir = paths
        .memories_dir
        .join("sessions")
        .join(crate::utils::slugify_lower(&session.session_id));
    if memory_dir.exists() {
        std::fs::remove_dir_all(memory_dir)?;
    }
    crate::graph::delete_session_entities(paths, &session.session_id)
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
