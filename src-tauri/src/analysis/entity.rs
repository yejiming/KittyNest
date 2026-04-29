const ENTITY_DISAMBIGUATION_BATCH_SIZE: usize = 100;
const ENTITY_DISAMBIGUATION_ROUND2_MIN_ENTITIES: usize = 61;
const ENTITY_ALIAS_GROUP_SYSTEM_PROMPT: &str = "Think simply and do not overthink. Return only JSON with groups. Merge synonymous entities. groups must be an array of {canonical_id, canonical_name, aliases}. canonical_id must be unique. canonical_name must be unique and human-facing. aliases must include every synonym string from the supplied entities that belongs to the group. Each supplied entity may appear in at most one group's aliases. Example response: {\"groups\":[{\"canonical_id\":\"sqlite\",\"canonical_name\":\"SQLite\",\"aliases\":[\"sqlite\",\"SQLite DB\",\"SQLite database\"]},{\"canonical_id\":\"react\",\"canonical_name\":\"React\",\"aliases\":[\"react\",\"React.js\"]}]}";
const ENTITY_ALIAS_MERGE_SYSTEM_PROMPT: &str = "Think simply and do not overthink. Return only JSON. Identify synonymous names from the supplied list. Return {\"merges\":[{\"keep\":\"name_to_keep\",\"merge\":[\"synonym1\",\"synonym2\"]}]} for each group of synonyms. If no merges are needed, return {\"merges\":[]}. Example: input [\"/home/user/.kittynest\",\"~/.kittynest\",\"KittyNest\",\"kittynest\"] → {\"merges\":[{\"keep\":\"KittyNest\",\"merge\":[\"kittynest\"]},{\"keep\":\"~/.kittynest\",\"merge\":[\"/home/user/.kittynest\"]}]}";

#[derive(Clone, Debug, PartialEq, Eq)]
struct EntityAliasMerge {
    keep: String,
    merge: Vec<String>,
}

fn disambiguate_memory_entities(
    paths: &AppPaths,
    settings: &crate::models::LlmSettings,
) -> anyhow::Result<()> {
    let entities = crate::graph::all_entities(paths)?;
    if entities.is_empty() {
        return Ok(());
    }
    let settings =
        crate::config::resolve_llm_settings(settings, crate::config::LlmScenario::Memory);
    let mut seen_entity_names = std::collections::BTreeSet::new();
    let mut entity_names = entities
        .iter()
        .filter_map(|entity| {
            if seen_entity_names.insert(entity.name.clone()) {
                Some(entity.name.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    entity_names.sort();
    let entity_names_json = serde_json::to_string(&entity_names)?;
    let mut log_system_prompt = ENTITY_ALIAS_GROUP_SYSTEM_PROMPT;
    let mut log_user_prompt = format!("Existing entity names:\n{entity_names_json}");
    let result = (|| -> anyhow::Result<()> {
        let mut groups = Vec::new();
        for batch in entity_names.chunks(ENTITY_DISAMBIGUATION_BATCH_SIZE) {
            let batch_names = batch
                .iter()
                .map(|name| name.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            let batch_entities = entities
                .iter()
                .filter(|entity| batch_names.contains(entity.name.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            let user_prompt = format!("Existing entity names:\n{}", serde_json::to_string(batch)?);
            log_system_prompt = ENTITY_ALIAS_GROUP_SYSTEM_PROMPT;
            log_user_prompt = user_prompt.clone();
            groups.extend(remote_entity_alias_groups(
                paths,
                &settings,
                &batch_entities,
                ENTITY_ALIAS_GROUP_SYSTEM_PROMPT,
                &user_prompt,
            )?);
        }
        groups = normalize_entity_alias_groups(groups);

        if entity_names.len() >= ENTITY_DISAMBIGUATION_ROUND2_MIN_ENTITIES {
            let canonical_names = groups
                .iter()
                .filter_map(|group| {
                    let canonical_name = group.canonical_name.trim();
                    if canonical_name.is_empty() {
                        None
                    } else {
                        Some(canonical_name.to_string())
                    }
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let mut merges = Vec::new();
            for batch in canonical_names.chunks(ENTITY_DISAMBIGUATION_BATCH_SIZE) {
                let user_prompt = format!(
                    "Existing canonical names:\n{}",
                    serde_json::to_string(batch)?
                );
                log_system_prompt = ENTITY_ALIAS_MERGE_SYSTEM_PROMPT;
                log_user_prompt = user_prompt.clone();
                merges.extend(remote_entity_alias_merges(
                    paths,
                    &settings,
                    ENTITY_ALIAS_MERGE_SYSTEM_PROMPT,
                    &user_prompt,
                )?);
            }
            groups = apply_entity_alias_merges(groups, &merges);
            groups = normalize_entity_alias_groups(groups);
        }

        crate::graph::write_entity_aliases(paths, &groups)
    })();
    if let Err(error) = &result {
        append_error_log(
            paths,
            "Entity disambiguation failed",
            &format_entity_disambiguation_error(
                error,
                entities.len(),
                log_system_prompt,
                &log_user_prompt,
            ),
        );
    }
    result
}

fn remote_entity_alias_groups(
    paths: &AppPaths,
    settings: &crate::models::LlmSettings,
    entities: &[crate::graph::EntityForDisambiguation],
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<Vec<crate::graph::EntityAliasGroup>> {
    let response = request_json_with_provider_count(paths, settings, system_prompt, user_prompt)?;
    entity_alias_groups_from_json(&response.content, entities)
        .map_err(|error| anyhow::anyhow!("{error}; raw_llm_response={}", response.content))
}

fn remote_entity_alias_merges(
    paths: &AppPaths,
    settings: &crate::models::LlmSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> anyhow::Result<Vec<EntityAliasMerge>> {
    let response = request_json_with_provider_count(paths, settings, system_prompt, user_prompt)?;
    entity_alias_merges_from_json(&response.content)
        .map_err(|error| anyhow::anyhow!("{error}; raw_llm_response={}", response.content))
}

fn format_entity_disambiguation_error(
    error: &anyhow::Error,
    entity_count: usize,
    system_prompt: &str,
    user_prompt: &str,
) -> String {
    format!(
        "stage: entity_disambiguation\nerror: {error:#}\nentity_count: {entity_count}\nentity_disambiguation_system_prompt:\n{system_prompt}\nentity_disambiguation_user_prompt:\n{user_prompt}",
    )
}

fn append_error_log(paths: &AppPaths, title: &str, details: &str) {
    let now = crate::utils::now_rfc3339();
    let date = now.get(..10).unwrap_or("unknown-date");
    let logs_dir = paths.data_dir.join("logs");
    let log_path = logs_dir.join(format!("error-{date}.log"));
    let entry = format!("[{now}] {title}\n{details}\n\n");
    if std::fs::create_dir_all(&logs_dir).is_ok() {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            use std::io::Write;
            let _ = file.write_all(entry.as_bytes());
        }
    }
}

fn entity_alias_groups_from_json(
    value: &serde_json::Value,
    entities: &[crate::graph::EntityForDisambiguation],
) -> anyhow::Result<Vec<crate::graph::EntityAliasGroup>> {
    let groups = value
        .get("groups")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("LLM JSON missing required array field `groups`"))?;
    let mut parsed = Vec::new();
    let mut covered = std::collections::BTreeSet::new();
    for group in groups {
        let canonical_id = group
            .get("canonical_id")
            .and_then(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| value.as_i64().map(|value| value.to_string()))
            })
            .ok_or_else(|| anyhow::anyhow!("entity alias group missing canonical_id"))?;
        let canonical_name = required_json_string(group, "canonical_name")?;
        let aliases = group
            .get("aliases")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("entity alias group missing aliases"))?
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        for alias in &aliases {
            covered.insert(alias.to_lowercase());
        }
        covered.insert(canonical_name.to_lowercase());
        parsed.push(crate::graph::EntityAliasGroup {
            canonical_id,
            canonical_name,
            aliases,
        });
    }
    for entity in entities {
        if !covered.contains(&entity.name.to_lowercase()) {
            parsed.push(crate::graph::EntityAliasGroup {
                canonical_id: entity.id.to_string(),
                canonical_name: entity.name.clone(),
                aliases: vec![entity.name.clone()],
            });
        }
    }
    Ok(parsed)
}

fn entity_alias_merges_from_json(
    value: &serde_json::Value,
) -> anyhow::Result<Vec<EntityAliasMerge>> {
    let merges = value
        .get("merges")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("LLM JSON missing required array field `merges`"))?;
    let mut parsed = Vec::new();
    for merge in merges {
        let keep = required_json_string(merge, "keep")?;
        let merge_names = merge
            .get("merge")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("entity alias merge missing merge"))?
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        parsed.push(EntityAliasMerge {
            keep,
            merge: merge_names,
        });
    }
    Ok(parsed)
}

fn apply_entity_alias_merges(
    mut groups: Vec<crate::graph::EntityAliasGroup>,
    merges: &[EntityAliasMerge],
) -> Vec<crate::graph::EntityAliasGroup> {
    for merge in merges {
        let keep_key = canonical_name_key(&merge.keep);
        for source in &merge.merge {
            let source_key = canonical_name_key(source);
            if source_key == keep_key {
                continue;
            }
            let Some(source_index) = groups
                .iter()
                .position(|group| canonical_name_key(&group.canonical_name) == source_key)
            else {
                continue;
            };
            let Some(_) = groups
                .iter()
                .position(|group| canonical_name_key(&group.canonical_name) == keep_key)
            else {
                continue;
            };
            let source_group = groups.remove(source_index);
            if let Some(keep_index) = groups
                .iter()
                .position(|group| canonical_name_key(&group.canonical_name) == keep_key)
            {
                groups[keep_index].aliases.push(source_group.canonical_name);
                groups[keep_index].aliases.extend(source_group.aliases);
            } else {
                groups.push(source_group);
            }
        }
    }
    groups
}

fn normalize_entity_alias_groups(
    groups: Vec<crate::graph::EntityAliasGroup>,
) -> Vec<crate::graph::EntityAliasGroup> {
    let mut merged = Vec::<crate::graph::EntityAliasGroup>::new();
    let mut canonical_indexes = std::collections::BTreeMap::<String, usize>::new();
    for mut group in groups {
        group.canonical_name = group.canonical_name.trim().to_string();
        group.aliases = cleaned_aliases(group.aliases);
        if let Some(index) = canonical_indexes.get(&group.canonical_name).copied() {
            merged[index].aliases.extend(group.aliases);
        } else {
            canonical_indexes.insert(group.canonical_name.clone(), merged.len());
            merged.push(group);
        }
    }
    for group in &mut merged {
        group.aliases = cleaned_aliases(std::mem::take(&mut group.aliases));
    }

    let canonical_owners = merged
        .iter()
        .enumerate()
        .map(|(index, group)| (entity_name_key(&group.canonical_name), index))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut alias_owners = std::collections::BTreeMap::<String, Vec<usize>>::new();
    for (index, group) in merged.iter().enumerate() {
        for alias in &group.aliases {
            alias_owners
                .entry(entity_name_key(alias))
                .or_default()
                .push(index);
        }
    }
    for (alias_key, indexes) in alias_owners {
        let keep_index = canonical_owners
            .get(&alias_key)
            .copied()
            .unwrap_or_else(|| {
                indexes
                    .iter()
                    .copied()
                    .min_by_key(|index| (merged[*index].aliases.len(), *index))
                    .expect("alias conflict must have at least one owner")
            });
        for (index, group) in merged.iter_mut().enumerate() {
            if index != keep_index {
                group
                    .aliases
                    .retain(|alias| entity_name_key(alias) != alias_key);
            }
        }
    }
    merged
}

fn cleaned_aliases(aliases: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    aliases
        .into_iter()
        .filter_map(|alias| {
            let alias = alias.trim();
            if alias.is_empty() {
                return None;
            }
            if seen.insert(entity_name_key(alias)) {
                Some(alias.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn entity_name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn canonical_name_key(name: &str) -> String {
    name.trim().to_string()
}

