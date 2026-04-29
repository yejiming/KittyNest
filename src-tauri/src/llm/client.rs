use std::{
    sync::{Condvar, Mutex, OnceLock},
    time::Duration,
};

use reqwest::StatusCode;

use crate::models::LlmSettings;

const MAX_LLM_CONCURRENCY: usize = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlmJsonResponse {
    pub content: serde_json::Value,
    pub used_provider: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlmTextResponse {
    pub content: String,
    pub used_provider: String,
}

pub fn configured_for_remote(settings: &LlmSettings) -> bool {
    !settings.api_key.trim().is_empty()
        && !settings.model.trim().is_empty()
        && !settings.base_url.trim().is_empty()
}

pub fn request_json(
    settings: &LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<LlmJsonResponse> {
    let _permit = acquire_llm_permit();
    #[cfg(test)]
    if let Some(response) = test_support::next_json_response(settings, system_prompt, user_prompt) {
        return Ok(response);
    }
    if !configured_for_remote(settings) {
        anyhow::bail!("LLM settings are incomplete");
    }
    match settings.interface.as_str() {
        "anthropic" => request_anthropic_json(settings, system_prompt, user_prompt),
        _ => request_openai_json(settings, system_prompt, user_prompt),
    }
}

pub fn request_markdown(
    settings: &LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<LlmTextResponse> {
    let _permit = acquire_llm_permit();
    #[cfg(test)]
    if let Some(response) =
        test_support::next_markdown_response(settings, system_prompt, user_prompt)
    {
        return Ok(response);
    }
    if !configured_for_remote(settings) {
        anyhow::bail!("LLM settings are incomplete");
    }
    match settings.interface.as_str() {
        "anthropic" => request_anthropic_text(settings, system_prompt, user_prompt),
        _ => request_openai_text(settings, system_prompt, user_prompt),
    }
}

struct LlmConcurrencyGate {
    active: Mutex<usize>,
    available: Condvar,
}

struct LlmPermit {
    gate: &'static LlmConcurrencyGate,
}

impl Drop for LlmPermit {
    fn drop(&mut self) {
        let Ok(mut active) = self.gate.active.lock() else {
            return;
        };
        *active = active.saturating_sub(1);
        self.gate.available.notify_one();
    }
}

fn acquire_llm_permit() -> LlmPermit {
    static GATE: OnceLock<LlmConcurrencyGate> = OnceLock::new();
    let gate = GATE.get_or_init(|| LlmConcurrencyGate {
        active: Mutex::new(0),
        available: Condvar::new(),
    });
    let mut active = gate.active.lock().expect("LLM concurrency gate poisoned");
    while *active >= MAX_LLM_CONCURRENCY {
        active = gate
            .available
            .wait(active)
            .expect("LLM concurrency gate poisoned");
    }
    *active += 1;
    LlmPermit { gate }
}

fn openai_chat_body(
    settings: &LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
    json_response: bool,
) -> serde_json::Value {
    let (system_prompt, user_prompt) = limited_prompts(settings, system_prompt, user_prompt);
    let max_tokens = effective_max_tokens(settings);
    let mut body = serde_json::json!({
        "model": settings.model.clone(),
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "max_tokens": max_tokens,
        "max_completion_tokens": max_tokens,
        "temperature": effective_temperature(settings)
    });
    if json_response {
        body["response_format"] = serde_json::json!({"type": "json_object"});
    }
    body
}

fn anthropic_messages_body(
    settings: &LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> serde_json::Value {
    let (system_prompt, user_prompt) = limited_prompts(settings, system_prompt, user_prompt);
    serde_json::json!({
        "model": settings.model.clone(),
        "max_tokens": effective_max_tokens(settings),
        "temperature": effective_temperature(settings),
        "system": system_prompt,
        "messages": [
            {"role": "user", "content": user_prompt}
        ]
    })
}

fn effective_max_tokens(settings: &LlmSettings) -> usize {
    if settings.max_tokens == 0 {
        4096
    } else {
        settings.max_tokens
    }
}

fn effective_temperature(settings: &LlmSettings) -> f64 {
    if settings.temperature.is_finite() {
        settings.temperature
    } else {
        0.2
    }
}

fn limited_prompts(
    settings: &LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> (String, String) {
    let Some(limit) = settings
        .max_context
        .checked_mul(4)
        .filter(|limit| *limit > 0)
    else {
        return (system_prompt.to_string(), user_prompt.to_string());
    };
    let system_len = system_prompt.chars().count();
    if system_len >= limit {
        return (tail_chars(system_prompt, limit), String::new());
    }
    let user_limit = limit - system_len;
    (
        system_prompt.to_string(),
        tail_chars(user_prompt, user_limit),
    )
}

fn tail_chars(value: &str, limit: usize) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= limit {
        return value.to_string();
    }
    chars[chars.len() - limit..].iter().collect()
}

fn request_openai_json(
    settings: &LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<LlmJsonResponse> {
    let endpoint = endpoint(&settings.base_url, "chat/completions");
    let body = openai_chat_body(settings, system_prompt, user_prompt, true);
    let response = post_openai_json(&endpoint, &settings.api_key, &body)?;
    let content = response
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "OpenAI-compatible response missing content\nraw_llm_response_json:\n{response}"
            )
        })?;
    Ok(LlmJsonResponse {
        content: parse_json_content_with_provider_response(content, &response)?,
        used_provider: settings.provider.clone(),
    })
}

fn request_openai_text(
    settings: &LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<LlmTextResponse> {
    let endpoint = endpoint(&settings.base_url, "chat/completions");
    let body = openai_chat_body(settings, system_prompt, user_prompt, false);
    let response = post_openai_json(&endpoint, &settings.api_key, &body)?;
    let content = response
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("OpenAI-compatible response missing content"))?;
    Ok(LlmTextResponse {
        content: content.to_string(),
        used_provider: settings.provider.clone(),
    })
}

fn request_anthropic_json(
    settings: &LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<LlmJsonResponse> {
    let endpoint = endpoint(&settings.base_url, "messages");
    let body = anthropic_messages_body(settings, system_prompt, user_prompt);
    let response = post_anthropic_json(&endpoint, &settings.api_key, &body)?;
    let content = response
        .get("content")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find_map(|item| item.get("text").and_then(serde_json::Value::as_str))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Anthropic-compatible response missing text content\nraw_llm_response_json:\n{response}"
            )
        })?;
    Ok(LlmJsonResponse {
        content: parse_json_content_with_provider_response(content, &response)?,
        used_provider: settings.provider.clone(),
    })
}

fn request_anthropic_text(
    settings: &LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<LlmTextResponse> {
    let endpoint = endpoint(&settings.base_url, "messages");
    let body = anthropic_messages_body(settings, system_prompt, user_prompt);
    let response = post_anthropic_json(&endpoint, &settings.api_key, &body)?;
    let content = response
        .get("content")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find_map(|item| item.get("text").and_then(serde_json::Value::as_str))
        })
        .ok_or_else(|| anyhow::anyhow!("Anthropic-compatible response missing text content"))?;
    Ok(LlmTextResponse {
        content: content.to_string(),
        used_provider: settings.provider.clone(),
    })
}

fn http_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(llm_http_timeout())
            .build()
            .expect("failed to build LLM HTTP client")
    })
}

fn llm_http_timeout() -> Duration {
    Duration::from_secs(300)
}

fn post_openai_json(
    endpoint: &str,
    api_key: &str,
    body: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    send_json_with_retry(|| {
        http_client()
            .post(endpoint)
            .bearer_auth(api_key)
            .json(body)
            .send()
    })
}

fn post_anthropic_json(
    endpoint: &str,
    api_key: &str,
    body: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    send_json_with_retry(|| {
        http_client()
            .post(endpoint)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(body)
            .send()
    })
}

fn send_json_with_retry<F>(mut send: F) -> anyhow::Result<serde_json::Value>
where
    F: FnMut() -> reqwest::Result<reqwest::blocking::Response>,
{
    let mut last_error: Option<anyhow::Error> = None;
    for attempt in 0..3 {
        match send() {
            Ok(response) => {
                let status = response.status();
                if should_retry_status(status) && attempt < 2 {
                    std::thread::sleep(retry_delay(status, response.headers(), attempt));
                    continue;
                }
                return Ok(response.error_for_status()?.json()?);
            }
            Err(error) if attempt < 2 => {
                last_error = Some(error.into());
                std::thread::sleep(exponential_delay(attempt));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("LLM request failed")))
}

fn should_retry_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn retry_delay(
    _status: StatusCode,
    headers: &reqwest::header::HeaderMap,
    attempt: usize,
) -> Duration {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| exponential_delay(attempt))
}

fn exponential_delay(attempt: usize) -> Duration {
    Duration::from_millis(500 * 2u64.pow(attempt as u32))
}

fn endpoint(base_url: &str, suffix: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with(suffix) {
        trimmed.to_string()
    } else if suffix == "messages" && trimmed.ends_with("/v1") {
        format!("{trimmed}/messages")
    } else if suffix == "messages" {
        format!("{trimmed}/v1/messages")
    } else {
        format!("{trimmed}/{suffix}")
    }
}

fn parse_json_content(content: &str) -> anyhow::Result<serde_json::Value> {
    if let Ok(value) = serde_json::from_str(content) {
        return Ok(value);
    }
    let start = content.find('{').ok_or_else(|| {
        raw_llm_response_error("LLM response did not contain JSON object", content)
    })?;
    let end = content.rfind('}').ok_or_else(|| {
        raw_llm_response_error("LLM response did not contain complete JSON object", content)
    })?;
    serde_json::from_str(&content[start..=end]).map_err(|error| {
        anyhow::anyhow!("LLM response JSON parse failed: {error}\nraw_llm_response:\n{content}")
    })
}

fn raw_llm_response_error(message: &str, content: &str) -> anyhow::Error {
    anyhow::anyhow!("{message}\nraw_llm_response:\n{content}")
}

fn parse_json_content_with_provider_response(
    content: &str,
    provider_response: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    parse_json_content(content)
        .map_err(|error| anyhow::anyhow!("{error:#}\nraw_llm_response_json:\n{provider_response}"))
}

