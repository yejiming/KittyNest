#[cfg(test)]
mod tests {
    use super::{
        analyze_session, create_manual_task, import_historical_sessions, memory_search_entities,
        normalize_entity_alias_groups, rebuild_memories, review_project, run_next_analysis_job,
        session_worker_count, store_session_analysis, write_progress, SessionAnalysis,
    };
    use crate::{
        db::{migrate, open, upsert_raw_sessions},
        graph::EntityAliasGroup,
        models::{AppPaths, LlmSettings, RawMessage, RawSession},
    };

    #[test]
    fn import_historical_sessions_writes_task_session_and_progress_once() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "session-1".into(),
                workdir: "/Users/kc/KittyNest".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:05:00Z".into(),
                raw_path: "/tmp/session-1.jsonl".into(),
                messages: vec![
                    RawMessage {
                        role: "user".into(),
                        content: "Implement historical session import".into(),
                    },
                    RawMessage {
                        role: "assistant".into(),
                        content: "Added importer".into(),
                    },
                ],
            }],
        )
        .unwrap();
        crate::llm::test_support::set_json_responses(vec![session_response(
            "implement-historical-session-import",
            "Implement Historical Session Import",
            "Historical session import summary.",
        )]);

        let first = import_historical_sessions(&paths).unwrap();
        let second = import_historical_sessions(&paths).unwrap();

        assert_eq!(first.projects_updated, 0);
        assert_eq!(first.tasks_created, 0);
        assert_eq!(first.sessions_written, 1);
        assert_eq!(second.sessions_written, 0);
        assert!(!paths.projects_dir.join("KittyNest/progress.md").exists());
        assert!(!paths.projects_dir.join("KittyNest/tasks").exists());
        assert!(paths
            .projects_dir
            .join("KittyNest/sessions/session-1/summary.md")
            .exists());
    }

    #[test]
    fn session_analysis_writes_session_folders_without_task_summaries() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "session-alpha".into(),
                    workdir: "/Users/kc/KittyNest".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:05:00Z".into(),
                    raw_path: "/tmp/session-alpha.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Build task summaries".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "session-beta".into(),
                    workdir: "/Users/kc/KittyNest".into(),
                    created_at: "2026-04-26T01:00:00Z".into(),
                    updated_at: "2026-04-26T01:10:00Z".into(),
                    raw_path: "/tmp/session-beta.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Extend task summaries".into(),
                    }],
                },
            ],
        )
        .unwrap();
        crate::llm::test_support::set_json_responses(vec![
            session_response("task-summary", "Task Summary", "First summary."),
            session_response("task-summary", "Task Summary", "Second summary."),
        ]);

        import_historical_sessions(&paths).unwrap();
        let sessions = crate::db::list_sessions(&connection).unwrap();

        assert!(crate::db::list_tasks(&connection).unwrap().is_empty());
        assert!(!paths.projects_dir.join("KittyNest/tasks").exists());
        assert!(paths
            .projects_dir
            .join("KittyNest/sessions/session-alpha/summary.md")
            .exists());
        assert!(paths
            .projects_dir
            .join("KittyNest/sessions/session-beta/summary.md")
            .exists());
        assert!(sessions.iter().all(|session| session
            .summary_path
            .as_deref()
            .is_some_and(|path| path.contains("/sessions/"))));
    }

    #[test]
    fn run_next_analysis_job_processes_queued_sessions_and_completes_job() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "session-1".into(),
                workdir: "/Users/kc/KittyNest".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:05:00Z".into(),
                raw_path: "/tmp/session-1.jsonl".into(),
                messages: vec![
                    RawMessage {
                        role: "user".into(),
                        content: "Implement background analysis".into(),
                    },
                    RawMessage {
                        role: "assistant".into(),
                        content: "Added worker".into(),
                    },
                ],
            }],
        )
        .unwrap();
        let enqueued = crate::db::enqueue_analyze_sessions(&connection, None).unwrap();
        crate::llm::test_support::set_json_responses(vec![session_response(
            "implement-background-analysis",
            "Implement Background Analysis",
            "Background analysis summary.",
        )]);

        let processed = run_next_analysis_job(&paths).unwrap();
        let sessions = crate::db::list_sessions(&connection).unwrap();
        let jobs = crate::db::list_active_jobs(&connection).unwrap();

        assert!(processed);
        assert!(jobs.is_empty());
        assert!(sessions[0].task_slug.is_none());
        assert!(!paths.projects_dir.join("KittyNest/progress.md").exists());
        assert_eq!(enqueued.total, 1);
    }

    #[test]
    fn run_next_analysis_job_marks_session_failed_when_llm_is_unavailable() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "no-llm".into(),
                workdir: "/Users/kc/KittyNest".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:05:00Z".into(),
                raw_path: "/tmp/no-llm.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "This should not use a local fallback".into(),
                }],
            }],
        )
        .unwrap();
        crate::db::enqueue_analyze_sessions(&connection, None).unwrap();
        crate::llm::test_support::clear();

        assert!(run_next_analysis_job(&paths).unwrap());
        let sessions = crate::db::list_sessions(&connection).unwrap();
        let (_, _, failed): (i64, i64, i64) = connection
            .query_row(
                "SELECT completed, total, failed FROM jobs LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(sessions[0].status, "failed");
        assert!(sessions[0].task_slug.is_none());
        assert_eq!(failed, 1);
    }

    #[test]
    fn session_analysis_retries_invalid_json_with_error_context() {
        let _mock_guard = crate::llm::test_support::guard();
        crate::llm::test_support::set_json_responses(vec![
            serde_json::json!({"task_name": "missing-fields"}),
            serde_json::json!({"task_name": "still-missing", "title": "Still Missing"}),
            session_response("fixed-json", "Fixed Json", "Valid on third attempt."),
        ]);
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let settings = empty_settings();
        let session = stored_test_session("retry-json");

        let analysis = analyze_session(&paths, &settings, &session).unwrap();
        let requests = crate::llm::test_support::take_requests();

        assert_eq!(analysis.session_title, "Fixed Json");
        assert_eq!(requests.len(), 3);
        assert!(requests[1]
            .user_prompt
            .contains("Previous LLM response error"));
        assert!(requests[2]
            .user_prompt
            .contains("Previous LLM response error"));
    }

    #[test]
    fn session_analysis_accepts_session_memory_contract() {
        let _mock_guard = crate::llm::test_support::guard();
        crate::llm::test_support::set_json_responses(vec![serde_json::json!({
            "session_title": "Focused Session",
            "summary": "Only session fields are required.",
            "memories": ["Session fields can drive memory."],
            "entities": [{"name": "KittyNest", "type": "project"}]
        })]);
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let settings = empty_settings();
        let session = stored_test_session("session-only-json");

        let analysis = analyze_session(&paths, &settings, &session).unwrap();
        let requests = crate::llm::test_support::take_requests();

        assert_eq!(analysis.session_title, "Focused Session");
        assert_eq!(
            analysis.session_summary,
            "Only session fields are required."
        );
        assert_eq!(
            analysis.memory.memories,
            vec!["Session fields can drive memory.".to_string()]
        );
        assert_eq!(analysis.memory.entities[0].name, "KittyNest");
        assert!(requests[0].system_prompt.contains("session_title"));
        assert!(requests[0].system_prompt.contains("memories"));
        assert!(requests[0].system_prompt.contains("entities"));
        assert!(!requests[0].system_prompt.contains("task_name"));
    }

    #[test]
    fn store_session_analysis_writes_session_summary_without_creating_task() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "session-only".into(),
                workdir: "/Users/kc/SessionOnly".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:05:00Z".into(),
                raw_path: "/tmp/session-only.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Summarize only this session".into(),
                }],
            }],
        )
        .unwrap();
        let stored = crate::db::unprocessed_session_by_session_id(&connection, "session-only")
            .unwrap()
            .remove(0);

        store_session_analysis(
            &paths,
            &connection,
            &stored,
            SessionAnalysis {
                session_title: "Session Only".into(),
                session_summary: "Session summary only.".into(),
                memory: session_memory_draft(),
            },
        )
        .unwrap();
        let sessions = crate::db::list_sessions(&connection).unwrap();

        assert!(crate::db::list_tasks(&connection).unwrap().is_empty());
        assert!(paths
            .projects_dir
            .join("SessionOnly/sessions/session-only/summary.md")
            .exists());
        assert!(!paths.projects_dir.join("SessionOnly/tasks").exists());
        assert_eq!(sessions[0].task_slug, None);
        assert_eq!(sessions[0].title.as_deref(), Some("Session Only"));
    }

    #[test]
    fn store_session_analysis_writes_session_memory_and_graph_entities() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "memory-session".into(),
                workdir: "/Users/kc/MemoryProject".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:05:00Z".into(),
                raw_path: "/tmp/memory-session.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Remember CozoDB uses SQLite".into(),
                }],
            }],
        )
        .unwrap();
        let stored = crate::db::unprocessed_session_by_session_id(&connection, "memory-session")
            .unwrap()
            .remove(0);

        store_session_analysis(
            &paths,
            &connection,
            &stored,
            SessionAnalysis {
                session_title: "Memory Session".into(),
                session_summary: "Session summary with memory.".into(),
                memory: session_memory_draft(),
            },
        )
        .unwrap();
        let memory_path = paths.memories_dir.join("sessions/memory-session/memory.md");
        let memory_markdown = std::fs::read_to_string(memory_path).unwrap();
        let memory_records =
            crate::db::session_memories_by_session_id(&connection, "memory-session").unwrap();
        let graph_counts = crate::graph::graph_counts(&paths).unwrap();

        assert_eq!(
            memory_markdown,
            "CozoDB is the graph store.\nUser prefers short memory facts.\n"
        );
        assert_eq!(
            memory_records,
            vec![
                "CozoDB is the graph store.".to_string(),
                "User prefers short memory facts.".to_string()
            ]
        );
        assert_eq!(graph_counts.entities, 2);
    }

    #[test]
    fn rebuild_memories_regenerates_analyzed_session_memory_from_raw_messages() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "rebuild-memory".into(),
                workdir: "/Users/kc/RebuildMemory".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:05:00Z".into(),
                raw_path: "/tmp/rebuild-memory.jsonl".into(),
                messages: vec![
                    RawMessage {
                        role: "system".into(),
                        content: "hidden".into(),
                    },
                    RawMessage {
                        role: "user".into(),
                        content: "Remember the user prefers short memory facts".into(),
                    },
                    RawMessage {
                        role: "assistant".into(),
                        content: "I will keep each memory concise.".into(),
                    },
                ],
            }],
        )
        .unwrap();
        let stored = crate::db::unprocessed_session_by_session_id(&connection, "rebuild-memory")
            .unwrap()
            .remove(0);
        store_session_analysis(
            &paths,
            &connection,
            &stored,
            SessionAnalysis {
                session_title: "Old Memory".into(),
                session_summary: "Old summary.".into(),
                memory: session_memory_draft(),
            },
        )
        .unwrap();
        connection
            .execute(
                "UPDATE sessions SET processed_at = '2026-04-26T00:00:00Z' WHERE id = ?1",
                rusqlite::params![stored.id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE session_memories SET created_at = '2026-04-25T23:00:00Z' WHERE source_session = 'rebuild-memory'",
                [],
            )
            .unwrap();
        crate::llm::test_support::set_json_responses(vec![
            serde_json::json!({
                "memories": ["User prefers concise memory facts."],
                "entities": [{"name": "Memory Facts", "type": "concept"}]
            }),
            serde_json::json!({
                "groups": [
                    {
                        "canonical_id": 1,
                        "canonical_name": "Memory Facts",
                        "aliases": ["Memory Facts"]
                    }
                ]
            }),
        ]);

        let rebuilt = rebuild_memories(&paths).unwrap();
        let memory_path = paths.memories_dir.join("sessions/rebuild-memory/memory.md");
        let memory_markdown = std::fs::read_to_string(memory_path).unwrap();
        let memory_records =
            crate::db::session_memories_by_session_id(&connection, "rebuild-memory").unwrap();
        let requests = crate::llm::test_support::take_requests();

        assert_eq!(rebuilt, 1);
        assert_eq!(memory_markdown, "User prefers concise memory facts.\n");
        assert_eq!(
            memory_records,
            vec!["User prefers concise memory facts.".to_string()]
        );
        assert!(requests[0].system_prompt.contains("memories"));
        assert!(!requests[0].system_prompt.contains("session_title"));
        assert!(requests[0]
            .user_prompt
            .contains("Remember the user prefers"));
        assert!(!requests[0].user_prompt.contains("hidden"));
    }

    #[test]
    fn run_next_analysis_job_rebuilds_memories_from_job_queue() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        seed_rebuildable_session(&paths, "queued-rebuild", "MemoryProject");
        crate::llm::test_support::set_json_responses(vec![
            serde_json::json!({
                "memories": ["Queued rebuild memory."],
                "entities": [{"name": "MemoryProject", "type": "project"}]
            }),
            serde_json::json!({
                "groups": [
                    {
                        "canonical_id": 1,
                        "canonical_name": "MemoryProject",
                        "aliases": ["MemoryProject"]
                    }
                ]
            }),
        ]);
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::enqueue_rebuild_memories(&connection).unwrap();

        assert!(run_next_analysis_job(&paths).unwrap());

        let memory_path = paths.memories_dir.join("sessions/queued-rebuild/memory.md");
        assert_eq!(
            std::fs::read_to_string(memory_path).unwrap(),
            "Queued rebuild memory.\n"
        );
    }

    #[test]
    fn run_next_analysis_job_clears_old_memory_before_rebuild_attempt() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session = seed_analyzed_session(&paths, "cleanup-rebuild", "MemoryProject");
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        connection
            .execute(
                "UPDATE sessions SET processed_at = '2026-04-27T10:00:00Z' WHERE id = ?1",
                rusqlite::params![session.id],
            )
            .unwrap();
        crate::memory::generate_session_memory_at(
            &paths,
            &connection,
            &session,
            &session_memory_draft(),
            "2026-04-27T09:00:00Z",
        )
        .unwrap();
        crate::llm::test_support::set_json_responses(vec![serde_json::json!({})]);
        crate::db::enqueue_rebuild_memories(&connection).unwrap();

        assert!(run_next_analysis_job(&paths).unwrap());

        let memory_path = paths
            .memories_dir
            .join("sessions/cleanup-rebuild/memory.md");
        assert!(!memory_path.exists());
        assert!(
            crate::db::session_memories_by_session_id(&connection, "cleanup-rebuild")
                .unwrap()
                .is_empty()
        );
        assert_eq!(crate::graph::graph_counts(&paths).unwrap().entities, 0);
    }

    #[test]
    fn run_next_analysis_job_disambiguates_entities_after_memory_rebuild() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        seed_rebuildable_session(&paths, "sqlite-session", "MemoryProject");
        seed_rebuildable_session(&paths, "sqlite-duplicate-session", "MemoryProject");
        seed_rebuildable_session(&paths, "sqlite-db-session", "MemoryProject");
        crate::llm::test_support::set_json_responses(vec![
            serde_json::json!({
                "memories": ["SQLite stores local memory."],
                "entities": [{"name": "sqlite", "type": "technology"}]
            }),
            serde_json::json!({
                "memories": ["SQLite also stores local memory."],
                "entities": [{"name": "sqlite", "type": "technology"}]
            }),
            serde_json::json!({
                "memories": ["SQLite DB stores local memory."],
                "entities": [{"name": "SQLite DB", "type": "technology"}]
            }),
            serde_json::json!({
                "groups": [
                    {
                        "canonical_id": "sqlite",
                        "canonical_name": "SQLite",
                        "aliases": ["sqlite", "SQLite DB"]
                    }
                ]
            }),
        ]);
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::enqueue_rebuild_memories(&connection).unwrap();

        assert!(run_next_analysis_job(&paths).unwrap());

        let requests = crate::llm::test_support::take_requests();
        let entities = crate::graph::entity_session_counts(&paths).unwrap();
        let related = crate::graph::related_sessions_for_session(&paths, "sqlite-session").unwrap();

        assert_eq!(requests.len(), 4);
        assert!(requests[3].system_prompt.contains("canonical_id"));
        assert!(requests[3].system_prompt.contains("Example response"));
        assert!(requests[3].system_prompt.contains("SQLite database"));
        assert!(requests[3]
            .user_prompt
            .starts_with("Existing entity names:\n[\""));
        assert!(!requests[3].user_prompt.contains("\n  \""));
        assert_eq!(requests[3].user_prompt.matches("\"sqlite\"").count(), 1);
        assert!(requests[3].user_prompt.contains("\"sqlite db\""));
        assert!(!requests[3].user_prompt.contains("sourceSession"));
        assert!(!requests[3].user_prompt.contains("entityType"));
        assert!(!requests[3].user_prompt.contains("sourceProject"));
        assert!(!requests[3].user_prompt.contains("\"id\""));
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity, "SQLite");
        assert_eq!(entities[0].session_count, 3);
        assert_eq!(related[0].shared_entities, vec!["SQLite".to_string()]);
    }

    #[test]
    fn run_next_analysis_job_disambiguates_entities_even_when_no_memories_need_rebuild() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let first = seed_analyzed_session(&paths, "fresh-sqlite", "MemoryProject");
        let second = seed_analyzed_session(&paths, "fresh-sqlite-db", "MemoryProject");
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        for session in [&first, &second] {
            connection
                .execute(
                    "UPDATE sessions SET processed_at = '2026-04-27T10:00:00Z' WHERE id = ?1",
                    rusqlite::params![session.id],
                )
                .unwrap();
        }
        crate::memory::generate_session_memory_at(
            &paths,
            &connection,
            &first,
            &crate::memory::SessionMemoryDraft {
                memories: vec!["SQLite stores local memory.".into()],
                entities: vec![crate::memory::MemoryEntity {
                    name: "sqlite".into(),
                    entity_type: "technology".into(),
                }],
            },
            "2026-04-27T10:00:00Z",
        )
        .unwrap();
        crate::memory::generate_session_memory_at(
            &paths,
            &connection,
            &second,
            &crate::memory::SessionMemoryDraft {
                memories: vec!["SQLite DB stores local memory.".into()],
                entities: vec![crate::memory::MemoryEntity {
                    name: "SQLite DB".into(),
                    entity_type: "technology".into(),
                }],
            },
            "2026-04-27T10:00:00Z",
        )
        .unwrap();
        crate::llm::test_support::set_json_responses(vec![serde_json::json!({
            "groups": [
                {
                    "canonical_id": "sqlite",
                    "canonical_name": "SQLite",
                    "aliases": ["sqlite", "SQLite DB"]
                }
            ]
        })]);

        let enqueued = crate::db::enqueue_rebuild_memories(&connection).unwrap();
        assert_eq!(enqueued.total, 1);
        assert!(run_next_analysis_job(&paths).unwrap());

        let requests = crate::llm::test_support::take_requests();
        let entities = crate::graph::entity_session_counts(&paths).unwrap();

        assert_eq!(requests.len(), 1);
        assert!(requests[0].system_prompt.contains("Example response"));
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity, "SQLite");
        assert_eq!(entities[0].session_count, 2);
    }

    #[test]
    fn run_next_analysis_job_batches_disambiguation_and_merges_canonical_names() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session = seed_analyzed_session(&paths, "batched-entities", "MemoryProject");
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        connection
            .execute(
                "UPDATE sessions SET processed_at = '2026-04-27T10:00:00Z' WHERE id = ?1",
                rusqlite::params![session.id],
            )
            .unwrap();
        let mut entities = vec![crate::memory::MemoryEntity {
            name: "aaa-alpha".into(),
            entity_type: "project".into(),
        }];
        for index in 0..99 {
            entities.push(crate::memory::MemoryEntity {
                name: format!("filler-{index:03}"),
                entity_type: "concept".into(),
            });
        }
        entities.push(crate::memory::MemoryEntity {
            name: "zzz-alpha".into(),
            entity_type: "project".into(),
        });
        crate::memory::generate_session_memory_at(
            &paths,
            &connection,
            &session,
            &crate::memory::SessionMemoryDraft {
                memories: vec!["Batched entity memory.".into()],
                entities,
            },
            "2026-04-27T10:00:00Z",
        )
        .unwrap();
        crate::llm::test_support::set_json_responses(vec![
            serde_json::json!({
                "groups": [
                    {
                        "canonical_id": "alpha-tool",
                        "canonical_name": "Alpha Tool",
                        "aliases": ["aaa-alpha"]
                    }
                ]
            }),
            serde_json::json!({
                "groups": [
                    {
                        "canonical_id": "alpha-tool-lower",
                        "canonical_name": "alpha tool",
                        "aliases": ["zzz-alpha"]
                    }
                ]
            }),
            serde_json::json!({
                "merges": [
                    {
                        "keep": "Alpha Tool",
                        "merge": ["alpha tool"]
                    }
                ]
            }),
            serde_json::json!({
                "merges": []
            }),
        ]);

        let enqueued = crate::db::enqueue_rebuild_memories(&connection).unwrap();
        assert_eq!(enqueued.total, 1);
        assert!(run_next_analysis_job(&paths).unwrap());

        let requests = crate::llm::test_support::take_requests();
        assert_eq!(requests.len(), 4);
        assert!(requests[0]
            .user_prompt
            .starts_with("Existing entity names:\n[\"aaa-alpha\""));
        assert!(requests[0].user_prompt.contains("\"filler-098\""));
        assert!(!requests[0].user_prompt.contains("\"zzz-alpha\""));
        assert_eq!(requests[0].user_prompt.matches("\"filler-").count(), 99);
        assert_eq!(
            requests[1].user_prompt,
            "Existing entity names:\n[\"zzz-alpha\"]"
        );
        assert!(requests[2]
            .system_prompt
            .contains("Identify synonymous names from the supplied list"));
        assert!(requests[2]
            .user_prompt
            .starts_with("Existing canonical names:\n[\"Alpha Tool\",\"alpha tool\""));
        assert_eq!(
            requests[3].user_prompt,
            "Existing canonical names:\n[\"filler-098\"]"
        );

        let graph = rusqlite::Connection::open(paths.data_dir.join("kittynest_graph.db")).unwrap();
        let aaa_canonical: String = graph
            .query_row(
                "SELECT canonical_name FROM entity_alias WHERE name = ?1",
                ["aaa-alpha"],
                |row| row.get(0),
            )
            .unwrap();
        let zzz_canonical: String = graph
            .query_row(
                "SELECT canonical_name FROM entity_alias WHERE name = ?1",
                ["zzz-alpha"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(aaa_canonical, "Alpha Tool");
        assert_eq!(zzz_canonical, "Alpha Tool");
    }

    #[test]
    fn normalize_entity_alias_groups_merges_duplicate_canonicals_and_resolves_alias_conflicts() {
        let groups = normalize_entity_alias_groups(vec![
            EntityAliasGroup {
                canonical_id: "sqlite".into(),
                canonical_name: "SQLite".into(),
                aliases: vec!["sqlite".into(), "SQLite DB".into(), "duplicate".into()],
            },
            EntityAliasGroup {
                canonical_id: "sqlite-duplicate".into(),
                canonical_name: "SQLite Duplicate".into(),
                aliases: vec!["sqlite".into(), "duplicate".into()],
            },
            EntityAliasGroup {
                canonical_id: "kittynest-a".into(),
                canonical_name: "KittyNest".into(),
                aliases: vec!["kittynest".into()],
            },
            EntityAliasGroup {
                canonical_id: "kittynest-b".into(),
                canonical_name: "KittyNest".into(),
                aliases: vec!["KittyNest app".into()],
            },
        ]);
        let sqlite = groups
            .iter()
            .find(|group| group.canonical_name == "SQLite")
            .unwrap();
        let duplicate = groups
            .iter()
            .find(|group| group.canonical_name == "SQLite Duplicate")
            .unwrap();
        let kittynest = groups
            .iter()
            .find(|group| group.canonical_name == "KittyNest")
            .unwrap();

        assert_eq!(
            groups
                .iter()
                .filter(|group| group.canonical_name == "KittyNest")
                .count(),
            1
        );
        assert!(sqlite.aliases.contains(&"sqlite".to_string()));
        assert!(sqlite.aliases.contains(&"SQLite DB".to_string()));
        assert!(!sqlite.aliases.contains(&"duplicate".to_string()));
        assert!(!duplicate.aliases.contains(&"sqlite".to_string()));
        assert!(duplicate.aliases.contains(&"duplicate".to_string()));
        assert!(kittynest.aliases.contains(&"kittynest".to_string()));
        assert!(kittynest.aliases.contains(&"KittyNest app".to_string()));
    }

    #[test]
    fn run_next_analysis_job_logs_entity_disambiguation_failures() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session = seed_analyzed_session(&paths, "log-disambiguation", "MemoryProject");
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        connection
            .execute(
                "UPDATE sessions SET processed_at = '2026-04-27T10:00:00Z' WHERE id = ?1",
                rusqlite::params![session.id],
            )
            .unwrap();
        crate::memory::generate_session_memory_at(
            &paths,
            &connection,
            &session,
            &crate::memory::SessionMemoryDraft {
                memories: vec!["SQLite stores local memory.".into()],
                entities: vec![crate::memory::MemoryEntity {
                    name: "sqlite".into(),
                    entity_type: "technology".into(),
                }],
            },
            "2026-04-27T10:00:00Z",
        )
        .unwrap();
        crate::llm::test_support::set_json_responses(vec![serde_json::json!({
            "unexpected": []
        })]);
        crate::db::enqueue_rebuild_memories(&connection).unwrap();

        assert!(run_next_analysis_job(&paths).unwrap());

        let logs_dir = paths.data_dir.join("logs");
        let log_path = std::fs::read_dir(&logs_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("error-") && name.ends_with(".log"))
            })
            .unwrap();
        let log = std::fs::read_to_string(log_path).unwrap();

        assert!(log.contains("Entity disambiguation failed"));
        assert!(log.contains("stage: entity_disambiguation"));
        assert!(log.contains("entity_disambiguation_system_prompt:"));
        assert!(log.contains("entity_disambiguation_user_prompt:"));
        assert!(log.contains("LLM JSON missing required array field `groups`"));
        assert!(log.contains("raw_llm_response={\"unexpected\":[]}"));
        assert!(log.contains("\"unexpected\""));
        assert!(log.contains("Existing entity names:"));
        assert!(log.contains("\"sqlite\""));
        assert!(!log.contains("\n  \"sqlite\""));
        assert!(!log.contains("entity_disambiguation_input_json:"));
        assert!(!log.contains("raw_entity_alias_response="));
        assert!(!log.contains("\"name\": \"sqlite\""));
    }

    #[test]
    fn run_next_analysis_job_searches_memories_from_job_queue() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session = seed_analyzed_session(&paths, "search-session", "MemoryProject");
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::replace_session_memories(
            &connection,
            &session,
            &["SQLite is used for local graph memory.".to_string()],
        )
        .unwrap();
        crate::graph::write_session_graph(
            &paths,
            &session,
            &[crate::memory::MemoryEntity {
                name: "SQLite".into(),
                entity_type: "technology".into(),
            }],
        )
        .unwrap();
        crate::llm::test_support::set_json_responses(vec![serde_json::json!({
            "entities": ["SQLite"]
        })]);
        crate::db::enqueue_search_memories(&connection, "Where is sqlite used?").unwrap();

        assert!(run_next_analysis_job(&paths).unwrap());

        let latest = crate::db::latest_memory_search(&connection)
            .unwrap()
            .unwrap();
        assert_eq!(latest.results[0].source_session, "search-session");
        assert_eq!(
            latest.results[0].memory,
            "SQLite is used for local graph memory."
        );
    }

    #[test]
    fn memory_search_entity_extraction_uses_memory_model() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session = seed_analyzed_session(&paths, "memory-model-search", "MemoryProject");
        let mut settings = crate::config::default_llm_settings();
        settings.model = "default-model".into();
        settings.api_key = "default-key".into();
        settings.models = vec![
            crate::models::LlmModelSettings {
                id: "default-model-id".into(),
                provider: "DefaultProvider".into(),
                remark: "Default".into(),
                base_url: "https://default.example/v1".into(),
                interface: "openai".into(),
                model: "default-model".into(),
                api_key: "default-key".into(),
                max_context: 128_000,
                max_tokens: 4_096,
                temperature: 0.2,
            },
            crate::models::LlmModelSettings {
                id: "memory-model-id".into(),
                provider: "MemoryProvider".into(),
                remark: "Memory".into(),
                base_url: "https://memory.example/v1".into(),
                interface: "openai".into(),
                model: "memory-model".into(),
                api_key: "memory-key".into(),
                max_context: 128_000,
                max_tokens: 4_096,
                temperature: 0.2,
            },
        ];
        settings.scenario_models.default_model = "default-model-id".into();
        settings.scenario_models.memory_model = "memory-model-id".into();
        crate::config::write_llm_settings(&paths, &settings).unwrap();

        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::replace_session_memories(
            &connection,
            &session,
            &["SQLite is used for local graph memory.".to_string()],
        )
        .unwrap();
        crate::graph::write_session_graph(
            &paths,
            &session,
            &[crate::memory::MemoryEntity {
                name: "SQLite".into(),
                entity_type: "technology".into(),
            }],
        )
        .unwrap();
        crate::llm::test_support::set_json_responses(vec![serde_json::json!({
            "entities": ["SQLite"]
        })]);
        crate::db::enqueue_search_memories(&connection, "Where is sqlite used?").unwrap();

        assert!(run_next_analysis_job(&paths).unwrap());

        let requests = crate::llm::test_support::take_requests();
        assert_eq!(requests[0].model, "memory-model");
        assert!(requests[0].user_prompt.contains("Graph entities"));
        assert!(requests[0].user_prompt.contains("\"sqlite\""));
        assert!(requests[0].user_prompt.contains("User query"));
        assert!(requests[0].user_prompt.contains("Where is sqlite used?"));
    }

    #[test]
    fn memory_search_entity_extraction_filters_entities_absent_from_graph() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session = seed_analyzed_session(&paths, "memory-filter-search", "MemoryProject");
        crate::graph::write_session_graph(
            &paths,
            &session,
            &[crate::memory::MemoryEntity {
                name: "KittyCopilot".into(),
                entity_type: "project".into(),
            }],
        )
        .unwrap();
        crate::llm::test_support::set_json_responses(vec![serde_json::json!({
            "entities": ["kittycopilot项目", "kittycopilot"]
        })]);

        let entities = memory_search_entities(
            &paths,
            &crate::config::default_llm_settings(),
            "kittycopilot项目是做什么的",
        )
        .unwrap();

        assert!(entities.contains(&"kittycopilot".to_string()));
        assert!(!entities.contains(&"kittycopilot项目".to_string()));
    }

    #[test]
    fn memory_search_entity_extraction_failures_write_error_log() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session = seed_analyzed_session(&paths, "memory-error-search", "MemoryProject");
        crate::graph::write_session_graph(
            &paths,
            &session,
            &[crate::memory::MemoryEntity {
                name: "KittyCopilot".into(),
                entity_type: "project".into(),
            }],
        )
        .unwrap();
        crate::llm::test_support::set_json_responses(vec![serde_json::json!({
            "not_entities": ["kittycopilot"]
        })]);

        let error = memory_search_entities(
            &paths,
            &crate::config::default_llm_settings(),
            "kittycopilot项目是做什么的",
        )
        .unwrap_err()
        .to_string();
        let log_path = std::fs::read_dir(paths.data_dir.join("logs"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let log = std::fs::read_to_string(log_path).unwrap();

        assert!(error.contains("entities"));
        assert!(log.contains("Memory search entity extraction failed"));
        assert!(log.contains("stage: memory_search_entity_extraction"));
        assert!(log.contains("query: kittycopilot项目是做什么的"));
        assert!(log.contains("\"kittycopilot\""));
        assert!(log.contains("LLM JSON missing required array field `entities`"));
    }

    #[test]
    fn run_next_analysis_job_searches_by_literal_entity_and_falls_back_to_session_memories() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session = seed_analyzed_session(&paths, "search-fallback", "MemoryProject");
        let connection = crate::db::open(&paths).unwrap();
        crate::db::migrate(&connection).unwrap();
        crate::db::replace_session_memories(
            &connection,
            &session,
            &["Project analyze now runs from a queued job.".to_string()],
        )
        .unwrap();
        crate::graph::write_session_graph(
            &paths,
            &session,
            &[crate::memory::MemoryEntity {
                name: "enqueue_analyze_project".into(),
                entity_type: "function".into(),
            }],
        )
        .unwrap();
        crate::llm::test_support::set_json_responses(vec![serde_json::json!({
            "entities": []
        })]);
        crate::db::enqueue_search_memories(&connection, "enqueue_analyze_project").unwrap();

        assert!(run_next_analysis_job(&paths).unwrap());

        let latest = crate::db::latest_memory_search(&connection)
            .unwrap()
            .unwrap();
        assert_eq!(latest.message, "1 memory found");
        assert_eq!(latest.results[0].source_session, "search-fallback");
        assert_eq!(
            latest.results[0].memory,
            "Project analyze now runs from a queued job."
        );
    }

    #[test]
    fn run_next_analysis_job_marks_session_failed_after_three_invalid_json_responses() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "bad-json".into(),
                workdir: "/Users/kc/KittyNest".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:05:00Z".into(),
                raw_path: "/tmp/bad-json.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Return bad JSON".into(),
                }],
            }],
        )
        .unwrap();
        crate::db::enqueue_analyze_sessions(&connection, None).unwrap();
        crate::llm::test_support::set_json_responses(vec![
            serde_json::json!({"task_name": "bad-json"}),
            serde_json::json!({"task_name": "bad-json"}),
            serde_json::json!({"task_name": "bad-json"}),
        ]);

        assert!(run_next_analysis_job(&paths).unwrap());
        let sessions = crate::db::list_sessions(&connection).unwrap();
        let requests = crate::llm::test_support::take_requests();

        assert_eq!(sessions[0].status, "failed");
        assert_eq!(requests.len(), 3);
    }

    #[test]
    fn run_next_analysis_job_processes_only_project_scoped_sessions() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "target-session".into(),
                    workdir: "/Users/kc/TargetProject".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:05:00Z".into(),
                    raw_path: "/tmp/target-session.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Implement target project import".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "other-session".into(),
                    workdir: "/Users/kc/OtherProject".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:06:00Z".into(),
                    raw_path: "/tmp/other-session.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Implement other project import".into(),
                    }],
                },
            ],
        )
        .unwrap();
        let target_project = crate::db::list_projects(&connection)
            .unwrap()
            .into_iter()
            .find(|project| project.workdir == "/Users/kc/TargetProject")
            .unwrap();
        crate::db::enqueue_analyze_project_sessions(&connection, &target_project.slug).unwrap();
        crate::llm::test_support::set_json_responses(vec![session_response(
            "implement-target-project-import",
            "Implement Target Project Import",
            "Target project summary.",
        )]);

        assert!(run_next_analysis_job(&paths).unwrap());
        let sessions = crate::db::list_sessions(&connection).unwrap();
        let target = sessions
            .iter()
            .find(|session| session.session_id == "target-session")
            .unwrap();
        let other = sessions
            .iter()
            .find(|session| session.session_id == "other-session")
            .unwrap();

        assert_eq!(target.status, "analyzed");
        assert!(target.task_slug.is_none());
        assert_eq!(other.status, "pending");
        assert!(!paths
            .projects_dir
            .join(format!("{}/progress.md", target_project.slug))
            .exists());
    }

    #[test]
    fn run_next_analysis_job_resumes_completed_count_after_restart() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "done-session".into(),
                    workdir: "/Users/kc/ResumeProject".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:05:00Z".into(),
                    raw_path: "/tmp/done-session.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Already processed".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "pending-session".into(),
                    workdir: "/Users/kc/ResumeProject".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:06:00Z".into(),
                    raw_path: "/tmp/pending-session.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Still pending".into(),
                    }],
                },
            ],
        )
        .unwrap();
        let job = crate::db::enqueue_analyze_sessions(&connection, None).unwrap();
        let claimed = crate::db::claim_next_job(&connection).unwrap().unwrap();
        let done_session =
            crate::db::unprocessed_session_by_session_id(&connection, "done-session")
                .unwrap()
                .remove(0);
        let (task_id, _) = crate::db::upsert_task(
            &connection,
            done_session.project_id,
            "already-processed",
            "Already Processed",
            "Already processed",
            "developing",
            "/tmp/already-processed.md",
        )
        .unwrap();
        crate::db::mark_session_processed(
            &connection,
            done_session.id,
            task_id,
            "Already Processed",
            "Already processed",
            "/tmp/done-session.md",
        )
        .unwrap();
        crate::db::update_job_progress(&connection, claimed.id, 1, 0, "Analyzed 1 of 2").unwrap();
        crate::db::mark_stale_running_jobs_queued(&connection).unwrap();
        crate::llm::test_support::set_json_responses(vec![session_response(
            "still-pending",
            "Still Pending",
            "Pending session summary.",
        )]);

        assert!(run_next_analysis_job(&paths).unwrap());
        let (completed, total, status): (i64, i64, String) = connection
            .query_row(
                "SELECT completed, total, status FROM jobs WHERE id = ?1",
                rusqlite::params![job.job_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(total, 2);
        assert_eq!(completed, 2);
        assert_eq!(status, "completed");
    }

    #[test]
    fn session_worker_count_uses_multiple_workers_for_batches() {
        assert_eq!(session_worker_count(1), 1);
        assert_eq!(session_worker_count(8), 5);
    }

    #[test]
    fn run_next_analysis_job_processes_queued_project_review_and_completes_job() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "review-project".into(),
                workdir: temp
                    .path()
                    .join("ReviewProject")
                    .to_string_lossy()
                    .to_string(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: temp
                    .path()
                    .join("review-project.jsonl")
                    .to_string_lossy()
                    .to_string(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Review the project".into(),
                }],
            }],
        )
        .unwrap();
        let project = crate::db::list_projects(&connection).unwrap().remove(0);
        let enqueued = crate::db::enqueue_review_project(&connection, &project.slug).unwrap();
        crate::llm::test_support::set_markdown_responses(vec![
            "## summary\n\nReviewed.\n\n## tech_stack\n\nRust.\n\n## architecture\n\nLocal modules.\n\n## code_quality\n\nReadable.\n\n## risks\n\nNone known.",
        ]);

        let processed = run_next_analysis_job(&paths).unwrap();
        let jobs = crate::db::list_active_jobs(&connection).unwrap();
        let (_, reviewed) = crate::db::get_project_by_slug(&connection, &project.slug)
            .unwrap()
            .unwrap();

        assert!(processed);
        assert_eq!(enqueued.total, 1);
        assert!(jobs.is_empty());
        assert_eq!(reviewed.review_status, "reviewed");
        assert!(reviewed.info_path.is_some());
    }

    #[test]
    fn run_next_analysis_job_analyzes_newest_twenty_then_writes_project_summary_and_progress() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let workdir = temp.path().join("AnalyzeProject");
        std::fs::create_dir_all(&workdir).unwrap();
        std::fs::write(workdir.join("README.md"), "# Analyze Project").unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let raw_sessions = (1..=22)
            .map(|index| RawSession {
                source: "codex".into(),
                session_id: format!("session-{index:02}"),
                workdir: workdir.to_string_lossy().to_string(),
                created_at: format!("2026-04-26T00:{index:02}:00Z"),
                updated_at: format!("2026-04-26T00:{index:02}:30Z"),
                raw_path: temp
                    .path()
                    .join(format!("session-{index:02}.jsonl"))
                    .to_string_lossy()
                    .to_string(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: format!("Analyze session {index:02}"),
                }],
            })
            .collect::<Vec<_>>();
        upsert_raw_sessions(&mut connection, &raw_sessions).unwrap();
        let project = crate::db::list_projects(&connection).unwrap().remove(0);
        let session_responses = (1..=20)
            .map(|index| {
                session_response(
                    &format!("analyzed-session-{index:02}"),
                    &format!("Analyzed Session {index:02}"),
                    &format!("Analyzed summary {index:02}."),
                )
            })
            .collect::<Vec<_>>();
        crate::llm::test_support::set_json_responses(session_responses);
        crate::llm::test_support::set_markdown_responses_by_prompt(vec![
            (
                "Review the project",
                "## summary\n\nProject analyzed.\n\n## tech_stack\n\nRust.\n\n## architecture\n\nTauri.\n\n## code_quality\n\nFocused.\n\n## risks\n\nNone.",
            ),
            ("Project Progress", "# Progress\n\nCurrent project progress."),
            (
                "durable, reusable working preferences",
                "# User Preference\n\nPrefers concise implementation notes.",
            ),
            (
                "Create an AGENTS.md file",
                "# AGENTS.md\n\nAlways run focused tests before changing code.",
            ),
        ]);

        let enqueued = crate::db::enqueue_analyze_project(&connection, &project.slug).unwrap();
        let processed = run_next_analysis_job(&paths).unwrap();
        let sessions = crate::db::list_sessions(&connection).unwrap();
        let oldest = sessions
            .iter()
            .filter(|session| matches!(session.session_id.as_str(), "session-01" | "session-02"))
            .collect::<Vec<_>>();
        let analyzed_count = sessions
            .iter()
            .filter(|session| session.status == "analyzed")
            .count();
        let (_, reviewed) = crate::db::get_project_by_slug(&connection, &project.slug)
            .unwrap()
            .unwrap();

        assert!(processed);
        let markdown_requests = crate::llm::test_support::take_requests()
            .into_iter()
            .filter(|request| request.kind == "markdown")
            .collect::<Vec<_>>();

        assert_eq!(enqueued.total, 24);
        assert_eq!(analyzed_count, 20);
        assert!(oldest.iter().all(|session| session.status == "pending"));
        assert_eq!(reviewed.review_status, "reviewed");
        assert!(reviewed
            .info_path
            .as_deref()
            .is_some_and(|path| path.ends_with("/summary.md")));
        assert!(reviewed
            .progress_path
            .as_deref()
            .is_some_and(|path| path.ends_with("/progress.md")));
        assert!(reviewed
            .user_preference_path
            .as_deref()
            .is_some_and(|path| path.ends_with("/user_preference.md")));
        assert!(reviewed
            .agents_path
            .as_deref()
            .is_some_and(|path| path.ends_with("/AGENTS.md")));
        assert!(
            std::fs::read_to_string(reviewed.user_preference_path.unwrap())
                .unwrap()
                .contains("Prefers concise implementation notes.")
        );
        assert!(std::fs::read_to_string(reviewed.agents_path.unwrap())
            .unwrap()
            .contains("Always run focused tests before changing code."));
        let preference_request = markdown_requests
            .iter()
            .find(|request| {
                request
                    .system_prompt
                    .contains("durable, reusable working preferences")
            })
            .unwrap();
        let agents_request = markdown_requests
            .iter()
            .find(|request| request.system_prompt.contains("Create an AGENTS.md file"))
            .unwrap();
        let progress_request = markdown_requests
            .iter()
            .find(|request| request.system_prompt.contains("Project Progress"))
            .unwrap();
        assert!(!progress_request.user_prompt.contains("Analyze session 01"));
        assert!(!progress_request.user_prompt.contains("Analyze session 02"));
        assert!(progress_request
            .user_prompt
            .contains("Analyzed summary 20."));
        assert!(!preference_request
            .user_prompt
            .contains("Analyze session 01"));
        assert!(!preference_request
            .user_prompt
            .contains("Analyze session 02"));
        assert!(preference_request
            .user_prompt
            .contains("Analyze session 22"));
        assert!(preference_request
            .system_prompt
            .contains("Do not reproduce or summarize AGENTS.md instructions"));
        assert!(agents_request.user_prompt.contains("Project Summary"));
        assert!(agents_request.user_prompt.contains("Project Progress"));
        assert!(agents_request.user_prompt.contains("User Preferences"));
        assert!(agents_request
            .system_prompt
            .contains("Return English Markdown only"));
    }

    #[test]
    fn review_project_requires_llm_and_does_not_write_local_fallback() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let workdir = temp.path().join("NoFallbackProject");
        std::fs::create_dir_all(&workdir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "review-project".into(),
                workdir: workdir.to_string_lossy().to_string(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: temp
                    .path()
                    .join("review-project.jsonl")
                    .to_string_lossy()
                    .to_string(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Review the project".into(),
                }],
            }],
        )
        .unwrap();
        let project = crate::db::list_projects(&connection).unwrap().remove(0);
        crate::llm::test_support::clear();

        let result = review_project(&paths, &project.slug);

        assert!(result.is_err());
        assert!(!paths
            .projects_dir
            .join(format!("{}/summary.md", project.slug))
            .exists());
    }

    #[test]
    fn create_manual_task_rejects_unreviewed_projects() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "manual-task".into(),
                workdir: "/Users/kc/ManualTask".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/manual-task.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Create manual task".into(),
                }],
            }],
        )
        .unwrap();
        let project = crate::db::list_projects(&connection).unwrap().remove(0);

        let error = create_manual_task(&paths, &project.slug, "Build a better prompt")
            .unwrap_err()
            .to_string();

        assert!(error.contains("reviewed"));
        assert!(!paths.projects_dir.join("ManualTask/tasks").exists());
    }

    #[test]
    fn create_manual_task_writes_user_prompt_and_enqueues_prompt_job() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "manual-task".into(),
                workdir: "/Users/kc/ManualTask".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/manual-task.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Create manual task".into(),
                }],
            }],
        )
        .unwrap();
        let (project_id, project) = crate::db::get_project_by_slug(
            &connection,
            &crate::db::list_projects(&connection)
                .unwrap()
                .remove(0)
                .slug,
        )
        .unwrap()
        .unwrap();
        crate::db::update_project_review(&connection, project_id, "/tmp/summary.md").unwrap();

        let result = create_manual_task(&paths, &project.slug, "Build deploy flow").unwrap();
        let user_prompt = std::fs::read_to_string(&result.user_prompt_path).unwrap();
        let tasks = crate::db::list_tasks(&connection).unwrap();
        let jobs = crate::db::list_active_jobs(&connection).unwrap();

        assert_eq!(result.task_slug, "build-deploy-flow");
        assert_eq!(user_prompt, "Build deploy flow\n");
        assert_eq!(tasks[0].status, "discussing");
        assert_eq!(tasks[0].session_count, 0);
        assert_eq!(jobs[0].kind, "generate_task_prompt");
        assert_eq!(jobs[0].task_slug.as_deref(), Some("build-deploy-flow"));
    }

    #[test]
    fn run_next_analysis_job_generates_task_llm_prompt_from_project_context() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "manual-task".into(),
                workdir: "/Users/kc/ManualTask".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/manual-task.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Create manual task".into(),
                }],
            }],
        )
        .unwrap();
        let (project_id, project) = crate::db::get_project_by_slug(
            &connection,
            &crate::db::list_projects(&connection)
                .unwrap()
                .remove(0)
                .slug,
        )
        .unwrap()
        .unwrap();
        let project_dir = paths.projects_dir.join(&project.slug);
        std::fs::create_dir_all(&project_dir).unwrap();
        let summary_path = project_dir.join("summary.md");
        let progress_path = project_dir.join("progress.md");
        std::fs::write(&summary_path, "# Summary\n\nReviewed architecture.").unwrap();
        std::fs::write(&progress_path, "# Progress\n\nCurrent milestone.").unwrap();
        crate::db::update_project_review(&connection, project_id, &summary_path.to_string_lossy())
            .unwrap();
        crate::db::update_project_progress(
            &connection,
            &project.slug,
            &progress_path.to_string_lossy(),
        )
        .unwrap();
        let result = create_manual_task(&paths, &project.slug, "Ship the next milestone").unwrap();
        crate::llm::test_support::set_markdown_responses(vec![
            "Use the reviewed architecture and current milestone to ship the next milestone.",
        ]);

        assert!(run_next_analysis_job(&paths).unwrap());
        let llm_prompt = std::fs::read_to_string(&result.llm_prompt_path).unwrap();
        let requests = crate::llm::test_support::take_requests();

        assert!(llm_prompt.contains("reviewed architecture"));
        assert!(requests[0].user_prompt.contains("Reviewed architecture."));
        assert!(requests[0].user_prompt.contains("Current milestone."));
        assert!(requests[0].user_prompt.contains("Ship the next milestone"));
    }

    #[test]
    fn review_project_reads_file_bodies_and_writes_markdown_response() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let workdir = temp.path().join("ReviewProject");
        std::fs::create_dir_all(workdir.join("src")).unwrap();
        std::fs::write(
            workdir.join("src/lib.rs"),
            "pub fn architecture_marker() -> &'static str { \"hex grid\" }",
        )
        .unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "review-project".into(),
                workdir: workdir.to_string_lossy().to_string(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: temp
                    .path()
                    .join("review-project.jsonl")
                    .to_string_lossy()
                    .to_string(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Review the project".into(),
                }],
            }],
        )
        .unwrap();
        let project = crate::db::list_projects(&connection).unwrap().remove(0);
        crate::llm::test_support::set_markdown_responses(vec![
            "## summary\n\nReviewed from code.\n\n## tech_stack\n\nRust.\n\n## architecture\n\nHex grid.\n\n## code_quality\n\nClear.\n\n## risks\n\nNone.",
        ]);

        let info_path = review_project(&paths, &project.slug).unwrap();
        let markdown = std::fs::read_to_string(info_path).unwrap();
        let requests = crate::llm::test_support::take_requests();

        assert!(markdown.contains("Reviewed from code."));
        assert!(requests[0].system_prompt.contains("Summary"));
        assert!(requests[0].system_prompt.contains("Tech Stack"));
        assert!(requests[0].system_prompt.contains("Architecture"));
        assert!(requests[0].system_prompt.contains("Code Quality"));
        assert!(requests[0].system_prompt.contains("Risks"));
        assert!(requests[0].user_prompt.contains("architecture_marker"));
    }

    #[test]
    fn review_project_strips_llm_think_blocks_before_writing_markdown() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let workdir = temp.path().join("ThinkProject");
        std::fs::create_dir_all(&workdir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "think-project".into(),
                workdir: workdir.to_string_lossy().to_string(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: temp
                    .path()
                    .join("think-project.jsonl")
                    .to_string_lossy()
                    .to_string(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Review the project".into(),
                }],
            }],
        )
        .unwrap();
        let project = crate::db::list_projects(&connection).unwrap().remove(0);
        crate::llm::test_support::set_markdown_responses(vec![
            "<think>hidden reasoning</think>\n\n## summary\n\nVisible summary.",
        ]);

        let info_path = review_project(&paths, &project.slug).unwrap();
        let markdown = std::fs::read_to_string(info_path).unwrap();

        assert!(!markdown.contains("<think>"));
        assert!(!markdown.contains("hidden reasoning"));
        assert!(markdown.contains("Visible summary."));
    }

    #[test]
    fn write_progress_sends_analyzed_session_summaries_in_time_order_to_llm() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "later".into(),
                    workdir: "/Users/kc/TimelineProject".into(),
                    created_at: "2026-04-26T02:00:00Z".into(),
                    updated_at: "2026-04-26T02:10:00Z".into(),
                    raw_path: "/tmp/later.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Later work".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "earlier".into(),
                    workdir: "/Users/kc/TimelineProject".into(),
                    created_at: "2026-04-26T01:00:00Z".into(),
                    updated_at: "2026-04-26T01:10:00Z".into(),
                    raw_path: "/tmp/earlier.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Earlier work".into(),
                    }],
                },
            ],
        )
        .unwrap();
        let project = crate::db::list_projects(&connection).unwrap().remove(0);
        for (session_id, summary) in [
            ("later", "Second session summary"),
            ("earlier", "First session summary"),
        ] {
            let stored = crate::db::unprocessed_session_by_session_id(&connection, session_id)
                .unwrap()
                .remove(0);
            let (task_id, _) = crate::db::upsert_task(
                &connection,
                stored.project_id,
                session_id,
                session_id,
                summary,
                "developing",
                "/tmp/task.md",
            )
            .unwrap();
            crate::db::mark_session_processed(
                &connection,
                stored.id,
                task_id,
                session_id,
                summary,
                "/tmp/session.md",
            )
            .unwrap();
        }
        crate::llm::test_support::set_markdown_responses(vec![
            "# Project Progress\n\nNarrative timeline.",
        ]);
        let settings = empty_settings();

        write_progress(&paths, &settings, &project.slug).unwrap();
        let requests = crate::llm::test_support::take_requests();
        let prompt = &requests[0].user_prompt;
        let markdown = std::fs::read_to_string(
            paths
                .projects_dir
                .join(format!("{}/progress.md", project.slug)),
        )
        .unwrap();

        assert!(
            prompt.find("First session summary").unwrap()
                < prompt.find("Second session summary").unwrap()
        );
        assert_eq!(requests.len(), 1);
        assert!(requests[0].system_prompt.contains("majority language"));
        assert!(markdown.contains("Narrative timeline."));
    }

    #[test]
    fn write_progress_strips_llm_think_blocks_before_writing_markdown() {
        let _mock_guard = crate::llm::test_support::guard();
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "progress-think".into(),
                workdir: "/Users/kc/ProgressThink".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/progress-think.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Progress think".into(),
                }],
            }],
        )
        .unwrap();
        let project = crate::db::list_projects(&connection).unwrap().remove(0);
        let stored = crate::db::unprocessed_session_by_session_id(&connection, "progress-think")
            .unwrap()
            .remove(0);
        let (task_id, _) = crate::db::upsert_task(
            &connection,
            stored.project_id,
            "progress-think",
            "Progress Think",
            "Brief",
            "developing",
            "/tmp/progress-think.md",
        )
        .unwrap();
        crate::db::mark_session_processed(
            &connection,
            stored.id,
            task_id,
            "Progress Think",
            "Session summary",
            "/tmp/progress-think-session.md",
        )
        .unwrap();
        crate::llm::test_support::set_markdown_responses(vec![
            "<think>final thought</think>\n\n# Project Progress\n\nVisible progress.",
        ]);
        let settings = empty_settings();

        write_progress(&paths, &settings, &project.slug).unwrap();
        let requests = crate::llm::test_support::take_requests();
        let markdown = std::fs::read_to_string(
            paths
                .projects_dir
                .join(format!("{}/progress.md", project.slug)),
        )
        .unwrap();

        assert!(!markdown.contains("<think>"));
        assert!(!markdown.contains("final thought"));
        assert!(markdown.contains("Visible progress."));
        assert_eq!(requests.len(), 1);
    }

    #[test]
    fn session_analysis_prompt_uses_session_language_and_user_assistant_messages_only() {
        let _mock_guard = crate::llm::test_support::guard();
        crate::llm::test_support::set_json_responses(vec![serde_json::json!({
            "task_name": "localization",
            "title": "本地化",
            "brief": "继续使用中文总结。",
            "session_title": "中文会话",
            "summary": "用户要求保持中文。",
            "memories": ["用户要求保持中文。"],
            "entities": [{"name": "KittyNest", "type": "project"}]
        })]);
        let settings = empty_settings();
        let session = crate::models::StoredSession {
            id: 1,
            source: "codex".into(),
            session_id: "localized".into(),
            project_id: 1,
            project_slug: "KittyNest".into(),
            task_id: None,
            workdir: "/tmp/KittyNest".into(),
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:01Z".into(),
            messages: vec![
                RawMessage {
                    role: "system".into(),
                    content: "hidden prompt".into(),
                },
                RawMessage {
                    role: "user".into(),
                    content: "请用中文总结这个任务".into(),
                },
                RawMessage {
                    role: "tool".into(),
                    content: "tool output".into(),
                },
                RawMessage {
                    role: "assistant".into(),
                    content: "已经完成中文总结。".into(),
                },
            ],
        };

        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let analysis = analyze_session(&paths, &settings, &session).unwrap();
        let requests = crate::llm::test_support::take_requests();

        assert_eq!(analysis.session_summary, "用户要求保持中文。");
        assert!(requests[0].system_prompt.contains("same language"));
        assert!(requests[0].user_prompt.contains("请用中文总结这个任务"));
        assert!(requests[0].user_prompt.contains("已经完成中文总结"));
        assert!(!requests[0].user_prompt.contains("hidden prompt"));
        assert!(!requests[0].user_prompt.contains("tool output"));
    }

    fn empty_settings() -> LlmSettings {
        let mut settings = crate::config::default_llm_settings();
        settings.id = "test-default".into();
        settings.remark = "Default".into();
        settings.provider = "Test".into();
        settings.base_url = "".into();
        settings.interface = "openai".into();
        settings.model = "".into();
        settings.api_key = "".into();
        settings
    }

    fn session_response(task_slug: &str, title: &str, summary: &str) -> serde_json::Value {
        serde_json::json!({
            "task_name": task_slug,
            "title": title,
            "brief": summary,
            "session_title": title,
            "summary": summary,
            "memories": [summary],
            "entities": [{"name": "KittyNest", "type": "project"}]
        })
    }

    fn session_memory_draft() -> crate::memory::SessionMemoryDraft {
        crate::memory::SessionMemoryDraft {
            memories: vec![
                "CozoDB is the graph store.".into(),
                "User prefers short memory facts.".into(),
            ],
            entities: vec![
                crate::memory::MemoryEntity {
                    name: "MemoryProject".into(),
                    entity_type: "project".into(),
                },
                crate::memory::MemoryEntity {
                    name: "CozoDB".into(),
                    entity_type: "technology".into(),
                },
            ],
        }
    }

    fn seed_analyzed_session(
        paths: &AppPaths,
        session_id: &str,
        project_slug: &str,
    ) -> crate::models::StoredSession {
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::create_dir_all(&paths.projects_dir).unwrap();
        let mut connection = open(paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: session_id.into(),
                workdir: format!("/Users/kc/{project_slug}"),
                created_at: "2026-04-27T00:00:00Z".into(),
                updated_at: "2026-04-27T00:00:01Z".into(),
                raw_path: format!("/tmp/{session_id}.jsonl"),
                messages: vec![
                    RawMessage {
                        role: "user".into(),
                        content: "Remember SQLite".into(),
                    },
                    RawMessage {
                        role: "assistant".into(),
                        content: "SQLite memory captured.".into(),
                    },
                ],
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
        session
    }

    fn seed_rebuildable_session(
        paths: &AppPaths,
        session_id: &str,
        project_slug: &str,
    ) -> crate::models::StoredSession {
        let session = seed_analyzed_session(paths, session_id, project_slug);
        let connection = open(paths).unwrap();
        migrate(&connection).unwrap();
        connection
            .execute(
                "UPDATE sessions SET processed_at = '2026-04-27T10:00:00Z' WHERE id = ?1",
                rusqlite::params![session.id],
            )
            .unwrap();
        crate::memory::generate_session_memory_at(
            paths,
            &connection,
            &session,
            &session_memory_draft(),
            "2026-04-27T09:00:00Z",
        )
        .unwrap();
        session
    }

    fn stored_test_session(session_id: &str) -> crate::models::StoredSession {
        crate::models::StoredSession {
            id: 1,
            source: "codex".into(),
            session_id: session_id.into(),
            project_id: 1,
            project_slug: "KittyNest".into(),
            task_id: None,
            workdir: "/tmp/KittyNest".into(),
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:01Z".into(),
            messages: vec![
                RawMessage {
                    role: "user".into(),
                    content: "Analyze this session".into(),
                },
                RawMessage {
                    role: "assistant".into(),
                    content: "Session analyzed".into(),
                },
            ],
        }
    }
}

