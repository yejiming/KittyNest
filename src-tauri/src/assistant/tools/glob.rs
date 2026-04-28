use super::{function_schema, resolve_tool_path, ToolEnvironment};

pub fn schema() -> serde_json::Value {
    function_schema(
        "glob",
        "Find files matching a glob pattern.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string"},
                "path": {"type": "string"}
            },
            "required": ["pattern"]
        }),
    )
}

pub fn execute(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let Some(pattern) = arguments.get("pattern").and_then(serde_json::Value::as_str) else {
        return "Error: pattern is required".into();
    };
    let base = match resolve_tool_path(
        arguments.get("path").and_then(serde_json::Value::as_str),
        &env.project_root.clone(),
        &env.project_root.clone(),
        env,
    ) {
        Ok(path) => path,
        Err(error) => return error,
    };
    if !base.is_dir() {
        return format!("Error: {} is not a directory", base.display());
    }
    let pattern_path = base.join(pattern);
    let Some(pattern_text) = pattern_path.to_str() else {
        return "Error: glob pattern path is not valid UTF-8".into();
    };
    let entries = match ::glob::glob(pattern_text) {
        Ok(entries) => entries,
        Err(error) => return format!("Error: {error}"),
    };
    let mut hits = entries
        .filter_map(Result::ok)
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    hits.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    hits.reverse();
    let total = hits.len();
    let mut shown = hits
        .into_iter()
        .take(100)
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if total > 100 {
        shown.push(format!("... ({total} matches, showing first 100)"));
    }
    if shown.is_empty() {
        "No files matched.".into()
    } else {
        shown.join("\n")
    }
}
