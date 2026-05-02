use rusqlite::{params, params_from_iter, OptionalExtension};

use crate::models::{
    AppPaths, DashboardStats, EnqueueJobResult, JobRecord, MemorySearchRecord,
    MemorySearchResultRecord, ProjectRecord, ProjectSessionSummary, ProviderCallCount, RawMessage,
    RawSession, SessionRecord, StoredSession, TaskRecord,
};

pub const PROJECT_ANALYZE_SESSION_LIMIT: usize = 20;

pub fn open(paths: &AppPaths) -> anyhow::Result<rusqlite::Connection> {
    std::fs::create_dir_all(&paths.data_dir)?;
    Ok(rusqlite::Connection::open(&paths.db_path)?)
}

include!("schema.rs");
include!("projects.rs");
include!("sessions.rs");
include!("jobs.rs");
include!("tasks.rs");
include!("memories.rs");
include!("sync.rs");
include!("tests.rs");
