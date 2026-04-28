use regex::Regex;

use super::{function_schema, resolve_tool_path, walk_files, ToolEnvironment};

pub fn schema() -> serde_json::Value {
    function_schema(
        "grep",
        "Search project summary file contents with regex.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string"},
                "path": {"type": "string"},
                "include": {"type": "string"}
            },
            "required": ["pattern"]
        }),
    )
}

pub fn execute(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let Some(pattern) = arguments.get("pattern").and_then(serde_json::Value::as_str) else {
        return "Error: pattern is required".into();
    };
    let regex = match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => return format!("Invalid regex: {error}"),
    };
    let base = match resolve_tool_path(
        arguments.get("path").and_then(serde_json::Value::as_str),
        &env.project_summary_root.clone(),
        &env.project_summary_root.clone(),
        env,
    ) {
        Ok(path) => path,
        Err(error) => return error,
    };
    if !base.exists() {
        return format!("Error: {} not found", base.display());
    }
    let include = arguments.get("include").and_then(serde_json::Value::as_str);
    let files = if base.is_file() {
        vec![base]
    } else {
        walk_files(&base, include)
    };
    let mut matches = Vec::new();
    for file_path in files {
        let Ok(text) = std::fs::read_to_string(&file_path) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if regex.is_match(line) {
                matches.push(format!("{}:{}: {}", file_path.display(), index + 1, line));
                if matches.len() >= 200 {
                    matches.push("... (200 match limit reached)".into());
                    return matches.join("\n");
                }
            }
        }
    }
    if matches.is_empty() {
        "No matches found.".into()
    } else {
        matches.join("\n")
    }
}
