use super::{function_schema, ToolEnvironment};

pub fn schema() -> serde_json::Value {
    function_schema(
        "ask_user",
        "Ask the user clarifying questions.",
        serde_json::json!({
            "type": "object",
            "properties": {"questions": {"type": "array"}},
            "required": ["questions"]
        }),
    )
}

pub fn execute(arguments: serde_json::Value, env: &mut ToolEnvironment) -> String {
    let Some(questions) = arguments
        .get("questions")
        .and_then(serde_json::Value::as_array)
    else {
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
