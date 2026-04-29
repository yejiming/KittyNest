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
        markdown_responses_by_prompt: Vec<(String, String)>,
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
        state.markdown_responses_by_prompt.clear();
        state.requests.clear();
    }

    pub fn set_markdown_responses_by_prompt(responses: Vec<(&str, &str)>) {
        let mut state = state().lock().expect("mock LLM state poisoned");
        state.markdown_responses.clear();
        state.markdown_responses_by_prompt = responses
            .into_iter()
            .map(|(needle, response)| (needle.to_string(), response.to_string()))
            .collect();
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
        let content = if let Some(index) = state
            .markdown_responses_by_prompt
            .iter()
            .position(|(needle, _)| system_prompt.contains(needle) || user_prompt.contains(needle))
        {
            state.markdown_responses_by_prompt.remove(index).1
        } else {
            state.markdown_responses.pop_front()?
        };
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

