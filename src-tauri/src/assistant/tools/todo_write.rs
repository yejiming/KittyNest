use super::{function_schema, AgentTodo, ToolEnvironment};

pub fn schema() -> serde_json::Value {
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
    )
}

pub fn execute(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
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
        if !matches!(
            todo.status.as_str(),
            "pending" | "in_progress" | "completed"
        ) {
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
