use super::{function_schema, resolve_tool_path, ToolEnvironment};

pub fn schema() -> serde_json::Value {
    function_schema(
        "read_file",
        "Read a project summary file's contents with line numbers.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {"type": "string"},
                "offset": {"type": "integer"},
                "limit": {"type": "integer"}
            },
            "required": ["file_path"]
        }),
    )
}

pub fn execute(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let Some(file_path) = arguments
        .get("file_path")
        .and_then(serde_json::Value::as_str)
    else {
        return "Error: file_path is required".into();
    };
    let path = match resolve_tool_path(
        Some(file_path),
        &env.project_summary_root.clone(),
        &env.project_summary_root.clone(),
        env,
    ) {
        Ok(path) => path,
        Err(error) => return error,
    };
    if !path.exists() {
        return format!("Error: {} not found", path.display());
    }
    if !path.is_file() {
        return format!("Error: {} is a directory, not a file", path.display());
    }
    let offset = arguments
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1)
        .max(1) as usize;
    let limit = arguments
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(2000)
        .max(1) as usize;
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => return format!("Error: {error}"),
    };
    let lines = text.lines().collect::<Vec<_>>();
    let start = offset.saturating_sub(1);
    let chunk = lines
        .iter()
        .enumerate()
        .skip(start)
        .take(limit)
        .map(|(index, line)| format!("{}\t{}", index + 1, line))
        .collect::<Vec<_>>();
    if chunk.is_empty() {
        return "(empty file)".into();
    }
    chunk.join("\n")
}
