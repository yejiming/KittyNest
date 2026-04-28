use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

mod ask_user;
mod glob;
mod grep;
mod read_file;
mod todo_write;

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
    pub project_summary_root: PathBuf,
    pub todos: Vec<AgentTodo>,
    pub permission_requests: Vec<PermissionRequestRecord>,
    permission_decisions: Option<Arc<Mutex<Vec<String>>>>,
    permission_handler: Option<Box<dyn FnMut(&str, &str) -> PermissionDecision + Send>>,
    ask_user_handler: Option<Box<dyn FnMut(Vec<serde_json::Value>) -> serde_json::Value + Send>>,
}

impl ToolEnvironment {
    pub fn new(project_root: PathBuf, project_summary_root: PathBuf) -> Self {
        Self {
            project_root,
            project_summary_root,
            todos: Vec::new(),
            permission_requests: Vec::new(),
            permission_decisions: None,
            permission_handler: None,
            ask_user_handler: None,
        }
    }

    #[cfg(test)]
    pub fn for_tests(project_root: &Path) -> Self {
        Self::new(project_root.to_path_buf(), project_root.to_path_buf())
    }

    #[cfg(test)]
    pub fn for_tests_with_summary(project_root: &Path, project_summary_root: &Path) -> Self {
        Self::new(
            project_root.to_path_buf(),
            project_summary_root.to_path_buf(),
        )
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

    pub fn set_permission_handler<F>(&mut self, handler: F)
    where
        F: FnMut(&str, &str) -> PermissionDecision + Send + 'static,
    {
        self.permission_handler = Some(Box::new(handler));
    }

    pub fn request_permission(&mut self, title: &str, description: &str) -> PermissionDecision {
        self.permission_requests.push(PermissionRequestRecord {
            title: title.into(),
            description: description.into(),
        });
        if let Some(handler) = self.permission_handler.as_mut() {
            return handler(title, description);
        }
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
        read_file::schema(),
        grep::schema(),
        glob::schema(),
        todo_write::schema(),
        ask_user::schema(),
    ]
}

fn function_schema(
    name: &str,
    description: &str,
    parameters: serde_json::Value,
) -> serde_json::Value {
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
        "read_file" => read_file::execute(arguments, env),
        "grep" => grep::execute(arguments, env),
        "glob" => glob::execute(arguments, env),
        "todo_write" => todo_write::execute(arguments, env),
        "ask_user" => ask_user::execute(arguments, env),
        _ => format!("Error: unknown tool '{name}'"),
    }
}

fn resolve_tool_path(
    raw: Option<&str>,
    default: &Path,
    allowed_root: &Path,
    env: &mut ToolEnvironment,
) -> Result<PathBuf, String> {
    let requested = raw
        .map(PathBuf::from)
        .unwrap_or_else(|| default.to_path_buf());
    let absolute = if requested.is_absolute() {
        requested
    } else {
        default.join(requested)
    };
    let canonical = absolute
        .canonicalize()
        .map_err(|error| format!("Error: {error}"))?;
    let allowed_root = allowed_root
        .canonicalize()
        .map_err(|error| format!("Error: {error}"))?;
    if canonical.starts_with(&allowed_root) {
        return Ok(canonical);
    }
    let decision = env.request_permission(
        "File Permission",
        &format!(
            "Allow the assistant to access this path outside the default allowed directory?\n\n{}",
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
    let include_pattern = include.and_then(|pattern| ::glob::Pattern::new(pattern).ok());
    WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            !entry.path().components().any(|component| {
                SKIP_DIRS.contains(&component.as_os_str().to_string_lossy().as_ref())
            })
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
        std::fs::write(
            temp.path().join("a.rs"),
            "fn main() {}\nlet needle = true;\n",
        )
        .unwrap();
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

    #[test]
    fn read_file_allows_summary_root_without_permission() {
        let code_root = tempfile::tempdir().unwrap();
        let summary_root = tempfile::tempdir().unwrap();
        let summary_file = summary_root.path().join("summary.md");
        std::fs::write(&summary_file, "summary").unwrap();
        let mut env =
            super::ToolEnvironment::for_tests_with_summary(code_root.path(), summary_root.path());

        let result = super::execute_tool(
            "read_file",
            serde_json::json!({"file_path": "summary.md"}),
            &mut env,
        );

        assert_eq!(result, "1\tsummary");
        assert!(env.permission_requests.is_empty());
    }

    #[test]
    fn read_file_requests_permission_for_code_root() {
        let code_root = tempfile::tempdir().unwrap();
        let summary_root = tempfile::tempdir().unwrap();
        let code_file = code_root.path().join("src.txt");
        std::fs::write(&code_file, "source").unwrap();
        let decisions = Arc::new(Mutex::new(vec!["deny".to_string()]));
        let mut env =
            super::ToolEnvironment::for_tests_with_summary(code_root.path(), summary_root.path())
                .with_permission_decisions(decisions);

        let result = super::execute_tool(
            "read_file",
            serde_json::json!({"file_path": code_file}),
            &mut env,
        );

        assert!(result.contains("User denied permission"));
        assert_eq!(env.permission_requests.len(), 1);
    }
}
