fn run_memory_search_job(
    paths: &AppPaths,
    connection: &rusqlite::Connection,
    job_id: i64,
) -> anyhow::Result<usize> {
    let search = crate::db::memory_search_for_job(connection, job_id)?
        .ok_or_else(|| anyhow::anyhow!("memory search row not found for job {job_id}"))?;
    let settings = crate::config::resolve_llm_settings(
        &crate::config::read_llm_settings(paths)?,
        crate::config::LlmScenario::Memory,
    );
    let entities = memory_search_entities(paths, &settings, &search.query)?;
    let mut session_ids = std::collections::BTreeSet::new();
    for entity in &entities {
        for related in crate::graph::related_sessions_for_entity(paths, entity)? {
            session_ids.insert(related.session_id);
        }
    }
    let session_ids = session_ids.into_iter().collect::<Vec<_>>();
    let memories = crate::db::session_memories_for_sessions(connection, &session_ids)?;
    let entity_lowers = entities
        .iter()
        .map(|entity| entity.to_lowercase())
        .collect::<Vec<_>>();
    let mut results = Vec::new();
    for memory in &memories {
        let memory_lower = memory.memory.to_lowercase();
        if entity_lowers
            .iter()
            .any(|entity| memory_lower.contains(entity))
        {
            results.push(memory.clone());
        }
    }
    if results.is_empty() && !memories.is_empty() {
        results = memories;
    }
    let count = results.len();
    let message = if count == 1 {
        "1 memory found".to_string()
    } else {
        format!("{count} memories found")
    };
    crate::db::replace_memory_search_results(
        connection,
        search.id,
        "completed",
        &message,
        &results,
    )?;
    Ok(count)
}

fn memory_search_entities(
    paths: &AppPaths,
    settings: &crate::models::LlmSettings,
    query: &str,
) -> anyhow::Result<Vec<String>> {
    let graph_entities = crate::graph::entity_session_counts(paths)?
        .into_iter()
        .map(|entity| entity.entity)
        .collect::<Vec<_>>();
    let mut entities = match extract_memory_search_entities(paths, settings, query, &graph_entities)
    {
        Ok(entities) => entities,
        Err(error) => {
            append_error_log(
                paths,
                "Memory search entity extraction failed",
                &format_memory_search_entity_extraction_error(&error, query, &graph_entities),
            );
            return Err(error);
        }
    };
    let literal = query.trim();
    if !literal.is_empty() {
        entities.push(literal.to_string());
    }
    let mut seen = std::collections::BTreeSet::new();
    Ok(entities
        .into_iter()
        .filter(|entity| seen.insert(entity.to_lowercase()))
        .collect())
}

fn format_memory_search_entity_extraction_error(
    error: &anyhow::Error,
    query: &str,
    graph_entities: &[String],
) -> String {
    let graph_entities_json = serde_json::to_string_pretty(graph_entities)
        .unwrap_or_else(|_| "<graph entities json failed>".into());
    format!(
        "stage: memory_search_entity_extraction\nerror: {error:#}\nquery: {query}\ngraph_entity_count: {}\ngraph_entities_json:\n{graph_entities_json}",
        graph_entities.len()
    )
}

fn extract_memory_search_entities(
    paths: &AppPaths,
    settings: &crate::models::LlmSettings,
    query: &str,
    graph_entities: &[String],
) -> anyhow::Result<Vec<String>> {
    if graph_entities.is_empty() {
        return Ok(Vec::new());
    }
    let graph_entities_json = serde_json::to_string(graph_entities)?;
    let response = request_json_with_provider_count(
        paths,
        settings,
        "Return only JSON with entities. Select only entity strings from the supplied Graph entities list that appear in or are clearly referred to by the user query. Do not invent variants, compound phrases, or entities that are absent from Graph entities.",
        &format!("Graph entities: {graph_entities_json}\nUser query: {query}"),
    )?;
    let graph_entity_by_lower = graph_entities
        .iter()
        .map(|entity| (entity.to_lowercase(), entity.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let values = response
        .content
        .get("entities")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("LLM JSON missing required array field `entities`"))?;
    Ok(values
        .iter()
        .filter_map(|value| {
            if let Some(entity) = value.as_str() {
                return Some(entity.trim().to_string());
            }
            value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(|entity| entity.trim().to_string())
        })
        .filter(|entity| !entity.is_empty())
        .filter_map(|entity| graph_entity_by_lower.get(&entity.to_lowercase()).cloned())
        .collect())
}

