use crate::data::models::{ProjectRecord, SessionRecord, TaskRecord};

/// Slugify a string for use as an Obsidian-safe filename.
pub fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = true;
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

/// Render a project note with wikilinks to its sessions and tasks.
pub fn render_project_note(
    project: &ProjectRecord,
    summary_body: &str,
    session_slugs: &[String],
    task_slugs: &[String],
) -> String {
    let mut frontmatter = vec![
        ("tags", "[kittynest/project]".to_string()),
        ("workdir", project.workdir.clone()),
        ("sources", format!("[{}]", project.sources.join(", "))),
    ];
    if let Some(ref reviewed) = project.last_reviewed_at {
        frontmatter.push(("last_reviewed_at", reviewed.clone()));
    }
    if let Some(ref last_session) = project.last_session_at {
        frontmatter.push(("last_session_at", last_session.clone()));
    }

    let mut body = format!("# {}\n\n", project.display_title);
    body.push_str(summary_body.trim());
    body.push('\n');

    if !session_slugs.is_empty() {
        body.push_str("\n## Sessions\n");
        for slug in session_slugs {
            body.push_str(&format!("![[{}]]\n", slug));
        }
    }

    if !task_slugs.is_empty() {
        body.push_str("\n## Tasks\n");
        for slug in task_slugs {
            body.push_str(&format!("![[{}]]\n", slug));
        }
    }

    crate::markdown::render_frontmatter_markdown(
        &frontmatter.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        &body,
    )
}

/// Render a session note with wikilinks to its project and entities.
pub fn render_session_note(
    session: &SessionRecord,
    project_slug: &str,
    entity_names: &[String],
) -> String {
    let title = session.title.as_deref().unwrap_or("Untitled Session");
    let summary = session.summary.as_deref().unwrap_or("");

    let mut frontmatter = vec![
        ("tags", "[kittynest/session]".to_string()),
        ("source", session.source.clone()),
        ("session_id", session.session_id.clone()),
        ("project", format!("[[{}]]", project_slug)),
        ("created_at", session.created_at.clone()),
    ];
    if let Some(ref task_slug) = session.task_slug {
        frontmatter.push(("task", format!("[[{}]]", task_slug)));
    }

    let mut body = format!("# {}\n\n", title);
    body.push_str(summary.trim());
    body.push('\n');

    // Link to memory
    body.push_str(&format!(
        "\n## Memory\n![[memory-{}]]\n",
        session_slug(session)
    ));

    // Link to entities
    if !entity_names.is_empty() {
        body.push_str("\n## Related Entities\n");
        for name in entity_names {
            body.push_str(&format!("- [[{}]]\n", slugify(name)));
        }
    }

    crate::markdown::render_frontmatter_markdown(
        &frontmatter.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        &body,
    )
}

/// Render a task note with wikilinks.
pub fn render_task_note(
    task: &TaskRecord,
    session_slugs: &[String],
) -> String {
    let frontmatter = vec![
        ("tags", "[kittynest/task]".to_string()),
        ("status", task.status.clone()),
        ("project", format!("[[{}]]", task.project_slug)),
        ("created_at", task.created_at.clone()),
    ];

    let mut body = format!("# {}\n\n", task.title);

    // Read description or brief
    if let Some(ref desc_path) = task.description_path {
        if let Ok(content) = std::fs::read_to_string(desc_path) {
            body.push_str(content.trim());
            body.push('\n');
        }
    } else {
        body.push_str(task.brief.trim());
        body.push('\n');
    }

    if !session_slugs.is_empty() {
        body.push_str("\n## Related Sessions\n");
        for slug in session_slugs {
            body.push_str(&format!("- [[{}]]\n", slug));
        }
    }

    crate::markdown::render_frontmatter_markdown(
        &frontmatter.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        &body,
    )
}

/// Render a memory note (per-session aggregation).
pub fn render_memory_note(
    session_slug: &str,
    project_slug: &str,
    memories: &[String],
    entity_names: &[String],
) -> String {
    let frontmatter = vec![
        ("tags", "[kittynest/memory]".to_string()),
        ("session", format!("[[{}]]", session_slug)),
        ("project", format!("[[{}]]", project_slug)),
    ];

    let mut body = format!("# Memory: {}\n\n", session_slug);
    for memory in memories {
        body.push_str(&format!("- {}\n", memory));
    }

    if !entity_names.is_empty() {
        body.push_str("\n## Related Entities\n");
        for name in entity_names {
            body.push_str(&format!("- [[{}]]\n", slugify(name)));
        }
    }

    crate::markdown::render_frontmatter_markdown(
        &frontmatter.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        &body,
    )
}

/// Render an entity MOC (Map of Content) index page.
pub fn render_entity_moc(
    entity_name: &str,
    entity_type: &str,
    session_slugs: &[String],
    memory_slugs: &[String],
) -> String {
    let frontmatter = vec![
        ("tags", "[kittynest/entity]".to_string()),
        ("entity_type", entity_type.to_string()),
    ];

    let mut body = format!("# {}\n\n", entity_name);

    if !session_slugs.is_empty() {
        body.push_str("## Sessions\n");
        for slug in session_slugs {
            body.push_str(&format!("- [[{}]]\n", slug));
        }
    }

    if !memory_slugs.is_empty() {
        body.push_str("\n## Memories\n");
        for slug in memory_slugs {
            body.push_str(&format!("- [[{}]]\n", slug));
        }
    }

    crate::markdown::render_frontmatter_markdown(
        &frontmatter.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>(),
        &body,
    )
}

/// Derive a session slug from a SessionRecord for use as a note name.
pub fn session_slug(session: &SessionRecord) -> String {
    let date_prefix = session.created_at[..10].replace('-', "");
    let title_part = session
        .title
        .as_deref()
        .unwrap_or(&session.session_id[..8.min(session.session_id.len())]);
    format!("{}-{}", date_prefix, slugify(title_part))
}

/// Derive a task slug from a TaskRecord for use as a note name.
pub fn task_slug(task: &TaskRecord) -> String {
    slugify(&task.slug)
}

/// Build the Obsidian relative path for a note kind.
pub fn obsidian_relative_path(kind: &str, project_slug: &str, note_name: &str) -> String {
    match kind {
        "project" => format!("KittyNest/projects/{}/{}.md", project_slug, note_name),
        "session" => format!(
            "KittyNest/projects/{}/sessions/{}.md",
            project_slug, note_name
        ),
        "task" => format!(
            "KittyNest/projects/{}/tasks/{}.md",
            project_slug, note_name
        ),
        "memory" => format!("KittyNest/memories/{}.md", note_name),
        "entity" => format!("KittyNest/entities/{}.md", note_name),
        _ => format!("KittyNest/{}.md", note_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("Build Fix!"), "build-fix");
        assert_eq!(slugify("  spaces  "), "spaces");
        assert_eq!(slugify("camelCase"), "camelcase");
    }

    #[test]
    fn test_session_slug() {
        let session = SessionRecord {
            source: "claude".to_string(),
            session_id: "abc-123".to_string(),
            raw_path: String::new(),
            project_slug: "test".to_string(),
            task_slug: None,
            title: Some("Build Fix".to_string()),
            summary: None,
            summary_path: None,
            created_at: "2026-05-02T06:40:45Z".to_string(),
            updated_at: String::new(),
            status: "analyzed".to_string(),
        };
        assert_eq!(session_slug(&session), "20260502-build-fix");
    }

    #[test]
    fn test_obsidian_relative_path() {
        assert_eq!(
            obsidian_relative_path("session", "kitty-nest", "20260502-build-fix"),
            "KittyNest/projects/kitty-nest/sessions/20260502-build-fix.md"
        );
        assert_eq!(
            obsidian_relative_path("memory", "kitty-nest", "memory-20260502-build-fix"),
            "KittyNest/memories/memory-20260502-build-fix.md"
        );
        assert_eq!(
            obsidian_relative_path("entity", "kitty-nest", "rusqlite"),
            "KittyNest/entities/rusqlite.md"
        );
    }
}
