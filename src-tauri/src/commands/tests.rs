#[cfg(test)]
mod tests {
    use super::{
        get_app_state_with_roots, get_cached_app_state_with_roots, read_markdown_file_inner,
        reset_memories_inner, reset_projects_inner, reset_sessions_inner, reset_tasks_inner,
        scan_sources_into_db,
    };
    use crate::{
        memory::MemoryEntity,
        models::{AppPaths, RawMessage, RawSession},
    };

    #[test]
    fn parse_task_metadata_requires_name_and_description() {
        let parsed = super::parse_task_metadata_json(
            r#"{"task_name":"Save Drawer","task_description":"Persist **session**."}"#,
        )
        .unwrap();

        assert_eq!(parsed.task_name, "Save Drawer");
        assert!(parsed.task_description.contains("Persist"));

        let error = super::parse_task_metadata_json(r#"{"task_name":""}"#).unwrap_err();
        assert!(error.to_string().contains("task_description"));
    }

    #[test]
    fn get_app_state_does_not_scan_sources_on_load() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));

        let claude_project_dir = temp.path().join("claude/projects/project-a");
        std::fs::create_dir_all(&claude_project_dir).unwrap();
        std::fs::write(
            claude_project_dir.join("claude-1.jsonl"),
            r#"{"uuid":"claude-1","timestamp":"2026-04-26T01:59:00Z","type":"summary","summary":"ignored"}"#
                .to_owned()
                + "\n"
                + r#"{"uuid":"claude-1","timestamp":"2026-04-26T02:00:00Z","type":"user","cwd":"/Users/kc/ClaudeProject","message":{"role":"user","content":"Find sessions"}}"#
                + "\n"
                + r#"{"uuid":"claude-1","timestamp":"2026-04-26T02:01:00Z","type":"assistant","message":{"role":"assistant","content":"Found"}}"#,
        )
        .unwrap();

        let codex_sessions_dir = temp.path().join("codex/sessions");
        std::fs::create_dir_all(&codex_sessions_dir).unwrap();
        std::fs::write(
            codex_sessions_dir.join("codex-1.jsonl"),
            r#"{"type":"session_meta","payload":{"id":"codex-1","cwd":"/Users/kc/CodexProject","timestamp":"2026-04-26T03:00:00Z"}}"#
                .to_owned()
                + "\n"
                + r#"{"id":"codex-1","timestamp":"2026-04-26T03:01:00Z","message":{"role":"user","content":"Scan Codex"}}"#
                + "\n"
                + r#"{"id":"codex-1","timestamp":"2026-04-26T03:02:00Z","message":{"role":"assistant","content":"Scanned"}}"#,
        )
        .unwrap();

        let state =
            get_app_state_with_roots(&paths, temp.path().join("claude"), codex_sessions_dir)
                .unwrap();

        assert_eq!(state.stats.sessions, 0);
        assert_eq!(state.stats.active_projects, 0);
        assert!(state.projects.is_empty());
    }

    #[test]
    fn assistant_project_paths_requires_reviewed_project() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let mut connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        let project_dir = temp.path().join("app");
        std::fs::create_dir_all(&project_dir).unwrap();
        crate::db::upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "assistant-project-root".into(),
                workdir: project_dir.to_string_lossy().to_string(),
                created_at: "2026-04-28T00:00:00Z".into(),
                updated_at: "2026-04-28T00:00:01Z".into(),
                raw_path: temp
                    .path()
                    .join("session.jsonl")
                    .to_string_lossy()
                    .to_string(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "hello".into(),
                }],
            }],
        )
        .unwrap();

        let error = super::assistant_project_paths(&paths, "app")
            .unwrap_err()
            .to_string();

        assert!(error.contains("reviewed"));
    }

    #[test]
    fn assistant_project_paths_return_code_and_summary_roots() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let mut connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        let project_dir = temp.path().join("app");
        std::fs::create_dir_all(&project_dir).unwrap();
        crate::db::upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "assistant-reviewed-project".into(),
                workdir: project_dir.to_string_lossy().to_string(),
                created_at: "2026-04-28T00:00:00Z".into(),
                updated_at: "2026-04-28T00:00:01Z".into(),
                raw_path: temp
                    .path()
                    .join("session.jsonl")
                    .to_string_lossy()
                    .to_string(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "hello".into(),
                }],
            }],
        )
        .unwrap();
        let (project_id, _) = crate::db::get_project_by_slug(&connection, "app")
            .unwrap()
            .unwrap();
        crate::db::update_project_review(&connection, project_id, "/tmp/info.md").unwrap();

        let (code_root, summary_root) = super::assistant_project_paths(&paths, "app").unwrap();

        assert_eq!(code_root, project_dir);
        assert_eq!(summary_root, paths.projects_dir.join("app"));
        assert!(summary_root.exists());
    }

    #[test]
    fn save_agent_session_enqueue_writes_payload_job_without_generating_task() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let mut connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        let project_dir = temp.path().join("app");
        std::fs::create_dir_all(&project_dir).unwrap();
        crate::db::upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "save-agent-session".into(),
                workdir: project_dir.to_string_lossy().to_string(),
                created_at: "2026-04-28T00:00:00Z".into(),
                updated_at: "2026-04-28T00:00:01Z".into(),
                raw_path: temp.path().join("session.jsonl").to_string_lossy().to_string(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "hello".into(),
                }],
            }],
        )
        .unwrap();
        let (project_id, _) = crate::db::get_project_by_slug(&connection, "app")
            .unwrap()
            .unwrap();
        crate::db::update_project_review(&connection, project_id, "/tmp/info.md").unwrap();
        let timeline = crate::models::AgentTimelinePayload {
            version: 1,
            session_id: "drawer-session".into(),
            project_slug: "app".into(),
            messages: vec![serde_json::json!({"role": "user", "content": "Save this"})],
            todos: Vec::new(),
            context: serde_json::json!({}),
        };

        let result = super::enqueue_save_agent_session_inner(
            &paths,
            "drawer-session",
            "app",
            timeline,
            vec![serde_json::json!({"role": "user", "content": "Save this"})],
        )
        .unwrap();

        let jobs = crate::db::list_active_jobs(&connection).unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(jobs[0].kind, "save_agent_session");
        assert_eq!(jobs[0].session_id.as_deref(), Some("drawer-session"));
        assert_eq!(jobs[0].project_slug.as_deref(), Some("app"));
        let payload_path = super::save_agent_session_payload_path(&paths, result.job_id);
        let payload: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(payload_path).unwrap()).unwrap();
        assert_eq!(payload["llmMessages"][0]["content"], "Save this");
        assert!(crate::db::list_tasks(&connection).unwrap().is_empty());
    }

    #[test]
    fn save_agent_session_job_writes_task_and_saved_session_payload() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let mut connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        let project_dir = temp.path().join("app");
        std::fs::create_dir_all(&project_dir).unwrap();
        crate::db::upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "save-agent-session-job".into(),
                workdir: project_dir.to_string_lossy().to_string(),
                created_at: "2026-04-28T00:00:00Z".into(),
                updated_at: "2026-04-28T00:00:01Z".into(),
                raw_path: temp.path().join("session.jsonl").to_string_lossy().to_string(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "hello".into(),
                }],
            }],
        )
        .unwrap();
        let (project_id, _) = crate::db::get_project_by_slug(&connection, "app")
            .unwrap()
            .unwrap();
        crate::db::update_project_review(&connection, project_id, "/tmp/info.md").unwrap();
        let timeline = crate::models::AgentTimelinePayload {
            version: 1,
            session_id: "drawer-session".into(),
            project_slug: "app".into(),
            messages: vec![
                serde_json::json!({"role": "user", "content": "Save this"}),
                serde_json::json!({"role": "assistant", "content": "Saved answer"}),
            ],
            todos: Vec::new(),
            context: serde_json::json!({"usedTokens": 12}),
        };
        let job = super::enqueue_save_agent_session_inner(
            &paths,
            "drawer-session",
            "app",
            timeline,
            vec![serde_json::json!({"role": "assistant", "content": "Saved answer"})],
        )
        .unwrap();

        let task = super::run_save_agent_session_job_with_metadata(
            &paths,
            job.job_id,
            "drawer-session",
            "app",
            |_settings, _messages| {
                Ok(r#"{"task_name":"Saved Drawer","task_description":"Persisted **Drawer** session."}"#.into())
            },
        )
        .unwrap();

        assert_eq!(task.slug, "saved-drawer");
        assert_eq!(task.status, "discussing");
        let session_path = task.session_path.as_deref().unwrap();
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(session_path).unwrap()).unwrap();
        assert_eq!(saved["llmMessages"][0]["content"], "Saved answer");
        assert_eq!(saved["context"]["usedTokens"], 12);
    }

    #[test]
    fn get_app_state_includes_active_jobs() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::enqueue_analyze_sessions(&connection, None).unwrap();

        let state = get_app_state_with_roots(
            &paths,
            temp.path().join("claude"),
            temp.path().join("codex"),
        )
        .unwrap();

        assert_eq!(state.jobs.len(), 1);
        assert_eq!(state.jobs[0].status, "queued");
    }

    #[test]
    fn get_app_state_includes_entity_count() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session =
            seed_command_session_with_memory(&paths, "entity-session", "KittyNest", "memory");
        crate::graph::write_session_graph(
            &paths,
            &session,
            &[MemoryEntity {
                name: "SQLite".into(),
                entity_type: "technology".into(),
            }],
        )
        .unwrap();

        let state = get_app_state_with_roots(
            &paths,
            temp.path().join("claude"),
            temp.path().join("codex"),
        )
        .unwrap();

        assert_eq!(state.stats.memories, 1);
        assert_eq!(state.stats.entities, 1);
    }

    #[test]
    fn list_entity_sessions_hydrates_titles_outside_recent_session_limit() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session =
            seed_command_session_with_memory(&paths, "old-kittycopilot-session", "KittyCopilot", "memory");
        let mut connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        connection
            .execute(
                "UPDATE sessions SET title = ?1, created_at = ?2, updated_at = ?2 WHERE session_id = ?3",
                rusqlite::params![
                    "Old KittyCopilot Session",
                    "2026-04-19T05:11:23Z",
                    "old-kittycopilot-session"
                ],
            )
            .unwrap();
        let newer = (0..100)
            .map(|index| RawSession {
                source: "codex".into(),
                session_id: format!("newer-session-{index:03}"),
                workdir: format!("/Users/kc/Newer{index:03}"),
                created_at: format!("2026-04-20T00:{index:02}:00Z"),
                updated_at: format!("2026-04-20T00:{index:02}:01Z"),
                raw_path: format!("/tmp/newer-session-{index:03}.jsonl"),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Newer session".into(),
                }],
            })
            .collect::<Vec<_>>();
        crate::db::upsert_raw_sessions(&mut connection, &newer).unwrap();
        crate::graph::write_session_graph(
            &paths,
            &session,
            &[MemoryEntity {
                name: "Kittycopilot".into(),
                entity_type: "project".into(),
            }],
        )
        .unwrap();

        let related = super::list_entity_sessions_inner(&paths, "Kittycopilot").unwrap();

        assert_eq!(related.len(), 1);
        assert_eq!(related[0].session_id, "old-kittycopilot-session");
        assert_eq!(related[0].title, "Old KittyCopilot Session");
    }

    #[test]
    fn get_cached_app_state_does_not_scan_sources() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let codex_sessions_dir = temp.path().join("codex/sessions");
        let workdir = temp.path().join("Cached");
        std::fs::create_dir_all(&codex_sessions_dir).unwrap();
        std::fs::create_dir_all(&workdir).unwrap();
        std::fs::write(
            codex_sessions_dir.join("codex-cached.jsonl"),
            serde_json::json!({
                "type": "session_meta",
                "payload": {
                    "id": "codex-cached",
                    "cwd": workdir.to_string_lossy(),
                    "timestamp": "2026-04-26T03:00:00Z"
                }
            })
            .to_string()
                + "\n"
                + r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Do not scan"}]}}"#,
        )
        .unwrap();

        let cached = get_cached_app_state_with_roots(
            &paths,
            temp.path().join("claude"),
            codex_sessions_dir.clone(),
        )
        .unwrap();
        crate::config::initialize_workspace(&paths).unwrap();
        let mut connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        let (_, _, inserted) = scan_sources_into_db(
            &paths,
            &mut connection,
            &temp.path().join("claude"),
            &codex_sessions_dir,
        )
        .unwrap();
        let scanned =
            get_cached_app_state_with_roots(&paths, temp.path().join("claude"), codex_sessions_dir)
                .unwrap();

        assert_eq!(cached.stats.sessions, 0);
        assert_eq!(inserted, 1);
        assert_eq!(scanned.stats.sessions, 1);
    }

    #[test]
    fn scan_sources_deletes_removed_project_session_and_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let claude_root = temp.path().join("claude");
        let codex_root = temp.path().join("codex/sessions");
        let removed_workdir = temp.path().join("RemovedProject");
        let active_workdir = temp.path().join("ActiveProject");
        std::fs::create_dir_all(&codex_root).unwrap();
        std::fs::create_dir_all(&removed_workdir).unwrap();
        std::fs::create_dir_all(&active_workdir).unwrap();
        let removed_raw = codex_root.join("removed.jsonl");
        let active_raw = codex_root.join("active.jsonl");
        std::fs::write(
            &removed_raw,
            format!(
                "{}\n{}",
                serde_json::json!({"type":"session_meta","payload":{"id":"removed-session","cwd":removed_workdir.to_string_lossy(),"timestamp":"2026-04-26T00:00:00Z"}}),
                serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Removed project"}]}})
            ),
        )
        .unwrap();
        std::fs::write(
            &active_raw,
            format!(
                "{}\n{}",
                serde_json::json!({"type":"session_meta","payload":{"id":"active-session","cwd":active_workdir.to_string_lossy(),"timestamp":"2026-04-26T00:00:00Z"}}),
                serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Active project"}]}})
            ),
        )
        .unwrap();
        let mut connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        scan_sources_into_db(&paths, &mut connection, &claude_root, &codex_root).unwrap();
        let removed = crate::db::unprocessed_session_by_session_id(&connection, "removed-session")
            .unwrap()
            .remove(0);
        let removed_project_dir = paths.projects_dir.join(&removed.project_slug);
        let removed_session_dir = removed_project_dir.join("sessions/removed-session");
        let removed_task_dir = removed_project_dir.join("tasks/draft");
        std::fs::create_dir_all(&removed_session_dir).unwrap();
        std::fs::create_dir_all(&removed_task_dir).unwrap();
        std::fs::write(removed_session_dir.join("summary.md"), "# Removed").unwrap();
        std::fs::write(removed_task_dir.join("description.md"), "# Draft").unwrap();
        crate::db::upsert_task(
            &connection,
            removed.project_id,
            "draft",
            "Draft",
            "brief",
            "discussing",
            &removed_task_dir.join("description.md").to_string_lossy(),
        )
        .unwrap();
        crate::db::replace_session_memories(
            &connection,
            &removed,
            &["removed memory".to_string()],
        )
        .unwrap();
        crate::graph::write_session_graph(
            &paths,
            &removed,
            &[MemoryEntity {
                name: "RemovedEntity".into(),
                entity_type: "concept".into(),
            }],
        )
        .unwrap();
        let removed_memory_dir = paths.memories_dir.join("sessions/removed-session");
        std::fs::create_dir_all(&removed_memory_dir).unwrap();
        std::fs::write(removed_memory_dir.join("memory.md"), "removed memory\n").unwrap();

        std::fs::remove_file(&removed_raw).unwrap();
        std::fs::remove_dir_all(&removed_workdir).unwrap();
        scan_sources_into_db(&paths, &mut connection, &claude_root, &codex_root).unwrap();

        let sessions = crate::db::list_sessions(&connection).unwrap();
        let projects = crate::db::list_projects(&connection).unwrap();
        assert!(sessions
            .iter()
            .all(|session| session.session_id != "removed-session"));
        assert!(sessions
            .iter()
            .any(|session| session.session_id == "active-session"));
        assert!(projects
            .iter()
            .all(|project| project.slug != removed.project_slug));
        assert!(!removed_project_dir.exists());
        assert!(!removed_memory_dir.exists());
        assert!(crate::db::session_memories_by_session_id(&connection, "removed-session")
            .unwrap()
            .is_empty());
        assert_eq!(
            crate::graph::graph_counts(&paths).unwrap(),
            crate::graph::GraphCounts { entities: 0 }
        );
    }

    #[test]
    fn read_markdown_file_rejects_paths_outside_project_store() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let outside = temp.path().join("outside.md");
        std::fs::write(&outside, "# Outside").unwrap();

        let result = read_markdown_file_inner(&paths, &outside.to_string_lossy());

        assert!(result.is_err());
    }

    #[test]
    fn read_markdown_file_allows_memory_store_paths() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let memory_dir = paths.memories_dir.join("sessions/session-1");
        std::fs::create_dir_all(&memory_dir).unwrap();
        let memory_path = memory_dir.join("memory.md");
        std::fs::write(&memory_path, "memory line\n").unwrap();

        let content = read_markdown_file_inner(&paths, &memory_path.to_string_lossy()).unwrap();

        assert_eq!(content, "memory line\n");
    }

    #[test]
    fn session_memory_detail_includes_path_lines_and_related_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session = seed_command_session_with_memory(
            &paths,
            "detail-session",
            "MemoryProject",
            "SQLite memory",
        );
        crate::graph::write_session_graph(
            &paths,
            &session,
            &[crate::memory::MemoryEntity {
                name: "SQLite".into(),
                entity_type: "technology".into(),
            }],
        )
        .unwrap();

        let detail = super::get_session_memory_inner(&paths, "detail-session").unwrap();

        assert!(detail
            .memory_path
            .ends_with("memories/sessions/detail-session/memory.md"));
        assert_eq!(detail.memories, vec!["SQLite memory".to_string()]);
    }

    #[test]
    fn reset_tasks_inner_deletes_task_directories() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let task_dir = paths.projects_dir.join("KittyNest/tasks/session-ingest");
        std::fs::create_dir_all(&task_dir).unwrap();
        std::fs::write(task_dir.join("summary.md"), "{}").unwrap();

        reset_tasks_inner(&paths).unwrap();

        assert!(!task_dir.exists());
    }

    #[test]
    fn reset_sessions_inner_deletes_session_directories() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let session = seed_command_session_with_memory(&paths, "session-1", "KittyNest", "memory");
        crate::graph::write_session_graph(
            &paths,
            &session,
            &[MemoryEntity {
                name: "SessionEntity".into(),
                entity_type: "concept".into(),
            }],
        )
        .unwrap();
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        let session_dir = paths.projects_dir.join("KittyNest/sessions/session-1");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(session_dir.join("summary.md"), "# Session").unwrap();
        let memory_dir = paths.memories_dir.join("sessions/session-1");

        let reset = reset_sessions_inner(&paths).unwrap();

        assert_eq!(reset, 1);
        assert!(crate::db::list_sessions(&connection).unwrap().is_empty());
        assert!(!session_dir.exists());
        assert!(!memory_dir.exists());
        assert!(crate::db::session_memories_by_session_id(&connection, "session-1")
            .unwrap()
            .is_empty());
        assert_eq!(
            crate::graph::graph_counts(&paths).unwrap(),
            crate::graph::GraphCounts { entities: 0 }
        );
    }

    #[test]
    fn reset_memories_inner_deletes_memory_files_records_and_graph() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let mut connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "reset-memory".into(),
                workdir: "/tmp/reset-memory".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/reset-memory.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Remember reset".into(),
                }],
            }],
        )
        .unwrap();
        let stored = crate::db::unprocessed_session_by_session_id(&connection, "reset-memory")
            .unwrap()
            .remove(0);
        crate::db::mark_session_processed_with_optional_task_at(
            &connection,
            stored.id,
            None,
            "Reset Memory",
            "Summary",
            "/tmp/reset-memory/summary.md",
            "2026-04-27T10:00:00Z",
        )
        .unwrap();
        crate::db::replace_session_memories(
            &connection,
            &stored,
            &["memory to delete".to_string()],
        )
        .unwrap();
        crate::graph::write_session_graph(
            &paths,
            &stored,
            &[MemoryEntity {
                name: "Memory".into(),
                entity_type: "concept".into(),
            }],
        )
        .unwrap();
        let memory_dir = paths.memories_dir.join("sessions/reset-memory");
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("memory.md"), "memory to delete\n").unwrap();

        let reset = reset_memories_inner(&paths).unwrap();

        assert_eq!(reset, 1);
        assert_eq!(
            crate::db::enqueue_rebuild_memories(&connection)
                .unwrap()
                .total,
            2
        );
        assert!(!paths.memories_dir.join("sessions").exists());
        assert!(
            crate::db::session_memories_by_session_id(&connection, "reset-memory")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            crate::graph::graph_counts(&paths).unwrap(),
            crate::graph::GraphCounts { entities: 0 }
        );
    }

    #[test]
    fn reset_projects_inner_deletes_all_project_directories() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        crate::config::initialize_workspace(&paths).unwrap();
        let session =
            seed_command_session_with_memory(&paths, "project-session", "KittyNest", "memory");
        crate::graph::write_session_graph(
            &paths,
            &session,
            &[MemoryEntity {
                name: "ProjectEntity".into(),
                entity_type: "concept".into(),
            }],
        )
        .unwrap();
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        let project_dir = paths.projects_dir.join("KittyNest");
        std::fs::create_dir_all(project_dir.join("tasks/session-ingest")).unwrap();
        std::fs::write(project_dir.join("summary.md"), "# Summary").unwrap();
        std::fs::write(project_dir.join("progress.md"), "# Progress").unwrap();
        std::fs::write(
            project_dir.join("tasks/session-ingest/summary.md"),
            "# Task",
        )
        .unwrap();
        let memory_dir = paths.memories_dir.join("sessions/project-session");

        let reset = reset_projects_inner(&paths).unwrap();

        assert_eq!(reset, 1);
        assert!(crate::db::list_projects(&connection).unwrap().is_empty());
        assert!(crate::db::list_sessions(&connection).unwrap().is_empty());
        assert!(crate::db::list_tasks(&connection).unwrap().is_empty());
        assert!(!project_dir.exists());
        assert!(!memory_dir.exists());
        assert!(crate::db::session_memories_by_session_id(&connection, "project-session")
            .unwrap()
            .is_empty());
        assert_eq!(
            crate::graph::graph_counts(&paths).unwrap(),
            crate::graph::GraphCounts { entities: 0 }
        );
    }

    fn seed_command_session_with_memory(
        paths: &AppPaths,
        session_id: &str,
        project_slug: &str,
        memory: &str,
    ) -> crate::models::StoredSession {
        crate::config::initialize_workspace(paths).unwrap();
        let mut connection = crate::db::open(paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: session_id.into(),
                workdir: format!("/Users/kc/{project_slug}"),
                created_at: "2026-04-27T00:00:00Z".into(),
                updated_at: "2026-04-27T00:00:01Z".into(),
                raw_path: format!("/tmp/{session_id}.jsonl"),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Remember SQLite".into(),
                }],
            }],
        )
        .unwrap();
        let session = crate::db::unprocessed_session_by_session_id(&connection, session_id)
            .unwrap()
            .remove(0);
        crate::db::mark_session_processed_with_optional_task(
            &connection,
            session.id,
            None,
            session_id,
            "Summary",
            &format!("/tmp/{session_id}/summary.md"),
        )
        .unwrap();
        crate::db::replace_session_memories(&connection, &session, &[memory.to_string()]).unwrap();
        let memory_dir = paths
            .memories_dir
            .join("sessions")
            .join(crate::utils::slugify_lower(session_id));
        std::fs::create_dir_all(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("memory.md"), format!("{memory}\n")).unwrap();
        session
    }
}

