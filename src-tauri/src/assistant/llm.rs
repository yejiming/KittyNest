use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq)]
pub struct AssistantToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssistantLlmResponse {
    pub content: String,
    pub tool_calls: Vec<AssistantToolCall>,
}

#[derive(Default)]
struct ToolCallParts {
    id: String,
    name: String,
    arguments: String,
}

pub fn openai_stream_body(
    settings: &crate::models::LlmSettings,
    messages: Vec<serde_json::Value>,
    tools: Vec<serde_json::Value>,
) -> serde_json::Value {
    let max_tokens = if settings.max_tokens == 0 {
        4096
    } else {
        settings.max_tokens
    };
    let mut body = serde_json::json!({
        "model": settings.model,
        "messages": messages,
        "stream": true,
        "max_tokens": max_tokens,
        "max_completion_tokens": max_tokens,
        "temperature": if settings.temperature.is_finite() { settings.temperature } else { 0.2 }
    });
    if !tools.is_empty() {
        body["tools"] = serde_json::Value::Array(tools);
    }
    body
}

pub fn parse_openai_sse<R, F>(reader: R, mut on_token: F) -> anyhow::Result<AssistantLlmResponse>
where
    R: Read,
    F: FnMut(&str),
{
    let mut content = String::new();
    let mut tool_call_map: BTreeMap<usize, ToolCallParts> = BTreeMap::new();

    for line in BufReader::new(reader).lines() {
        let line = line?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            break;
        }
        let value: serde_json::Value = serde_json::from_str(data)?;
        let Some(delta) = value.pointer("/choices/0/delta") else {
            continue;
        };
        if let Some(token) = delta.get("content").and_then(serde_json::Value::as_str) {
            content.push_str(token);
            on_token(token);
        }
        let Some(tool_calls) = delta
            .get("tool_calls")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        for tool_call in tool_calls {
            let index = tool_call
                .get("index")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
            let entry = tool_call_map.entry(index).or_default();
            if let Some(id) = tool_call.get("id").and_then(serde_json::Value::as_str) {
                entry.id = id.into();
            }
            let Some(function) = tool_call.get("function") else {
                continue;
            };
            if let Some(name) = function.get("name").and_then(serde_json::Value::as_str) {
                entry.name = name.into();
            }
            if let Some(arguments) = function
                .get("arguments")
                .and_then(serde_json::Value::as_str)
            {
                entry.arguments.push_str(arguments);
            }
        }
    }

    let tool_calls = tool_call_map
        .into_values()
        .map(|raw| {
            let arguments =
                serde_json::from_str(&raw.arguments).unwrap_or_else(|_| serde_json::json!({}));
            AssistantToolCall {
                id: raw.id,
                name: raw.name,
                arguments,
            }
        })
        .collect();

    Ok(AssistantLlmResponse {
        content,
        tool_calls,
    })
}

pub fn request_openai_stream<F>(
    settings: &crate::models::LlmSettings,
    messages: Vec<serde_json::Value>,
    tools: Vec<serde_json::Value>,
    on_token: F,
) -> anyhow::Result<AssistantLlmResponse>
where
    F: FnMut(&str),
{
    if settings.interface != "openai" {
        anyhow::bail!("Task Assistant currently requires an OpenAI-compatible Assistant model");
    }
    if !crate::llm::configured_for_remote(settings) {
        anyhow::bail!("LLM settings are incomplete");
    }
    let endpoint = assistant_endpoint(&settings.base_url);
    let body = openai_stream_body(settings, messages, tools);
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?
        .post(endpoint)
        .bearer_auth(&settings.api_key)
        .json(&body)
        .send()?
        .error_for_status()?;
    parse_openai_sse(response, on_token)
}

pub fn request_openai_json(
    settings: &crate::models::LlmSettings,
    messages: Vec<serde_json::Value>,
) -> anyhow::Result<String> {
    if settings.interface != "openai" {
        anyhow::bail!("Task Assistant currently requires an OpenAI-compatible Assistant model");
    }
    if !crate::llm::configured_for_remote(settings) {
        anyhow::bail!("LLM settings are incomplete");
    }
    let max_tokens = if settings.max_tokens == 0 {
        4096
    } else {
        settings.max_tokens
    };
    let body = serde_json::json!({
        "model": settings.model,
        "messages": messages,
        "stream": false,
        "max_tokens": max_tokens,
        "max_completion_tokens": max_tokens,
        "temperature": if settings.temperature.is_finite() { settings.temperature } else { 0.2 }
    });
    let value: serde_json::Value = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?
        .post(assistant_endpoint(&settings.base_url))
        .bearer_auth(&settings.api_key)
        .json(&body)
        .send()?
        .error_for_status()?
        .json()?;
    Ok(value
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string())
}

fn assistant_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sse_parser_collects_tokens_and_tool_call_arguments() {
        let input = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hi \"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"file_path\\\":\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"src/App.tsx\\\"}\"}}]}}]}\n\n",
            "data: [DONE]\n\n"
        );

        let response = super::parse_openai_sse(input.as_bytes(), |_| {}).unwrap();

        assert_eq!(response.content, "Hi ");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call_1");
        assert_eq!(response.tool_calls[0].name, "read_file");
        assert_eq!(
            response.tool_calls[0].arguments,
            serde_json::json!({"file_path": "src/App.tsx"})
        );
    }

    #[test]
    fn openai_body_uses_max_completion_tokens_and_tools() {
        let mut settings = crate::config::default_llm_settings();
        settings.model = "openai/gpt-4o-mini".into();
        settings.max_tokens = 123;
        let body = super::openai_stream_body(
            &settings,
            vec![serde_json::json!({"role": "user", "content": "hello"})],
            vec![
                serde_json::json!({"type": "function", "function": {"name": "read_file", "parameters": {"type": "object"}}}),
            ],
        );

        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 123);
        assert_eq!(body["max_completion_tokens"], 123);
        assert_eq!(body["tools"][0]["function"]["name"], "read_file");
    }
}
