use super::{function_schema, ToolEnvironment};

pub fn schema() -> serde_json::Value {
    function_schema(
        "read_memory",
        "Read stored memories related to an entity in KittyNest.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "entity": {"type": "string"}
            },
            "required": ["entity"]
        }),
    )
}

pub fn execute(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let Some(entity) = arguments.get("entity").and_then(serde_json::Value::as_str) else {
        return "Error: entity is required".into();
    };
    let entity = entity.trim();
    if entity.is_empty() {
        return "Error: entity is required".into();
    }
    let Some(paths) = app_paths_from_summary_root(&env.project_summary_root) else {
        return "Error: project summary root is not inside KittyNest projects store".into();
    };
    let related = match crate::graph::related_sessions_for_entity(&paths, entity) {
        Ok(related) => related,
        Err(error) => return format!("Error: {error}"),
    };
    if related.is_empty() {
        return format!("No memories found for entity: {entity}");
    }
    let session_ids = related
        .iter()
        .map(|session| session.session_id.clone())
        .collect::<Vec<_>>();
    let connection = match crate::db::open(&paths).and_then(|connection| {
        crate::db::migrate(&connection)?;
        Ok(connection)
    }) {
        Ok(connection) => connection,
        Err(error) => return format!("Error: {error}"),
    };
    let memories = match crate::db::session_memories_for_sessions(&connection, &session_ids) {
        Ok(memories) => memories,
        Err(error) => return format!("Error: {error}"),
    };
    if memories.is_empty() {
        return format!("No memories found for entity: {entity}");
    }
    let rows = memories
        .into_iter()
        .map(|memory| {
            serde_json::json!({
                "sourceSession": memory.source_session,
                "sessionTitle": memory.session_title,
                "projectSlug": memory.project_slug,
                "memory": memory.memory,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string_pretty(&serde_json::json!({
        "entity": entity,
        "memories": rows,
    }))
    .unwrap_or_else(|error| format!("Error: {error}"))
}

fn app_paths_from_summary_root(summary_root: &std::path::Path) -> Option<crate::models::AppPaths> {
    let projects_dir = summary_root.parent()?;
    if projects_dir.file_name()?.to_string_lossy() != "projects" {
        return None;
    }
    let data_dir = projects_dir.parent()?;
    Some(crate::models::AppPaths::from_data_dir(data_dir.to_path_buf()))
}
