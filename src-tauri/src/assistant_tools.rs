use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    "dist",
    "build",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTodo {
    pub content: String,
    #[serde(alias = "active_form")]
    pub active_form: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRequestRecord {
    pub title: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionDecision {
    pub value: String,
    pub supplemental_info: String,
}

pub struct ToolEnvironment {
    pub project_root: PathBuf,
    pub todos: Vec<AgentTodo>,
    pub permission_requests: Vec<PermissionRequestRecord>,
    permission_decisions: Option<Arc<Mutex<Vec<String>>>>,
    ask_user_handler: Option<Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send>>,
}

impl ToolEnvironment {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            todos: Vec::new(),
            permission_requests: Vec::new(),
            permission_decisions: None,
            ask_user_handler: None,
        }
    }

    #[cfg(test)]
    pub fn for_tests(project_root: &Path) -> Self {
        Self::new(project_root.to_path_buf())
    }

    #[cfg(test)]
    pub fn with_permission_decisions(mut self, decisions: Arc<Mutex<Vec<String>>>) -> Self {
        self.permission_decisions = Some(decisions);
        self
    }

    pub fn set_ask_user_handler<F>(&mut self, handler: F)
    where
        F: FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send + 'static,
    {
        self.ask_user_handler = Some(Box::new(handler));
    }

    pub fn request_permission(&mut self, title: &str, description: &str) -> PermissionDecision {
        self.permission_requests.push(PermissionRequestRecord {
            title: title.into(),
            description: description.into(),
        });
        let value = self
            .permission_decisions
            .as_ref()
            .and_then(|items| items.lock().ok()?.pop())
            .unwrap_or_else(|| "deny".into());
        PermissionDecision {
            value,
            supplemental_info: String::new(),
        }
    }
}

pub fn tool_schemas() -> Vec<serde_json::Value> {
    vec![
        function_schema(
            "read_file",
            "Read a file's contents with line numbers.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": {"type": "string"},
                    "offset": {"type": "integer"},
                    "limit": {"type": "integer"}
                },
                "required": ["file_path"]
            }),
        ),
        function_schema(
            "grep",
            "Search file contents with regex.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string"},
                    "path": {"type": "string"},
                    "include": {"type": "string"}
                },
                "required": ["pattern"]
            }),
        ),
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
        ),
        function_schema(
            "todo_write",
            "Create and manage the current task list.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": {"type": "string"},
                                "active_form": {"type": "string"},
                                "status": {"type": "string"}
                            },
                            "required": ["content", "active_form", "status"]
                        }
                    }
                },
                "required": ["todos"]
            }),
        ),
        function_schema(
            "ask_user",
            "Ask the user clarifying questions.",
            serde_json::json!({
                "type": "object",
                "properties": {"questions": {"type": "array"}},
                "required": ["questions"]
            }),
        ),
    ]
}

fn function_schema(name: &str, description: &str, parameters: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters
        }
    })
}

pub fn execute_tool(name: &str, arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    match name {
        "read_file" => read_file(arguments, env),
        "grep" => grep(arguments, env),
        "glob" => glob_files(arguments, env),
        "todo_write" => todo_write(arguments, env),
        "ask_user" => ask_user(arguments, env),
        _ => format!("Error: unknown tool '{name}'"),
    }
}

fn read_file(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let Some(file_path) = arguments.get("file_path").and_then(serde_json::Value::as_str) else {
        return "Error: file_path is required".into();
    };
    let path = match resolve_tool_path(Some(file_path), &env.project_root.clone(), env) {
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

fn grep(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let Some(pattern) = arguments.get("pattern").and_then(serde_json::Value::as_str) else {
        return "Error: pattern is required".into();
    };
    let regex = match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => return format!("Invalid regex: {error}"),
    };
    let base = match resolve_tool_path(
        arguments.get("path").and_then(serde_json::Value::as_str),
        &env.project_root.clone(),
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

fn glob_files(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let Some(pattern) = arguments.get("pattern").and_then(serde_json::Value::as_str) else {
        return "Error: pattern is required".into();
    };
    let base = match resolve_tool_path(
        arguments.get("path").and_then(serde_json::Value::as_str),
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
    let entries = match glob::glob(pattern_text) {
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

fn todo_write(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let todos = match normalize_todos(arguments.get("todos")) {
        Ok(todos) => todos,
        Err(error) => return format!("Error: {error}"),
    };
    let previous = env.todos.len();
    let remaining = todos
        .iter()
        .filter(|todo| todo.status != "completed")
        .cloned()
        .collect::<Vec<_>>();
    env.todos = remaining;
    let mut lines = vec![
        "Todo list updated.".to_string(),
        format!("Previous items: {previous}"),
        format!("Current items: {}", env.todos.len()),
    ];
    for (index, item) in todos.iter().enumerate() {
        lines.push(format!("{}. [{}] {}", index + 1, item.status, item.content));
    }
    if env.todos.is_empty() {
        lines.push("All tasks are completed.".into());
    }
    lines.join("\n")
}

fn ask_user(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let Some(questions) = arguments.get("questions").and_then(serde_json::Value::as_array) else {
        return "Error: questions must contain at least one item".into();
    };
    if questions.is_empty() || questions.len() > 4 {
        return "Error: questions must contain 1-4 items".into();
    }
    let Some(handler) = env.ask_user_handler.as_mut() else {
        return "Error: ask_user requires an interactive KittyNest session".into();
    };
    let response = handler(questions.clone());
    let Some(answers) = response.as_object() else {
        return "User declined to answer the questions.".into();
    };
    if answers.is_empty() {
        return "User declined to answer the questions.".into();
    }
    let mut lines = vec!["User answers:".to_string()];
    for question in questions {
        let header = question
            .get("header")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Question");
        let question_text = question
            .get("question")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let answer = answers
            .get(question_text)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("(no answer)");
        lines.push(format!("- {header}: {answer}"));
    }
    lines.join("\n")
}

fn normalize_todos(value: Option<&serde_json::Value>) -> Result<Vec<AgentTodo>, String> {
    let Some(items) = value.and_then(serde_json::Value::as_array) else {
        return Err("todos must contain at least one item".into());
    };
    if items.is_empty() {
        return Err("todos must contain at least one item".into());
    }
    let mut todos = Vec::new();
    let mut in_progress = 0;
    for (index, item) in items.iter().enumerate() {
        let mut todo: AgentTodo = serde_json::from_value(item.clone())
            .map_err(|error| format!("todo #{} is invalid: {error}", index + 1))?;
        todo.content = todo.content.trim().into();
        todo.active_form = todo.active_form.trim().into();
        todo.status = todo.status.trim().to_ascii_lowercase();
        if todo.content.is_empty() {
            return Err(format!("todo #{} is missing content", index + 1));
        }
        if todo.active_form.is_empty() {
            return Err(format!("todo #{} is missing active_form", index + 1));
        }
        if !matches!(todo.status.as_str(), "pending" | "in_progress" | "completed") {
            return Err(format!("todo #{} has invalid status", index + 1));
        }
        if todo.status == "in_progress" {
            in_progress += 1;
        }
        todos.push(todo);
    }
    let unfinished = todos.iter().any(|todo| todo.status != "completed");
    if unfinished && in_progress != 1 {
        return Err("exactly one todo must be in_progress while unfinished work remains".into());
    }
    if !unfinished && in_progress > 0 {
        return Err("completed todo lists cannot contain in_progress items".into());
    }
    Ok(todos)
}

fn resolve_tool_path(
    raw: Option<&str>,
    default: &Path,
    env: &mut ToolEnvironment,
) -> Result<PathBuf, String> {
    let requested = raw.map(PathBuf::from).unwrap_or_else(|| default.to_path_buf());
    let absolute = if requested.is_absolute() {
        requested
    } else {
        env.project_root.join(requested)
    };
    let canonical = absolute.canonicalize().map_err(|error| format!("Error: {error}"))?;
    let project_root = env
        .project_root
        .canonicalize()
        .map_err(|error| format!("Error: {error}"))?;
    if canonical.starts_with(&project_root) {
        return Ok(canonical);
    }
    let decision = env.request_permission(
        "File Permission",
        &format!(
            "Allow the assistant to access this path outside the selected project?\n\n{}",
            canonical.display()
        ),
    );
    if decision.value == "allow" {
        Ok(canonical)
    } else {
        Err(append_permission_note(
            "User denied permission grant",
            &decision.supplemental_info,
        ))
    }
}

fn append_permission_note(output: &str, supplemental_info: &str) -> String {
    let note = supplemental_info.trim();
    if note.is_empty() {
        output.into()
    } else {
        format!("{output}\n[permission note]\n{note}")
    }
}

fn walk_files(root: &Path, include: Option<&str>) -> Vec<PathBuf> {
    let include_pattern = include.and_then(|pattern| glob::Pattern::new(pattern).ok());
    WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            !entry
                .path()
                .components()
                .any(|component| SKIP_DIRS.contains(&component.as_os_str().to_string_lossy().as_ref()))
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            let Some(pattern) = &include_pattern else {
                return true;
            };
            let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
            pattern.matches_path(relative)
                || entry
                    .path()
                    .file_name()
                    .is_some_and(|name| pattern.matches(&name.to_string_lossy()))
        })
        .take(5000)
        .map(walkdir::DirEntry::into_path)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    #[test]
    fn read_file_returns_line_numbers_inside_project() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("src.txt");
        std::fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();
        let mut env = super::ToolEnvironment::for_tests(temp.path());

        let result = super::execute_tool(
            "read_file",
            serde_json::json!({"file_path": file, "offset": 2, "limit": 1}),
            &mut env,
        );

        assert_eq!(result, "2\tbeta");
    }

    #[test]
    fn grep_searches_matching_lines() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.rs"), "fn main() {}\nlet needle = true;\n").unwrap();
        let mut env = super::ToolEnvironment::for_tests(temp.path());

        let result = super::execute_tool(
            "grep",
            serde_json::json!({"pattern": "needle", "path": temp.path(), "include": "*.rs"}),
            &mut env,
        );

        assert!(result.contains("a.rs:2: let needle = true;"));
    }

    #[test]
    fn glob_lists_matching_files() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/App.tsx"), "").unwrap();
        let mut env = super::ToolEnvironment::for_tests(temp.path());

        let result = super::execute_tool(
            "glob",
            serde_json::json!({"pattern": "src/**/*.tsx", "path": temp.path()}),
            &mut env,
        );

        assert!(result.contains("App.tsx"));
    }

    #[test]
    fn todo_write_stores_unfinished_items() {
        let temp = tempfile::tempdir().unwrap();
        let mut env = super::ToolEnvironment::for_tests(temp.path());

        let result = super::execute_tool(
            "todo_write",
            serde_json::json!({
                "todos": [
                    {"content": "Ship drawer", "active_form": "Shipping drawer", "status": "in_progress"},
                    {"content": "Run tests", "active_form": "Running tests", "status": "pending"}
                ]
            }),
            &mut env,
        );

        assert!(result.contains("Todo list updated."));
        assert_eq!(env.todos.len(), 2);
    }

    #[test]
    fn outside_path_requests_permission() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("secret.txt");
        std::fs::write(&file, "secret").unwrap();
        let decisions = Arc::new(Mutex::new(vec!["deny".to_string()]));
        let mut env =
            super::ToolEnvironment::for_tests(temp.path()).with_permission_decisions(decisions);

        let result = super::execute_tool(
            "read_file",
            serde_json::json!({"file_path": file}),
            &mut env,
        );

        assert!(result.contains("User denied permission"));
        assert_eq!(env.permission_requests.len(), 1);
    }
}
