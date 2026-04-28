use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub session_id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub questions: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub todos: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

impl AgentEvent {
    fn new(session_id: &str, event_type: &str) -> Self {
        Self {
            session_id: session_id.into(),
            event_type: event_type.into(),
            delta: None,
            status: None,
            tool_call_id: None,
            name: None,
            arguments: None,
            summary: None,
            result_preview: None,
            request_id: None,
            title: None,
            description: None,
            options: None,
            questions: None,
            todos: None,
            reply: None,
            error: None,
            context: None,
        }
    }
}

pub trait AgentEventEmitter: Clone + Send + Sync + 'static {
    fn emit(&self, event: &AgentEvent);
}

#[derive(Clone)]
pub struct AgentRegistry<E: AgentEventEmitter> {
    inner: Arc<AgentRegistryInner<E>>,
}

struct AgentRegistryInner<E: AgentEventEmitter> {
    emitter: E,
    sessions: Mutex<HashMap<String, AgentSession>>,
    request_counter: AtomicUsize,
}

#[derive(Default)]
struct AgentSession {
    cancelled: bool,
    pending_permissions: HashMap<String, Arc<PendingPermission>>,
    pending_ask_user: HashMap<String, Arc<PendingAskUser>>,
}

#[derive(Default)]
struct PendingPermission {
    response: Mutex<Option<crate::assistant_tools::PermissionDecision>>,
    available: Condvar,
}

#[derive(Default)]
struct PendingAskUser {
    response: Mutex<Option<serde_json::Value>>,
    available: Condvar,
}

impl<E: AgentEventEmitter> AgentRegistry<E> {
    pub fn new(emitter: E) -> Self {
        Self {
            inner: Arc::new(AgentRegistryInner {
                emitter,
                sessions: Mutex::new(HashMap::new()),
                request_counter: AtomicUsize::new(0),
            }),
        }
    }

    pub fn new_for_tests(emitter: E) -> Self {
        Self::new(emitter)
    }

    pub fn ensure_session(&self, session_id: &str) {
        let mut sessions = self.inner.sessions.lock().expect("agent sessions lock poisoned");
        sessions.entry(session_id.into()).or_default();
    }

    pub fn stop_run(&self, session_id: &str) -> bool {
        let mut sessions = self.inner.sessions.lock().expect("agent sessions lock poisoned");
        let session = sessions.entry(session_id.into()).or_default();
        session.cancelled = true;
        for pending in session.pending_permissions.values() {
            let mut response = pending
                .response
                .lock()
                .expect("permission response lock poisoned");
            if response.is_none() {
                *response = Some(crate::assistant_tools::PermissionDecision {
                    value: "deny".into(),
                    supplemental_info: "Run stopped.".into(),
                });
            }
            pending.available.notify_all();
        }
        for pending in session.pending_ask_user.values() {
            let mut response = pending
                .response
                .lock()
                .expect("ask_user response lock poisoned");
            if response.is_none() {
                *response = Some(serde_json::json!({}));
            }
            pending.available.notify_all();
        }
        true
    }

    pub fn is_cancelled(&self, session_id: &str) -> bool {
        self.inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned")
            .get(session_id)
            .is_some_and(|session| session.cancelled)
    }

    pub fn resolve_permission(
        &self,
        session_id: &str,
        request_id: &str,
        value: &str,
        supplemental_info: &str,
    ) -> bool {
        let pending = {
            let sessions = self.inner.sessions.lock().expect("agent sessions lock poisoned");
            sessions
                .get(session_id)
                .and_then(|session| session.pending_permissions.get(request_id).cloned())
        };
        let Some(pending) = pending else {
            return false;
        };
        *pending
            .response
            .lock()
            .expect("permission response lock poisoned") =
            Some(crate::assistant_tools::PermissionDecision {
                value: value.into(),
                supplemental_info: supplemental_info.into(),
            });
        pending.available.notify_all();
        true
    }

    pub fn resolve_ask_user(
        &self,
        session_id: &str,
        request_id: &str,
        answers: serde_json::Value,
    ) -> bool {
        let pending = {
            let sessions = self.inner.sessions.lock().expect("agent sessions lock poisoned");
            sessions
                .get(session_id)
                .and_then(|session| session.pending_ask_user.get(request_id).cloned())
        };
        let Some(pending) = pending else {
            return false;
        };
        *pending
            .response
            .lock()
            .expect("ask_user response lock poisoned") = Some(answers);
        pending.available.notify_all();
        true
    }

    pub fn create_permission_request(
        &self,
        session_id: &str,
        title: &str,
        description: &str,
    ) -> String {
        let request_id = self.next_request_id();
        let pending = Arc::new(PendingPermission::default());
        {
            let mut sessions = self.inner.sessions.lock().expect("agent sessions lock poisoned");
            sessions
                .entry(session_id.into())
                .or_default()
                .pending_permissions
                .insert(request_id.clone(), pending);
        }
        let mut event = AgentEvent::new(session_id, "permission_request");
        event.request_id = Some(request_id.clone());
        event.title = Some(title.into());
        event.description = Some(description.into());
        event.options = Some(serde_json::json!([
            {"label": "Allow", "value": "allow"},
            {"label": "Deny", "value": "deny"}
        ]));
        self.emit(event);
        request_id
    }

    fn next_request_id(&self) -> String {
        let counter = self.inner.request_counter.fetch_add(1, Ordering::SeqCst) + 1;
        let timestamp = crate::utils::now_rfc3339()
            .replace(':', "")
            .replace('.', "")
            .replace('-', "");
        format!("request-{counter}-{timestamp}")
    }

    fn emit(&self, event: AgentEvent) {
        self.inner.emitter.emit(&event);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct VecEmitter {
        events: Arc<Mutex<Vec<super::AgentEvent>>>,
    }

    impl super::AgentEventEmitter for VecEmitter {
        fn emit(&self, event: &super::AgentEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    #[test]
    fn permission_request_blocks_until_resolved() {
        let emitter = VecEmitter::default();
        let registry = super::AgentRegistry::new_for_tests(emitter.clone());
        let request_id = registry.create_permission_request(
            "session-1",
            "File Permission",
            "Read outside project?",
        );

        assert_eq!(
            emitter.events.lock().unwrap()[0].event_type,
            "permission_request"
        );
        assert!(registry.resolve_permission("session-1", &request_id, "allow", ""));
    }

    #[test]
    fn stop_marks_session_cancelled() {
        let registry = super::AgentRegistry::new_for_tests(VecEmitter::default());
        registry.ensure_session("session-1");

        assert!(registry.stop_run("session-1"));
        assert!(registry.is_cancelled("session-1"));
    }
}
