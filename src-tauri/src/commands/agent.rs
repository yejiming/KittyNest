#[tauri::command]
pub fn start_agent_run(
    app: tauri::AppHandle,
    session_id: String,
    project_slug: String,
    message: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let (project_root, project_summary_root) =
        assistant_project_paths(&services.paths, &project_slug).map_err(to_command_error)?;
    let settings = crate::config::resolve_llm_settings(
        &crate::config::read_llm_settings(&services.paths).map_err(to_command_error)?,
        crate::config::LlmScenario::Assistant,
    );
    assistant_registry(app).start_run(
        session_id,
        project_root,
        project_summary_root,
        services.paths.clone(),
        settings,
        message,
    );
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
pub fn clear_agent_session(
    app: tauri::AppHandle,
    session_id: String,
    _services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    assistant_registry(app).clear_session(&session_id);
    Ok(serde_json::json!({ "cleared": true }))
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
pub fn resolve_agent_create_task(
    app: tauri::AppHandle,
    session_id: String,
    request_id: String,
    accepted: bool,
    _services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let resolved = assistant_registry(app).resolve_create_task(&session_id, &request_id, accepted);
    Ok(serde_json::json!({ "resolved": resolved }))
}

#[tauri::command]
pub fn save_agent_session(
    app: tauri::AppHandle,
    session_id: String,
    project_slug: String,
    timeline: crate::models::AgentTimelinePayload,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    let snapshot = assistant_registry(app).session_export(&session_id);
    enqueue_save_agent_session_inner(
        &services.paths,
        &session_id,
        &project_slug,
        timeline,
        snapshot.llm_messages,
    )
        .map(|result| serde_json::json!({ "jobId": result.job_id, "total": result.total }))
        .map_err(to_command_error)
}

#[tauri::command]
pub fn load_agent_session(
    app: tauri::AppHandle,
    project_slug: String,
    task_slug: String,
    services: State<'_, AppServices>,
) -> CommandResult<serde_json::Value> {
    load_agent_session_inner(app, &services.paths, &project_slug, &task_slug)
        .map(|payload| serde_json::to_value(payload).unwrap_or_else(|_| serde_json::json!({})))
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

fn run_save_agent_session_job_with_metadata<F>(
    paths: &crate::models::AppPaths,
    job_id: i64,
    session_id: &str,
    project_slug: &str,
    request_metadata: F,
) -> anyhow::Result<crate::models::TaskRecord>
where
    F: FnOnce(&crate::models::LlmSettings, Vec<serde_json::Value>) -> anyhow::Result<String>,
{
    let payload_path = save_agent_session_payload_path(paths, job_id);
    let payload: QueuedAgentSessionSavePayload =
        serde_json::from_str(&std::fs::read_to_string(&payload_path)?)?;
    if payload.session_id != session_id || payload.project_slug != project_slug {
        anyhow::bail!("Assistant session save payload does not match queued job");
    }
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let (project_id, project) = crate::db::get_project_by_slug(&connection, project_slug)?
        .ok_or_else(|| anyhow::anyhow!("Project not found: {project_slug}"))?;
    if project.review_status != "reviewed" {
        anyhow::bail!("Task Assistant requires a reviewed project");
    }
    let settings = crate::config::resolve_llm_settings(
        &crate::config::read_llm_settings(paths)?,
        crate::config::LlmScenario::Assistant,
    );
    let raw = request_metadata(&settings, task_metadata_messages(&payload.timeline))?;
    crate::db::record_llm_provider_call_for_paths(paths, &settings.provider);
    let draft = parse_task_metadata_json(&raw)?;
    let task_slug =
        crate::db::unique_task_slug(&connection, project_id, &crate::utils::slugify_lower(&draft.task_name))?;
    let task_dir = paths
        .projects_dir
        .join(&project.slug)
        .join("tasks")
        .join(&task_slug);
    std::fs::create_dir_all(&task_dir)?;
    let description_path = task_dir.join("description.md");
    let session_path = task_dir.join("session.json");
    std::fs::write(&description_path, format!("{}\n", draft.task_description))?;
    let saved = crate::models::SavedAgentSessionPayload {
        version: 1,
        session_id: session_id.to_string(),
        project_slug: project.slug.clone(),
        project_root: project.workdir.clone(),
        created_at: crate::utils::now_rfc3339(),
        messages: payload.timeline.messages,
        todos: payload.timeline.todos,
        context: payload.timeline.context,
        llm_messages: payload.llm_messages,
    };
    std::fs::write(&session_path, serde_json::to_string_pretty(&saved)?)?;
    crate::db::upsert_task(
        &connection,
        project_id,
        &task_slug,
        &draft.task_name,
        &draft.task_description,
        "discussing",
        &description_path.to_string_lossy(),
    )?;
    crate::db::list_tasks(&connection)?
        .into_iter()
        .find(|task| task.project_slug == project.slug && task.slug == task_slug)
        .ok_or_else(|| anyhow::anyhow!("saved task not found after create"))
}

pub(crate) fn save_agent_session_payload_path(
    paths: &crate::models::AppPaths,
    job_id: i64,
) -> std::path::PathBuf {
    paths
        .data_dir
        .join("jobs")
        .join(job_id.to_string())
        .join("agent_session.json")
}

fn load_agent_session_inner(
    app: tauri::AppHandle,
    paths: &crate::models::AppPaths,
    project_slug: &str,
    task_slug: &str,
) -> anyhow::Result<crate::models::SavedAgentSessionPayload> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let task = crate::db::list_tasks(&connection)?
        .into_iter()
        .find(|task| task.project_slug == project_slug && task.slug == task_slug)
        .ok_or_else(|| anyhow::anyhow!("Task not found: {project_slug}/{task_slug}"))?;
    let session_path = task
        .session_path
        .ok_or_else(|| anyhow::anyhow!("Task has no saved agent session"))?;
    let content = std::fs::read_to_string(&session_path)?;
    let saved: crate::models::SavedAgentSessionPayload = serde_json::from_str(&content)?;
    let messages = saved
        .llm_messages
        .iter()
        .filter_map(|message| {
            let role = message.get("role").and_then(serde_json::Value::as_str)?;
            let content = message
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            Some(crate::assistant::context::AgentStoredMessage::new(role, content))
        })
        .collect::<Vec<_>>();
    let todos = saved
        .todos
        .iter()
        .filter_map(|todo| serde_json::from_value::<crate::assistant::tools::AgentTodo>(todo.clone()).ok())
        .collect::<Vec<_>>();
    assistant_registry(app).session_import(
        &saved.session_id,
        crate::assistant::AgentSessionSnapshot {
            messages,
            llm_messages: saved.llm_messages.clone(),
            todos,
        },
    );
    Ok(saved)
}

fn task_metadata_messages(timeline: &crate::models::AgentTimelinePayload) -> Vec<serde_json::Value> {
    let transcript = timeline
        .messages
        .iter()
        .filter_map(|message| {
            let role = message.get("role").and_then(serde_json::Value::as_str)?;
            if role != "user" && role != "assistant" {
                return None;
            }
            let content = message
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim();
            if content.is_empty() {
                return None;
            }
            Some(format!("{role}: {content}"))
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    vec![
        serde_json::json!({"role": "system", "content": "Return only JSON with task_name and task_description. The task_name must be concise. The task_description must be markdown grounded only in the transcript."}),
        serde_json::json!({"role": "user", "content": transcript}),
    ]
}
