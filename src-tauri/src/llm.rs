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

#[cfg(test)]
pub mod test_support {
    use std::{
        collections::VecDeque,
        sync::{Mutex, MutexGuard, OnceLock},
    };

    use super::{LlmJsonResponse, LlmSettings, LlmTextResponse};

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct MockRequest {
        pub kind: String,
        pub provider: String,
        pub model: String,
        pub system_prompt: String,
        pub user_prompt: String,
    }

    #[derive(Default)]
    struct MockState {
        json_responses: VecDeque<serde_json::Value>,
        markdown_responses: VecDeque<String>,
        requests: Vec<MockRequest>,
    }

    static STATE: OnceLock<Mutex<MockState>> = OnceLock::new();
    static ISOLATION: OnceLock<Mutex<()>> = OnceLock::new();

    fn state() -> &'static Mutex<MockState> {
        STATE.get_or_init(|| Mutex::new(MockState::default()))
    }

    pub fn guard() -> MutexGuard<'static, ()> {
        ISOLATION
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("mock LLM isolation lock poisoned")
    }

    pub fn clear() {
        if let Ok(mut state) = state().lock() {
            *state = MockState::default();
        }
    }

    pub fn set_json_responses(responses: Vec<serde_json::Value>) {
        let mut state = state().lock().expect("mock LLM state poisoned");
        state.json_responses = responses.into();
        state.requests.clear();
    }

    pub fn set_markdown_responses(responses: Vec<&str>) {
        let mut state = state().lock().expect("mock LLM state poisoned");
        state.markdown_responses = responses
            .into_iter()
            .map(ToString::to_string)
            .collect::<VecDeque<_>>();
        state.requests.clear();
    }

    pub fn take_requests() -> Vec<MockRequest> {
        let mut state = state().lock().expect("mock LLM state poisoned");
        std::mem::take(&mut state.requests)
    }

    pub(crate) fn next_json_response(
        settings: &LlmSettings,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Option<LlmJsonResponse> {
        let mut state = state().lock().ok()?;
        let content = state.json_responses.pop_front()?;
        state.requests.push(MockRequest {
            kind: "json".into(),
            provider: settings.provider.clone(),
            model: settings.model.clone(),
            system_prompt: system_prompt.into(),
            user_prompt: user_prompt.into(),
        });
        Some(LlmJsonResponse {
            content,
            used_provider: "mock".into(),
        })
    }

    pub(crate) fn next_markdown_response(
        settings: &LlmSettings,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Option<LlmTextResponse> {
        let mut state = state().lock().ok()?;
        let content = state.markdown_responses.pop_front()?;
        state.requests.push(MockRequest {
            kind: "markdown".into(),
            provider: settings.provider.clone(),
            model: settings.model.clone(),
            system_prompt: system_prompt.into(),
            user_prompt: user_prompt.into(),
        });
        Some(LlmTextResponse {
            content,
            used_provider: "mock".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    };

    #[test]
    fn llm_concurrency_gate_allows_at_most_five_active_permits() {
        let _guard = super::test_support::guard();
        let active = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let release = Arc::new((Mutex::new(false), Condvar::new()));

        std::thread::scope(|scope| {
            for _ in 0..6 {
                let active = Arc::clone(&active);
                let max_seen = Arc::clone(&max_seen);
                let release = Arc::clone(&release);
                scope.spawn(move || {
                    let _permit = super::acquire_llm_permit();
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(current, Ordering::SeqCst);
                    let (lock, cvar) = &*release;
                    let mut released = lock.lock().expect("release lock poisoned");
                    while !*released {
                        released = cvar.wait(released).expect("release lock poisoned");
                    }
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }

            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
            while active.load(Ordering::SeqCst) < 5 && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert_eq!(max_seen.load(Ordering::SeqCst), 5);
            assert_eq!(active.load(Ordering::SeqCst), 5);

            let (lock, cvar) = &*release;
            *lock.lock().expect("release lock poisoned") = true;
            cvar.notify_all();
        });
    }

    #[test]
    fn http_client_timeout_is_five_minutes() {
        assert_eq!(
            super::llm_http_timeout(),
            std::time::Duration::from_secs(300)
        );
    }

    #[test]
    fn json_parse_errors_include_raw_llm_response_when_content_exists() {
        let error = super::parse_json_content("I cannot produce JSON for this request.")
            .unwrap_err()
            .to_string();

        assert!(error.contains("LLM response did not contain JSON object"));
        assert!(error.contains("raw_llm_response:"));
        assert!(error.contains("I cannot produce JSON for this request."));
    }

    #[test]
    fn json_parse_errors_include_provider_response_when_content_is_empty() {
        let provider_response = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "content": ""
                    },
                    "finish_reason": "stop"
                }
            ]
        });
        let error = super::parse_json_content_with_provider_response("", &provider_response)
            .unwrap_err()
            .to_string();

        assert!(error.contains("LLM response did not contain JSON object"));
        assert!(error.contains("raw_llm_response_json:"));
        assert!(error.contains("\"finish_reason\":\"stop\""));
    }

    #[test]
    fn openai_body_maps_model_limits_temperature_and_context_budget() {
        let mut settings = crate::config::default_llm_settings();
        settings.model = "openai/gpt-4o-mini".into();
        settings.max_context = 4;
        settings.max_tokens = 123;
        settings.temperature = 0.7;

        let body = super::openai_chat_body(&settings, "system", "12345678901234567890", true);

        assert_eq!(body["max_completion_tokens"], 123);
        assert_eq!(body["max_tokens"], 123);
        assert_eq!(body["temperature"], 0.7);
        assert_eq!(body["response_format"]["type"], "json_object");
        let user_prompt = body["messages"][1]["content"].as_str().unwrap();
        assert!(user_prompt.chars().count() <= 16);
        assert!(user_prompt.contains("7890"));
    }

    #[test]
    fn anthropic_body_uses_global_limits_and_temperature() {
        let mut settings = crate::config::default_llm_settings();
        settings.model = "claude-3-5-sonnet-latest".into();
        settings.max_tokens = 321;
        settings.temperature = 0.4;

        let body = super::anthropic_messages_body(&settings, "system", "user");

        assert_eq!(body["max_tokens"], 321);
        assert_eq!(body["temperature"], 0.4);
        assert_eq!(body["system"], "system");
        assert_eq!(body["messages"][0]["content"], "user");
    }
}
