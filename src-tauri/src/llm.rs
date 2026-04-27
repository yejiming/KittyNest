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
    if let Some(response) = test_support::next_json_response(system_prompt, user_prompt) {
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
    if let Some(response) = test_support::next_markdown_response(system_prompt, user_prompt) {
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

fn request_openai_json(
    settings: &LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<LlmJsonResponse> {
    let endpoint = endpoint(&settings.base_url, "chat/completions");
    let body = serde_json::json!({
        "model": settings.model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "temperature": 0.2,
        "response_format": {"type": "json_object"}
    });
    let response = post_openai_json(&endpoint, &settings.api_key, &body)?;
    let content = response
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("OpenAI-compatible response missing content"))?;
    Ok(LlmJsonResponse {
        content: parse_json_content(content)?,
        used_provider: settings.provider.clone(),
    })
}

fn request_openai_text(
    settings: &LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<LlmTextResponse> {
    let endpoint = endpoint(&settings.base_url, "chat/completions");
    let body = serde_json::json!({
        "model": settings.model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "temperature": 0.2
    });
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
    let body = serde_json::json!({
        "model": settings.model,
        "max_tokens": 4096,
        "system": system_prompt,
        "messages": [
            {"role": "user", "content": user_prompt}
        ]
    });
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
    Ok(LlmJsonResponse {
        content: parse_json_content(content)?,
        used_provider: settings.provider.clone(),
    })
}

fn request_anthropic_text(
    settings: &LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<LlmTextResponse> {
    let endpoint = endpoint(&settings.base_url, "messages");
    let body = serde_json::json!({
        "model": settings.model,
        "max_tokens": 4096,
        "system": system_prompt,
        "messages": [
            {"role": "user", "content": user_prompt}
        ]
    });
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
            .timeout(Duration::from_secs(60))
            .build()
            .expect("failed to build LLM HTTP client")
    })
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
    let start = content
        .find('{')
        .ok_or_else(|| anyhow::anyhow!("LLM response did not contain JSON object"))?;
    let end = content
        .rfind('}')
        .ok_or_else(|| anyhow::anyhow!("LLM response did not contain complete JSON object"))?;
    Ok(serde_json::from_str(&content[start..=end])?)
}

#[cfg(test)]
pub mod test_support {
    use std::{
        collections::VecDeque,
        sync::{Mutex, MutexGuard, OnceLock},
    };

    use super::{LlmJsonResponse, LlmTextResponse};

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct MockRequest {
        pub kind: String,
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
        system_prompt: &str,
        user_prompt: &str,
    ) -> Option<LlmJsonResponse> {
        let mut state = state().lock().ok()?;
        let content = state.json_responses.pop_front()?;
        state.requests.push(MockRequest {
            kind: "json".into(),
            system_prompt: system_prompt.into(),
            user_prompt: user_prompt.into(),
        });
        Some(LlmJsonResponse {
            content,
            used_provider: "mock".into(),
        })
    }

    pub(crate) fn next_markdown_response(
        system_prompt: &str,
        user_prompt: &str,
    ) -> Option<LlmTextResponse> {
        let mut state = state().lock().ok()?;
        let content = state.markdown_responses.pop_front()?;
        state.requests.push(MockRequest {
            kind: "markdown".into(),
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
}
