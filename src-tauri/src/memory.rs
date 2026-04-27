use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::models::{AppPaths, StoredSession};

#[derive(Clone, Debug, PartialEq)]
pub struct SessionMemoryDraft {
    pub memories: Vec<String>,
    pub entities: Vec<MemoryEntity>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct MemoryEntity {
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
}

pub fn session_memory_system_prompt() -> &'static str {
    "Return only JSON with session_title, summary, memories, and entities. Use the same language as the session transcript for all human-facing fields. memories must be an array of short strings, where each string is one key fact or user preference. Keep every memory brief. entities must be an array of {name, type}. Do not include relations or legacy task fields."
}

pub fn session_memory_rebuild_system_prompt() -> &'static str {
    "Return only JSON with memories and entities. Use the same language as the session transcript for all human-facing fields. memories must be an array of short strings, where each string is one key fact or user preference. Keep every memory brief. entities must be an array of {name, type}. Do not include relations."
}

pub fn session_memory_user_prompt(session: &StoredSession, transcript: &str) -> String {
    format!(
        "Analyze this agent session using only these user and assistant messages.\n\nProject: {}\nSession: {}\nWorkdir: {}\n\nTranscript:\n\n{}",
        session.project_slug, session.session_id, session.workdir, transcript
    )
}

pub fn session_memory_from_json(value: &serde_json::Value) -> anyhow::Result<SessionMemoryDraft> {
    let memories = serde_json::from_value::<Vec<String>>(
        value
            .get("memories")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("LLM JSON missing required array field `memories`"))?,
    )?
    .into_iter()
    .map(|memory| memory.trim().to_string())
    .filter(|memory| !memory.is_empty())
    .collect::<Vec<_>>();
    let entities = serde_json::from_value::<Vec<MemoryEntity>>(
        value
            .get("entities")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("LLM JSON missing required array field `entities`"))?,
    )?;
    Ok(SessionMemoryDraft { memories, entities })
}

pub fn generate_session_memory(
    paths: &AppPaths,
    connection: &rusqlite::Connection,
    session: &StoredSession,
    draft: &SessionMemoryDraft,
) -> anyhow::Result<PathBuf> {
    generate_session_memory_at(
        paths,
        connection,
        session,
        draft,
        &crate::utils::now_rfc3339(),
    )
}

pub fn generate_session_memory_at(
    paths: &AppPaths,
    connection: &rusqlite::Connection,
    session: &StoredSession,
    draft: &SessionMemoryDraft,
    updated_at: &str,
) -> anyhow::Result<PathBuf> {
    let session_slug = crate::utils::slugify_lower(&session.session_id);
    let memory_dir = paths.memories_dir.join("sessions").join(session_slug);
    std::fs::create_dir_all(&memory_dir)?;
    let memory_path = session_memory_path(paths, &session.session_id);
    let mut content = draft.memories.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    std::fs::write(&memory_path, content)?;
    crate::db::replace_session_memories_at(connection, session, &draft.memories, updated_at)?;
    crate::graph::write_session_graph(paths, session, &draft.entities)?;
    Ok(memory_path)
}

pub fn delete_session_memory_file(paths: &AppPaths, session: &StoredSession) -> anyhow::Result<()> {
    let memory_path = session_memory_path(paths, &session.session_id);
    match std::fs::remove_file(memory_path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn session_memory_path(paths: &AppPaths, session_id: &str) -> PathBuf {
    paths
        .memories_dir
        .join("sessions")
        .join(crate::utils::slugify_lower(session_id))
        .join("memory.md")
}

#[cfg(test)]
mod tests {
    use super::session_memory_from_json;

    #[test]
    fn session_memory_from_json_requires_memories_and_entities_only() {
        let draft = session_memory_from_json(&serde_json::json!({
            "memories": ["Uses entity-only graph.", "Prefers short facts."],
            "entities": [{"name": "SQLite", "type": "technology"}]
        }))
        .unwrap();

        assert_eq!(
            draft.memories,
            vec![
                "Uses entity-only graph.".to_string(),
                "Prefers short facts.".to_string()
            ]
        );
        assert_eq!(draft.entities[0].name, "SQLite");
        assert_eq!(draft.entities[0].entity_type, "technology");
    }
}
