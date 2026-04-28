use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
};

use serde::{Deserialize, Serialize};

pub mod context;
pub mod llm;
pub mod tools;

use self::{
    context::{estimate_context, AgentStoredMessage, ThinkBlockStreamFilter, ThinkStreamEvent},
    tools::{AgentTodo, PermissionDecision, ToolEnvironment},
};

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
    run_id: usize,
    messages: Vec<AgentStoredMessage>,
    llm_messages: Vec<serde_json::Value>,
    todos: Vec<AgentTodo>,
    pending_permissions: HashMap<String, Arc<PendingPermission>>,
    pending_ask_user: HashMap<String, Arc<PendingAskUser>>,
    pending_create_task: HashMap<String, Arc<PendingAskUser>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionSnapshot {
    pub messages: Vec<AgentStoredMessage>,
    pub llm_messages: Vec<serde_json::Value>,
    pub todos: Vec<AgentTodo>,
}

#[derive(Default)]
struct PendingPermission {
    response: Mutex<Option<PermissionDecision>>,
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
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned");
        sessions.entry(session_id.into()).or_default();
    }

    pub fn stop_run(&self, session_id: &str) -> bool {
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned");
        let session = sessions.entry(session_id.into()).or_default();
        session.cancelled = true;
        for pending in session.pending_permissions.values() {
            let mut response = pending
                .response
                .lock()
                .expect("permission response lock poisoned");
            if response.is_none() {
                *response = Some(PermissionDecision {
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

    pub fn clear_session(&self, session_id: &str) {
        self.stop_run(session_id);
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned");
        sessions.insert(session_id.into(), AgentSession::default());
    }

    pub fn session_export(&self, session_id: &str) -> AgentSessionSnapshot {
        self.inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned")
            .get(session_id)
            .map(|session| AgentSessionSnapshot {
                messages: session.messages.clone(),
                llm_messages: session.llm_messages.clone(),
                todos: session.todos.clone(),
            })
            .unwrap_or_default()
    }

    pub fn session_import(&self, session_id: &str, snapshot: AgentSessionSnapshot) {
        self.stop_run(session_id);
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned");
        sessions.insert(
            session_id.into(),
            AgentSession {
                messages: snapshot.messages,
                llm_messages: snapshot.llm_messages,
                todos: snapshot.todos,
                ..AgentSession::default()
            },
        );
    }

    pub fn resolve_permission(
        &self,
        session_id: &str,
        request_id: &str,
        value: &str,
        supplemental_info: &str,
    ) -> bool {
        let pending = {
            let sessions = self
                .inner
                .sessions
                .lock()
                .expect("agent sessions lock poisoned");
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
            .expect("permission response lock poisoned") = Some(PermissionDecision {
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
            let sessions = self
                .inner
                .sessions
                .lock()
                .expect("agent sessions lock poisoned");
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

    pub fn resolve_create_task(
        &self,
        session_id: &str,
        request_id: &str,
        accepted: bool,
    ) -> bool {
        let pending = {
            let sessions = self
                .inner
                .sessions
                .lock()
                .expect("agent sessions lock poisoned");
            sessions
                .get(session_id)
                .and_then(|session| session.pending_create_task.get(request_id).cloned())
        };
        let Some(pending) = pending else {
            return false;
        };
        *pending
            .response
            .lock()
            .expect("create_task response lock poisoned") = Some(serde_json::json!({
            "accepted": accepted
        }));
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
            let mut sessions = self
                .inner
                .sessions
                .lock()
                .expect("agent sessions lock poisoned");
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

    pub fn run_with_llm_for_tests<F>(
        &self,
        session_id: &str,
        project_root: PathBuf,
        project_summary_root: PathBuf,
        settings: crate::models::LlmSettings,
        user_input: &str,
        mut llm: F,
    ) where
        F: FnMut(
            Vec<serde_json::Value>,
            Vec<serde_json::Value>,
            &mut dyn FnMut(&str),
        ) -> anyhow::Result<llm::AssistantLlmResponse>,
    {
        if let Err(error) = self.run_inner(
            session_id,
            project_root,
            project_summary_root,
            settings,
            user_input,
            &mut llm,
        ) {
            self.emit_error(session_id, &error.to_string(), None);
        }
    }

    pub fn start_run(
        &self,
        session_id: String,
        project_root: PathBuf,
        project_summary_root: PathBuf,
        settings: crate::models::LlmSettings,
        message: String,
    ) {
        let registry = self.clone();
        std::thread::spawn(move || {
            let request_settings = settings.clone();
            registry.run_with_llm_for_tests(
                &session_id,
                project_root,
                project_summary_root,
                settings,
                &message,
                move |messages, tools, on_token| {
                    llm::request_openai_stream(&request_settings, messages, tools, |token| {
                        on_token(token)
                    })
                },
            );
        });
    }

    fn run_inner<F>(
        &self,
        session_id: &str,
        project_root: PathBuf,
        project_summary_root: PathBuf,
        settings: crate::models::LlmSettings,
        user_input: &str,
        llm: &mut F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(
            Vec<serde_json::Value>,
            Vec<serde_json::Value>,
            &mut dyn FnMut(&str),
        ) -> anyhow::Result<llm::AssistantLlmResponse>,
    {
        let run_id = self.begin_user_turn(session_id, user_input);
        let system_prompt = assistant_system_prompt(&project_summary_root);
        let max_context = if settings.max_context == 0 {
            128_000
        } else {
            settings.max_context
        };

        for _ in 0..50 {
            if !self.is_current_run(session_id, run_id) {
                return Ok(());
            }
            if self.is_cancelled(session_id) {
                self.emit_cancelled(session_id, &system_prompt, max_context);
                return Ok(());
            }

            let messages = self.openai_messages(session_id, &system_prompt);
            let tools = tools::tool_schemas();
            let mut think_filter = ThinkBlockStreamFilter::default();
            let token_registry = self.clone();
            let token_session_id = session_id.to_string();
            let mut on_token = |token: &str| {
                for event in think_filter.consume(token) {
                    match event {
                        ThinkStreamEvent::Visible(delta) => {
                            let mut payload = AgentEvent::new(&token_session_id, "token");
                            payload.delta = Some(delta);
                            token_registry.emit(payload);
                        }
                        ThinkStreamEvent::ThinkingStatus(status) => {
                            let mut payload = AgentEvent::new(&token_session_id, "thinking_status");
                            payload.status = Some(status);
                            token_registry.emit(payload);
                        }
                        ThinkStreamEvent::ThinkingDelta(delta) => {
                            let mut payload = AgentEvent::new(&token_session_id, "thinking_delta");
                            payload.delta = Some(delta);
                            token_registry.emit(payload);
                        }
                    }
                }
            };

            let response = llm(messages, tools, &mut on_token)?;
            if !self.is_current_run(session_id, run_id) {
                return Ok(());
            }
            if self.is_cancelled(session_id) {
                self.emit_cancelled(session_id, &system_prompt, max_context);
                return Ok(());
            }
            if think_filter.needs_finish_event() {
                let mut payload = AgentEvent::new(session_id, "thinking_status");
                payload.status = Some("finished".into());
                self.emit(payload);
            }

            if response.tool_calls.is_empty() {
                self.append_assistant_message(session_id, &response.content);
                let reply = strip_think_blocks(&response.content).trim().to_string();
                let mut payload = AgentEvent::new(session_id, "done");
                payload.reply = Some(reply);
                payload.context = Some(self.context_value(session_id, &system_prompt, max_context));
                self.emit(payload);
                return Ok(());
            }

            self.append_assistant_tool_call_message(
                session_id,
                &response.content,
                &response.reasoning_content,
                &response.tool_calls,
            );

            for tool_call in response.tool_calls {
                if self.is_cancelled(session_id) {
                    self.emit_cancelled(session_id, &system_prompt, max_context);
                    return Ok(());
                }
                let show_tool_card = !matches!(tool_call.name.as_str(), "todo_write" | "ask_user");
                if show_tool_card {
                    let mut payload = AgentEvent::new(session_id, "tool_start");
                    payload.tool_call_id = Some(tool_call.id.clone());
                    payload.name = Some(tool_call.name.clone());
                    payload.arguments = Some(tool_call.arguments.clone());
                    payload.summary =
                        Some(summarize_tool_call(&tool_call.name, &tool_call.arguments));
                    self.emit(payload);
                }

                let mut env =
                    ToolEnvironment::new(project_root.clone(), project_summary_root.clone());
                env.session_id = session_id.to_string();
                env.todos = self.todos(session_id);
                let permission_registry = self.clone();
                let permission_session_id = session_id.to_string();
                env.set_permission_handler(move |title, description| {
                    permission_registry.request_permission_wait(
                        &permission_session_id,
                        title,
                        description,
                    )
                });
                let ask_registry = self.clone();
                let ask_session_id = session_id.to_string();
                env.set_ask_user_handler(move |questions| {
                    ask_registry.request_ask_user_wait(&ask_session_id, questions)
                });
                let create_task_registry = self.clone();
                let create_task_session_id = session_id.to_string();
                env.set_create_task_handler(move |proposal| {
                    create_task_registry.request_create_task_wait(&create_task_session_id, proposal)
                });

                let result =
                    tools::execute_tool(&tool_call.name, tool_call.arguments.clone(), &mut env);
                if !self.is_current_run(session_id, run_id) {
                    return Ok(());
                }
                if self.is_cancelled(session_id) {
                    self.emit_cancelled(session_id, &system_prompt, max_context);
                    return Ok(());
                }
                self.set_todos(session_id, env.todos);
                self.append_tool_result(session_id, &tool_call.id, &result);

                if tool_call.name == "todo_write" {
                    let mut payload = AgentEvent::new(session_id, "todo_update");
                    payload.todos = Some(serde_json::to_value(self.todos(session_id))?);
                    self.emit(payload);
                } else if show_tool_card {
                    let mut output = AgentEvent::new(session_id, "tool_output");
                    output.tool_call_id = Some(tool_call.id.clone());
                    output.delta = Some(result.clone());
                    self.emit(output);

                    let mut end = AgentEvent::new(session_id, "tool_end");
                    end.tool_call_id = Some(tool_call.id);
                    end.name = Some(tool_call.name.clone());
                    end.status = Some(if result.starts_with("Error") {
                        "error".into()
                    } else {
                        "done".into()
                    });
                    end.result_preview = Some(preview_tool_result(&result));
                    self.emit(end);
                }
            }
        }

        let mut payload = AgentEvent::new(session_id, "done");
        payload.reply = Some("(reached maximum tool-call rounds)".into());
        payload.context = Some(self.context_value(session_id, &system_prompt, max_context));
        self.emit(payload);
        Ok(())
    }

    fn begin_user_turn(&self, session_id: &str, user_input: &str) -> usize {
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned");
        let session = sessions.entry(session_id.into()).or_default();
        session.run_id += 1;
        let run_id = session.run_id;
        session.cancelled = false;
        prune_incomplete_tool_call_messages(&mut session.llm_messages);
        session
            .messages
            .push(AgentStoredMessage::new("user", user_input));
        session
            .llm_messages
            .push(serde_json::json!({"role": "user", "content": user_input}));
        run_id
    }

    fn is_current_run(&self, session_id: &str, run_id: usize) -> bool {
        self.inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned")
            .get(session_id)
            .is_some_and(|session| session.run_id == run_id)
    }

    fn openai_messages(&self, session_id: &str, system_prompt: &str) -> Vec<serde_json::Value> {
        let sessions = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned");
        let mut messages = vec![serde_json::json!({"role": "system", "content": system_prompt})];
        if let Some(session) = sessions.get(session_id) {
            messages.extend(session.llm_messages.clone());
        }
        messages
    }

    fn append_assistant_message(&self, session_id: &str, content: &str) {
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned");
        let session = sessions.entry(session_id.into()).or_default();
        session
            .messages
            .push(AgentStoredMessage::new("assistant", content));
        session
            .llm_messages
            .push(serde_json::json!({"role": "assistant", "content": content}));
    }

    fn append_assistant_tool_call_message(
        &self,
        session_id: &str,
        content: &str,
        reasoning_content: &str,
        tool_calls: &[llm::AssistantToolCall],
    ) {
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned");
        let session = sessions.entry(session_id.into()).or_default();
        if !content.trim().is_empty() {
            session
                .messages
                .push(AgentStoredMessage::new("assistant", content));
        }
        let tool_calls = tool_calls
            .iter()
            .map(|tool_call| {
                serde_json::json!({
                    "id": tool_call.id,
                    "type": "function",
                    "function": {
                        "name": tool_call.name,
                        "arguments": tool_call.arguments.to_string()
                    }
                })
            })
            .collect::<Vec<_>>();
        let mut assistant_message = serde_json::json!({
            "role": "assistant",
            "content": if content.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(content.into()) },
            "tool_calls": tool_calls
        });
        if !reasoning_content.is_empty() {
            assistant_message["reasoning_content"] =
                serde_json::Value::String(reasoning_content.into());
        }
        session.llm_messages.push(assistant_message);
    }

    fn append_tool_result(&self, session_id: &str, tool_call_id: &str, result: &str) {
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned");
        let session = sessions.entry(session_id.into()).or_default();
        session
            .messages
            .push(AgentStoredMessage::new("tool", result));
        session.llm_messages.push(serde_json::json!({
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": result
        }));
    }

    fn todos(&self, session_id: &str) -> Vec<AgentTodo> {
        self.inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned")
            .get(session_id)
            .map(|session| session.todos.clone())
            .unwrap_or_default()
    }

    fn set_todos(&self, session_id: &str, todos: Vec<AgentTodo>) {
        let mut sessions = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned");
        sessions.entry(session_id.into()).or_default().todos = todos;
    }

    fn request_permission_wait(
        &self,
        session_id: &str,
        title: &str,
        description: &str,
    ) -> PermissionDecision {
        let request_id = self.create_permission_request(session_id, title, description);
        let pending = {
            let sessions = self
                .inner
                .sessions
                .lock()
                .expect("agent sessions lock poisoned");
            sessions
                .get(session_id)
                .and_then(|session| session.pending_permissions.get(&request_id).cloned())
        };
        let Some(pending) = pending else {
            return PermissionDecision {
                value: "deny".into(),
                supplemental_info: "Permission request was interrupted.".into(),
            };
        };
        let mut response = pending
            .response
            .lock()
            .expect("permission response lock poisoned");
        while response.is_none() {
            response = pending
                .available
                .wait(response)
                .expect("permission response lock poisoned");
        }
        let decision = response.take().unwrap_or(PermissionDecision {
            value: "deny".into(),
            supplemental_info: "Permission request was interrupted.".into(),
        });
        if let Some(session) = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned")
            .get_mut(session_id)
        {
            session.pending_permissions.remove(&request_id);
        }
        decision
    }

    fn request_ask_user_wait(
        &self,
        session_id: &str,
        questions: Vec<serde_json::Value>,
    ) -> serde_json::Value {
        let request_id = self.next_request_id();
        let pending = Arc::new(PendingAskUser::default());
        {
            let mut sessions = self
                .inner
                .sessions
                .lock()
                .expect("agent sessions lock poisoned");
            sessions
                .entry(session_id.into())
                .or_default()
                .pending_ask_user
                .insert(request_id.clone(), pending.clone());
        }
        let mut event = AgentEvent::new(session_id, "ask_user_request");
        event.request_id = Some(request_id.clone());
        event.title = Some("Need your input".into());
        event.questions = Some(serde_json::Value::Array(questions));
        self.emit(event);

        let mut response = pending
            .response
            .lock()
            .expect("ask_user response lock poisoned");
        while response.is_none() {
            response = pending
                .available
                .wait(response)
                .expect("ask_user response lock poisoned");
        }
        let answers = response.take().unwrap_or_else(|| serde_json::json!({}));
        if let Some(session) = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned")
            .get_mut(session_id)
        {
            session.pending_ask_user.remove(&request_id);
        }
        answers
    }

    fn request_create_task_wait(
        &self,
        session_id: &str,
        proposal: serde_json::Value,
    ) -> serde_json::Value {
        let request_id = self.next_request_id();
        let pending = Arc::new(PendingAskUser::default());
        {
            let mut sessions = self
                .inner
                .sessions
                .lock()
                .expect("agent sessions lock poisoned");
            sessions
                .entry(session_id.into())
                .or_default()
                .pending_create_task
                .insert(request_id.clone(), pending.clone());
        }
        let mut event = AgentEvent::new(session_id, "create_task_request");
        event.request_id = Some(request_id.clone());
        event.title = proposal
            .get("taskName")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);
        event.description = proposal
            .get("taskDescription")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);
        self.emit(event);

        let mut response = pending
            .response
            .lock()
            .expect("create_task response lock poisoned");
        while response.is_none() {
            response = pending
                .available
                .wait(response)
                .expect("create_task response lock poisoned");
        }
        let answer = response
            .take()
            .unwrap_or_else(|| serde_json::json!({"accepted": false}));
        if let Some(session) = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned")
            .get_mut(session_id)
        {
            session.pending_create_task.remove(&request_id);
        }
        answer
    }

    fn emit_cancelled(&self, session_id: &str, system_prompt: &str, max_context: usize) {
        let mut payload = AgentEvent::new(session_id, "cancelled");
        payload.context = Some(self.context_value(session_id, system_prompt, max_context));
        self.emit(payload);
    }

    fn emit_error(&self, session_id: &str, error: &str, context: Option<serde_json::Value>) {
        let mut payload = AgentEvent::new(session_id, "error");
        payload.error = Some(error.into());
        payload.context = context;
        self.emit(payload);
    }

    fn context_value(
        &self,
        session_id: &str,
        system_prompt: &str,
        max_context: usize,
    ) -> serde_json::Value {
        let messages = self
            .inner
            .sessions
            .lock()
            .expect("agent sessions lock poisoned")
            .get(session_id)
            .map(|session| session.messages.clone())
            .unwrap_or_default();
        serde_json::to_value(estimate_context(system_prompt, &messages, max_context))
            .unwrap_or_else(|_| serde_json::json!({}))
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

fn assistant_system_prompt(project_summary_root: &std::path::Path) -> String {
    format!(
        "You are KittyNest Task Assistant. Help with the selected reviewed project.\nProject summary root: {}\nUse tools when reading project summary files or tracking work. Ask the user when a decision is required.",
        project_summary_root.display()
    )
}

fn strip_think_blocks(source: &str) -> String {
    let mut result = String::new();
    let mut rest = source;
    loop {
        let Some(start) = rest.find("<think>") else {
            result.push_str(rest);
            break;
        };
        result.push_str(&rest[..start]);
        let after_start = &rest[start + "<think>".len()..];
        let Some(end) = after_start.find("</think>") else {
            break;
        };
        rest = &after_start[end + "</think>".len()..];
    }
    result
}

fn prune_incomplete_tool_call_messages(messages: &mut Vec<serde_json::Value>) {
    let mut remove = vec![false; messages.len()];
    for index in 0..messages.len() {
        if messages[index]
            .get("role")
            .and_then(serde_json::Value::as_str)
            != Some("assistant")
        {
            continue;
        }
        let Some(tool_calls) = messages[index]
            .get("tool_calls")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        let expected_ids = tool_calls
            .iter()
            .filter_map(|tool_call| tool_call.get("id").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>();
        if expected_ids.is_empty() {
            continue;
        }

        let following_tool_ids = messages
            .iter()
            .skip(index + 1)
            .take_while(|message| {
                message.get("role").and_then(serde_json::Value::as_str) == Some("tool")
            })
            .filter_map(|message| {
                message
                    .get("tool_call_id")
                    .and_then(serde_json::Value::as_str)
            })
            .collect::<HashSet<_>>();
        if expected_ids
            .iter()
            .any(|tool_call_id| !following_tool_ids.contains(tool_call_id))
        {
            remove[index] = true;
            for later in index + 1..messages.len() {
                if messages[later]
                    .get("role")
                    .and_then(serde_json::Value::as_str)
                    == Some("tool")
                {
                    remove[later] = true;
                } else {
                    break;
                }
            }
        }
    }

    let mut index = 0;
    messages.retain(|_| {
        let keep = !remove[index];
        index += 1;
        keep
    });
}

fn summarize_tool_call(name: &str, arguments: &serde_json::Value) -> String {
    if let Some(object) = arguments.as_object() {
        if let Some(value) = object.values().next().and_then(serde_json::Value::as_str) {
            return format!("{name} {value}");
        }
    }
    if arguments == &serde_json::json!({}) {
        name.into()
    } else {
        format!("{name} {arguments}")
    }
}

fn preview_tool_result(content: &str) -> String {
    content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("(no output)")
        .into()
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

    #[test]
    fn clear_session_removes_messages_todos_and_context() {
        let registry = super::AgentRegistry::new_for_tests(VecEmitter::default());
        let settings = crate::config::default_llm_settings();
        let project_root = tempfile::tempdir().unwrap();
        let summary_root = tempfile::tempdir().unwrap();
        registry.run_with_llm_for_tests(
            "session-1",
            project_root.path().to_path_buf(),
            summary_root.path().to_path_buf(),
            settings,
            "hello",
            |_messages, _tools, _on_token| {
                Ok(super::llm::AssistantLlmResponse {
                    content: "world".into(),
                    reasoning_content: String::new(),
                    tool_calls: Vec::new(),
                })
            },
        );

        assert!(!registry.session_export("session-1").messages.is_empty());

        registry.clear_session("session-1");

        assert!(registry.session_export("session-1").messages.is_empty());
        assert!(registry.session_export("session-1").llm_messages.is_empty());
    }

    #[test]
    fn import_session_replaces_backend_context() {
        let registry = super::AgentRegistry::new_for_tests(VecEmitter::default());
        registry.session_import(
            "session-1",
            super::AgentSessionSnapshot {
                messages: vec![super::context::AgentStoredMessage::new("user", "loaded")],
                llm_messages: vec![serde_json::json!({"role": "user", "content": "loaded"})],
                todos: vec![super::tools::AgentTodo {
                    content: "Loaded todo".into(),
                    active_form: "Loading todo".into(),
                    status: "pending".into(),
                }],
            },
        );

        let exported = registry.session_export("session-1");

        assert_eq!(exported.messages[0].content, "loaded");
        assert_eq!(exported.llm_messages[0]["content"], "loaded");
        assert_eq!(exported.todos[0].content, "Loaded todo");
    }

    #[test]
    fn run_with_fake_llm_streams_token_and_done() {
        let emitter = VecEmitter::default();
        let registry = super::AgentRegistry::new_for_tests(emitter.clone());
        let settings = crate::config::default_llm_settings();
        let project_root = tempfile::tempdir().unwrap();
        let summary_root = tempfile::tempdir().unwrap();
        registry.run_with_llm_for_tests(
            "session-1",
            project_root.path().to_path_buf(),
            summary_root.path().to_path_buf(),
            settings,
            "hello",
            |_messages, _tools, on_token| {
                on_token("Hello");
                Ok(super::llm::AssistantLlmResponse {
                    content: "Hello".into(),
                    reasoning_content: String::new(),
                    tool_calls: Vec::new(),
                })
            },
        );

        let events = emitter.events.lock().unwrap();
        assert_eq!(events[0].event_type, "token");
        assert_eq!(events.last().unwrap().event_type, "done");
    }

    #[test]
    fn run_with_fake_llm_executes_todo_tool_as_task_list_update() {
        let emitter = VecEmitter::default();
        let registry = super::AgentRegistry::new_for_tests(emitter.clone());
        let settings = crate::config::default_llm_settings();
        let project_root = tempfile::tempdir().unwrap();
        let summary_root = tempfile::tempdir().unwrap();
        let mut rounds = 0;

        registry.run_with_llm_for_tests(
            "session-1",
            project_root.path().to_path_buf(),
            summary_root.path().to_path_buf(),
            settings,
            "plan",
            |_messages, _tools, _on_token| {
                rounds += 1;
                if rounds == 1 {
                    Ok(super::llm::AssistantLlmResponse {
                        content: String::new(),
                        reasoning_content: String::new(),
                        tool_calls: vec![super::llm::AssistantToolCall {
                            id: "todo_1".into(),
                            name: "todo_write".into(),
                            arguments: serde_json::json!({
                                "todos": [
                                    {"content": "Ship drawer", "active_form": "Shipping drawer", "status": "in_progress"}
                                ]
                            }),
                        }],
                    })
                } else {
                    Ok(super::llm::AssistantLlmResponse {
                        content: "Ready".into(),
                        reasoning_content: String::new(),
                        tool_calls: Vec::new(),
                    })
                }
            },
        );

        let events = emitter.events.lock().unwrap();
        assert!(events.iter().any(|event| event.event_type == "todo_update"));
        assert_eq!(events.last().unwrap().event_type, "done");
    }

    #[test]
    fn tool_call_history_preserves_reasoning_content() {
        let registry = super::AgentRegistry::new_for_tests(VecEmitter::default());
        let settings = crate::config::default_llm_settings();
        let project_root = tempfile::tempdir().unwrap();
        let summary_root = tempfile::tempdir().unwrap();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_messages = Arc::clone(&captured);
        let mut rounds = 0;

        registry.run_with_llm_for_tests(
            "session-1",
            project_root.path().to_path_buf(),
            summary_root.path().to_path_buf(),
            settings,
            "plan",
            move |messages, _tools, _on_token| {
                rounds += 1;
                if rounds == 1 {
                    Ok(super::llm::AssistantLlmResponse {
                        content: String::new(),
                        reasoning_content: "Need to update the todo list.".into(),
                        tool_calls: vec![super::llm::AssistantToolCall {
                            id: "todo_1".into(),
                            name: "todo_write".into(),
                            arguments: serde_json::json!({
                                "todos": [
                                    {"content": "Check DeepSeek tool calls", "active_form": "Checking DeepSeek tool calls", "status": "in_progress"}
                                ]
                            }),
                        }],
                    })
                } else {
                    *captured_messages.lock().unwrap() = messages;
                    Ok(super::llm::AssistantLlmResponse {
                        content: "Done".into(),
                        reasoning_content: String::new(),
                        tool_calls: Vec::new(),
                    })
                }
            },
        );

        let messages = captured.lock().unwrap();
        let assistant_tool_message = messages
            .iter()
            .find(|message| message.get("tool_calls").is_some())
            .unwrap();

        assert_eq!(
            assistant_tool_message["reasoning_content"],
            "Need to update the todo list."
        );
    }

    #[test]
    fn stopped_tool_call_turn_is_removed_before_next_user_message() {
        let registry = super::AgentRegistry::new_for_tests(VecEmitter::default());
        let settings = crate::config::default_llm_settings();
        let project_root = tempfile::tempdir().unwrap();
        let summary_root = tempfile::tempdir().unwrap();
        let stop_registry = registry.clone();

        registry.run_with_llm_for_tests(
            "session-1",
            project_root.path().to_path_buf(),
            summary_root.path().to_path_buf(),
            settings.clone(),
            "read a file",
            move |_messages, _tools, _on_token| {
                stop_registry.stop_run("session-1");
                Ok(super::llm::AssistantLlmResponse {
                    content: String::new(),
                    reasoning_content: String::new(),
                    tool_calls: vec![super::llm::AssistantToolCall {
                        id: "call_read".into(),
                        name: "read_file".into(),
                        arguments: serde_json::json!({"file_path": "src/App.tsx"}),
                    }],
                })
            },
        );

        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_messages = Arc::clone(&captured);
        registry.run_with_llm_for_tests(
            "session-1",
            project_root.path().to_path_buf(),
            summary_root.path().to_path_buf(),
            settings,
            "try again",
            move |messages, _tools, _on_token| {
                *captured_messages.lock().unwrap() = messages;
                Ok(super::llm::AssistantLlmResponse {
                    content: "ok".into(),
                    reasoning_content: String::new(),
                    tool_calls: Vec::new(),
                })
            },
        );

        let messages = captured.lock().unwrap();
        assert!(!messages
            .iter()
            .any(|message| message.get("tool_calls").is_some()));
        assert_eq!(messages.last().unwrap()["content"], "try again");
    }

    #[test]
    fn system_prompt_uses_summary_root_not_code_root() {
        let registry = super::AgentRegistry::new_for_tests(VecEmitter::default());
        let settings = crate::config::default_llm_settings();
        let project_root = tempfile::tempdir().unwrap();
        let summary_root = tempfile::tempdir().unwrap();
        let captured = Arc::new(Mutex::new(String::new()));
        let captured_prompt = Arc::clone(&captured);

        registry.run_with_llm_for_tests(
            "session-1",
            project_root.path().to_path_buf(),
            summary_root.path().to_path_buf(),
            settings,
            "hello",
            move |messages, _tools, _on_token| {
                *captured_prompt.lock().unwrap() = messages[0]["content"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                Ok(super::llm::AssistantLlmResponse {
                    content: "ok".into(),
                    reasoning_content: String::new(),
                    tool_calls: Vec::new(),
                })
            },
        );

        let prompt = captured.lock().unwrap();
        assert!(prompt.contains(&summary_root.path().display().to_string()));
        assert!(!prompt.contains(&project_root.path().display().to_string()));
        assert!(prompt.contains("Project summary root"));
    }
}
