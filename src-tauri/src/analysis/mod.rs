use crate::llm::prompts::{session_transcript, session_user_transcript, strip_llm_think_blocks};
use crate::models::AppPaths;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub projects_updated: usize,
    pub tasks_created: usize,
    pub sessions_written: usize,
}

include!("jobs.rs");
include!("session.rs");
include!("entity.rs");
include!("memory_search.rs");
include!("code_context.rs");
include!("tests.rs");
