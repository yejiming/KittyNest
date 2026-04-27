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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettings {
    pub provider: String,
    pub base_url: String,
    pub interface: String,
    pub model: String,
    pub api_key: String,
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
    pub session_count: usize,
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
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub source: String,
    pub path: String,
    pub exists: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppStateDto {
    pub data_dir: String,
    pub llm_settings: LlmSettings,
    pub provider_presets: Vec<ProviderPreset>,
    pub source_statuses: Vec<SourceStatus>,
    pub stats: DashboardStats,
    pub projects: Vec<ProjectRecord>,
    pub tasks: Vec<TaskRecord>,
    pub sessions: Vec<SessionRecord>,
    pub jobs: Vec<JobRecord>,
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
