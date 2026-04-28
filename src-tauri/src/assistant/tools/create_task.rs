use super::{function_schema, ToolEnvironment};

pub fn schema() -> serde_json::Value {
    function_schema(
        "create_task",
        "Create a KittyNest task proposal from the current agent context and ask the user to accept it.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_name": {"type": "string"},
                "task_description": {"type": "string"}
            },
            "required": ["task_name", "task_description"]
        }),
    )
}

pub fn execute(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let task_name = string_arg(&arguments, "task_name", "taskName");
    let task_description = string_arg(&arguments, "task_description", "taskDescription");
    if task_name.is_empty() {
        return "Error: task_name is required".into();
    }
    if task_description.is_empty() {
        return "Error: task_description is required".into();
    }
    let Some(handler) = env.create_task_handler.as_mut() else {
        return "Error: create_task requires an interactive KittyNest session".into();
    };
    let response = handler(serde_json::json!({
        "taskName": task_name,
        "taskDescription": task_description,
    }));
    if !response
        .get("accepted")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return "User cancelled task creation.".into();
    }
    let Some((paths, project_slug)) = task_paths_from_summary_root(&env.project_summary_root) else {
        return "Error: project summary root is not inside KittyNest projects store".into();
    };
    match create_task_files(env, &paths, &project_slug, &task_name, &task_description) {
        Ok(task_slug) => format!("Task created: {project_slug}/{task_slug}"),
        Err(error) => format!("Error: {error}"),
    }
}

fn create_task_files(
    env: &ToolEnvironment,
    paths: &crate::models::AppPaths,
    project_slug: &str,
    task_name: &str,
    task_description: &str,
) -> anyhow::Result<String> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let (project_id, project) = crate::db::get_project_by_slug(&connection, project_slug)?
        .ok_or_else(|| anyhow::anyhow!("Project not found: {project_slug}"))?;
    let task_slug =
        crate::db::unique_task_slug(&connection, project_id, &crate::utils::slugify_lower(task_name))?;
    let task_dir = paths
        .projects_dir
        .join(project_slug)
        .join("tasks")
        .join(&task_slug);
    std::fs::create_dir_all(&task_dir)?;
    let description_path = task_dir.join("description.md");
    let session_path = task_dir.join("session.json");
    std::fs::write(&description_path, format!("{}\n", task_description.trim()))?;
    let now = crate::utils::now_rfc3339();
    let saved = crate::models::SavedAgentSessionPayload {
        version: 1,
        session_id: if env.session_id.is_empty() {
            format!("task-{task_slug}")
        } else {
            env.session_id.clone()
        },
        project_slug: project_slug.to_string(),
        project_root: project.workdir,
        created_at: now,
        messages: Vec::new(),
        todos: Vec::new(),
        context: serde_json::json!({}),
        llm_messages: Vec::new(),
    };
    std::fs::write(&session_path, serde_json::to_string_pretty(&saved)?)?;
    crate::db::upsert_task(
        &connection,
        project_id,
        &task_slug,
        task_name,
        task_description,
        "discussing",
        &description_path.to_string_lossy(),
    )?;
    Ok(task_slug)
}

fn string_arg(value: &serde_json::Value, snake: &str, camel: &str) -> String {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn task_paths_from_summary_root(
    summary_root: &std::path::Path,
) -> Option<(crate::models::AppPaths, String)> {
    let project_slug = summary_root.file_name()?.to_string_lossy().to_string();
    let projects_dir = summary_root.parent()?;
    if projects_dir.file_name()?.to_string_lossy() != "projects" {
        return None;
    }
    let data_dir = projects_dir.parent()?;
    Some((crate::models::AppPaths::from_data_dir(data_dir.to_path_buf()), project_slug))
}
