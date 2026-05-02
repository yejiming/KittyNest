use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub provider: String,
    pub base_url: String,
    pub interface: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RawMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RawSession {
    pub source: String,
    pub session_id: String,
    pub workdir: String,
    pub created_at: String,
    pub updated_at: String,
    pub raw_path: String,
    pub messages: Vec<RawMessage>,
}

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub config_path: PathBuf,
    pub db_path: PathBuf,
    pub projects_dir: PathBuf,
    pub memories_dir: PathBuf,
}

impl AppPaths {
    pub fn from_data_dir(data_dir: PathBuf) -> Self {
        Self {
            config_path: data_dir.join("config.toml"),
            db_path: data_dir.join("kittynest.sqlite"),
            projects_dir: data_dir.join("projects"),
            memories_dir: data_dir.join("memories"),
            data_dir,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmModelSettings {
    pub id: String,
    pub remark: String,
    pub provider: String,
    pub base_url: String,
    pub interface: String,
    pub model: String,
    pub api_key: String,
    #[serde(default)]
    pub max_context: usize,
    #[serde(default)]
    pub max_tokens: usize,
    #[serde(default)]
    pub temperature: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LlmScenarioModels {
    pub default_model: String,
    pub project_model: String,
    pub session_model: String,
    pub memory_model: String,
    #[serde(default, alias = "taskModel")]
    pub assistant_model: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettings {
    pub id: String,
    pub remark: String,
    pub provider: String,
    pub base_url: String,
    pub interface: String,
    pub model: String,
    pub api_key: String,
    pub max_context: usize,
    pub max_tokens: usize,
    pub temperature: f64,
    pub models: Vec<LlmModelSettings>,
    pub scenario_models: LlmScenarioModels,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub slug: String,
    pub display_title: String,
    pub workdir: String,
    pub sources: Vec<String>,
    pub info_path: Option<String>,
    pub progress_path: Option<String>,
    pub user_preference_path: Option<String>,
    pub agents_path: Option<String>,
    pub review_status: String,
    pub last_reviewed_at: Option<String>,
    pub last_session_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub project_slug: String,
    pub slug: String,
    pub title: String,
    pub brief: String,
    pub status: String,
    pub summary_path: String,
    pub description_path: Option<String>,
    pub session_path: Option<String>,
    pub session_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub source: String,
    pub session_id: String,
    pub raw_path: String,
    pub project_slug: String,
    pub task_slug: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub summary_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub active_projects: usize,
    pub open_tasks: usize,
    pub sessions: usize,
    pub unprocessed_sessions: usize,
    pub memories: usize,
    pub entities: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCallCount {
    pub provider: String,
    pub calls: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub source: String,
    pub path: String,
    pub exists: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppStateDto {
    pub data_dir: String,
    pub llm_settings: LlmSettings,
    pub llm_provider_calls: Vec<ProviderCallCount>,
    pub provider_presets: Vec<ProviderPreset>,
    pub source_statuses: Vec<SourceStatus>,
    pub stats: DashboardStats,
    pub projects: Vec<ProjectRecord>,
    pub tasks: Vec<TaskRecord>,
    pub sessions: Vec<SessionRecord>,
    pub jobs: Vec<JobRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTimelinePayload {
    pub version: usize,
    pub session_id: String,
    pub project_slug: String,
    pub messages: Vec<serde_json::Value>,
    pub todos: Vec<serde_json::Value>,
    pub context: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedAgentSessionPayload {
    pub version: usize,
    pub session_id: String,
    pub project_slug: String,
    pub project_root: String,
    pub created_at: String,
    pub messages: Vec<serde_json::Value>,
    pub todos: Vec<serde_json::Value>,
    pub context: serde_json::Value,
    pub llm_messages: Vec<serde_json::Value>,
}

#[derive(Clone, Debug)]
pub struct StoredSession {
    pub id: i64,
    pub source: String,
    pub session_id: String,
    pub project_id: i64,
    pub project_slug: String,
    pub task_id: Option<i64>,
    pub workdir: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<RawMessage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectSessionSummary {
    pub session_id: String,
    pub title: String,
    pub summary: String,
    pub task_slug: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JobRecord {
    pub id: i64,
    pub kind: String,
    pub scope: String,
    pub session_id: Option<String>,
    pub project_slug: Option<String>,
    pub task_slug: Option<String>,
    pub updated_after: Option<String>,
    pub status: String,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub pending: usize,
    pub message: String,
    pub started_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueJobResult {
    pub job_id: i64,
    pub total: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskResult {
    pub project_slug: String,
    pub task_slug: String,
    pub job_id: i64,
    pub total: usize,
    pub user_prompt_path: String,
    pub llm_prompt_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemorySearchRecord {
    pub id: i64,
    pub job_id: i64,
    pub query: String,
    pub status: String,
    pub message: String,
    pub created_at: String,
    pub updated_at: String,
    pub results: Vec<MemorySearchResultRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemorySearchResultRecord {
    pub source_session: String,
    pub session_title: String,
    pub project_slug: String,
    pub memory: String,
    pub ordinal: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionMemoryDetail {
    pub session_id: String,
    pub memory_path: String,
    pub memories: Vec<String>,
    pub related_sessions: Vec<MemoryRelatedSession>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRelatedSession {
    pub session_id: String,
    pub title: String,
    pub project_slug: String,
    pub shared_entities: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsidianVault {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub vault_path: Option<String>,
    pub auto_sync: bool,
    pub delete_removed: bool,
    pub last_sync_at: Option<String>,
    pub total_synced: usize,
    pub kind_counts: SyncKindCounts,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncKindCounts {
    pub projects: usize,
    pub sessions: usize,
    pub tasks: usize,
    pub memories: usize,
    pub entities: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub unchanged: usize,
}
