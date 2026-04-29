use std::sync::OnceLock;

use tauri::{Emitter, State};

use crate::{
    models::{AppStateDto, LlmSettings, SourceStatus},
    services::AppServices,
    utils::{to_command_error, CommandResult},
};

#[derive(Clone)]
pub struct TauriAgentEmitter {
    app: tauri::AppHandle,
}

impl crate::assistant::AgentEventEmitter for TauriAgentEmitter {
    fn emit(&self, event: &crate::assistant::AgentEvent) {
        let _ = self.app.emit("agent://event", event);
    }
}

fn assistant_registry(
    app: tauri::AppHandle,
) -> &'static crate::assistant::AgentRegistry<TauriAgentEmitter> {
    static REGISTRY: OnceLock<crate::assistant::AgentRegistry<TauriAgentEmitter>> = OnceLock::new();
    REGISTRY.get_or_init(|| crate::assistant::AgentRegistry::new(TauriAgentEmitter { app }))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TaskMetadataDraft {
    pub task_name: String,
    pub task_description: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueuedAgentSessionSavePayload {
    version: usize,
    session_id: String,
    project_slug: String,
    timeline: crate::models::AgentTimelinePayload,
    llm_messages: Vec<serde_json::Value>,
}

pub(crate) fn parse_task_metadata_json(content: &str) -> anyhow::Result<TaskMetadataDraft> {
    let value: serde_json::Value = serde_json::from_str(content.trim())?;
    let task_description = value
        .get("task_description")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if task_description.is_empty() {
        anyhow::bail!("task_description is required");
    }
    let task_name = value
        .get("task_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if task_name.is_empty() {
        anyhow::bail!("task_name is required");
    }
    Ok(TaskMetadataDraft {
        task_name,
        task_description,
    })
}

include!("app_state.rs");
include!("jobs.rs");
include!("agent.rs");
include!("tasks.rs");
include!("tests.rs");
