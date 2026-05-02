use crate::models::{AppPaths, LlmModelSettings, LlmScenarioModels, LlmSettings, ObsidianConfig};

const DEFAULT_MAX_CONTEXT: usize = 128_000;
const DEFAULT_MAX_TOKENS: usize = 4_096;
const DEFAULT_TEMPERATURE: f64 = 0.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LlmScenario {
    Default,
    Project,
    Session,
    Memory,
    Assistant,
}

include!("workspace.rs");
include!("settings.rs");
