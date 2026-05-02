#[cfg(test)]
mod tests {
    use super::{
        cancel_job, claim_next_job, delete_task_if_empty, enqueue_analyze_project_sessions,
        enqueue_analyze_session, enqueue_analyze_sessions, enqueue_review_project,
        enqueue_scan_sources, ensure_project_for_workdir, list_active_jobs,
        list_llm_provider_calls, list_projects, list_sessions, list_tasks, mark_session_failed,
        mark_session_processed, mark_session_processed_with_optional_task,
        mark_stale_running_jobs_queued, migrate, open, record_llm_provider_call,
        replace_session_memories, reset_all_memories, reset_all_projects, reset_all_sessions,
        reset_all_tasks, session_memories_by_session_id, unprocessed_session_by_session_id,
        unprocessed_sessions, unprocessed_sessions_updated_after, update_job_progress,
        update_project_progress, update_project_review, update_task_status, upsert_raw_sessions,
        upsert_task,
    };
    use crate::models::{AppPaths, MemorySearchResultRecord, RawMessage, RawSession};

    #[test]
    fn upsert_raw_sessions_deduplicates_by_source_and_session_id() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let session = RawSession {
            source: "codex".into(),
            session_id: "abc".into(),
            workdir: "/tmp/demo".into(),
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:01Z".into(),
            raw_path: "/tmp/session.jsonl".into(),
            messages: vec![RawMessage {
                role: "user".into(),
                content: "Build dashboard".into(),
            }],
        };

        assert_eq!(
            upsert_raw_sessions(&mut connection, &[session.clone()]).unwrap(),
            1
        );
        assert_eq!(upsert_raw_sessions(&mut connection, &[session]).unwrap(), 0);
    }

    #[test]
    fn task_records_include_created_at_and_saved_paths() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let project_id = ensure_project_for_workdir(
            &connection,
            "/tmp/saved-task-project",
            "codex",
            "2026-04-28T08:00:00Z",
        )
        .unwrap();
        let task_dir = paths
            .projects_dir
            .join("saved-task-project")
            .join("tasks")
            .join("draft");
        std::fs::create_dir_all(&task_dir).unwrap();
        let description = task_dir.join("description.md");
        let session = task_dir.join("session.json");
        std::fs::write(&description, "Description").unwrap();
        std::fs::write(&session, "{}").unwrap();
        upsert_task(
            &connection,
            project_id,
            "draft",
            "Draft Task",
            "brief",
            "discussing",
            &description.to_string_lossy(),
        )
        .unwrap();

        let task = list_tasks(&connection).unwrap().remove(0);

        assert_eq!(task.created_at, task.updated_at);
        assert_eq!(
            task.description_path.as_deref(),
            Some(description.to_string_lossy().as_ref())
        );
        assert_eq!(
            task.session_path.as_deref(),
            Some(session.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn upsert_raw_sessions_updates_modified_existing_session_and_marks_pending() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let session = RawSession {
            source: "codex".into(),
            session_id: "abc".into(),
            workdir: "/tmp/demo".into(),
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:01Z".into(),
            raw_path: "/tmp/session.jsonl".into(),
            messages: vec![RawMessage {
                role: "user".into(),
                content: "Build dashboard".into(),
            }],
        };
        upsert_raw_sessions(&mut connection, &[session.clone()]).unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "abc")
            .unwrap()
            .remove(0);
        let (task_id, _) = upsert_task(
            &connection,
            stored.project_id,
            "old-task",
            "Old Task",
            "Old brief",
            "developing",
            "/tmp/old-task.md",
        )
        .unwrap();
        mark_session_processed(
            &connection,
            stored.id,
            task_id,
            "Old title",
            "Old summary",
            "/tmp/old-session.md",
        )
        .unwrap();

        let mut changed = session;
        changed.updated_at = "2026-04-26T00:10:00Z".into();
        changed.raw_path = "/tmp/session-updated.jsonl".into();
        changed.messages = vec![RawMessage {
            role: "user".into(),
            content: "Build dashboard and session updater".into(),
        }];

        assert_eq!(upsert_raw_sessions(&mut connection, &[changed]).unwrap(), 1);
        let session = list_sessions(&connection)
            .unwrap()
            .into_iter()
            .find(|session| session.session_id == "abc")
            .unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "abc")
            .unwrap()
            .remove(0);

        assert_eq!(session.status, "pending");
        assert_eq!(session.updated_at, "2026-04-26T00:10:00Z");
        assert_eq!(session.raw_path, "/tmp/session-updated.jsonl");
        assert_eq!(session.task_slug.as_deref(), Some("old-task"));
        assert!(session.summary.is_none());
        assert_eq!(stored.task_id, Some(task_id));
        assert_eq!(
            stored.messages[0].content,
            "Build dashboard and session updater"
        );
    }

    #[test]
    fn upsert_raw_sessions_disambiguates_projects_with_same_directory_name() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let first = RawSession {
            source: "codex".into(),
            session_id: "first".into(),
            workdir: "/tmp/one/Demo".into(),
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:01Z".into(),
            raw_path: "/tmp/first.jsonl".into(),
            messages: vec![RawMessage {
                role: "user".into(),
                content: "First project".into(),
            }],
        };
        let mut second = first.clone();
        second.session_id = "second".into();
        second.workdir = "/tmp/two/Demo".into();
        second.raw_path = "/tmp/second.jsonl".into();

        assert_eq!(
            upsert_raw_sessions(&mut connection, &[first, second]).unwrap(),
            2
        );

        let projects = list_projects(&connection).unwrap();
        let mut slugs = projects
            .iter()
            .map(|project| project.slug.as_str())
            .collect::<Vec<_>>();
        slugs.sort_unstable();
        assert_eq!(projects.len(), 2);
        assert_ne!(slugs[0], slugs[1]);
        assert!(slugs.contains(&"Demo"));
    }

    #[test]
    fn enqueue_analyze_sessions_persists_total_and_active_job() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "abc".into(),
                workdir: "/tmp/demo".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/session.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Analyze me".into(),
                }],
            }],
        )
        .unwrap();

        let result = enqueue_analyze_sessions(&connection, None).unwrap();
        let jobs = list_active_jobs(&connection).unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, result.job_id);
        assert_eq!(jobs[0].kind, "analyze_sessions");
        assert_eq!(jobs[0].scope, "all_unprocessed");
        assert_eq!(jobs[0].status, "queued");
        assert_eq!(jobs[0].total, 1);
        assert_eq!(jobs[0].completed, 0);
        assert_eq!(jobs[0].pending, 1);
    }

    #[test]
    fn enqueue_sync_to_obsidian_persists_single_unit_total() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let connection = open(&paths).unwrap();
        migrate(&connection).unwrap();

        let job_id = super::enqueue_sync_to_obsidian(&connection, "incremental").unwrap();
        let jobs = list_active_jobs(&connection).unwrap();

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, job_id);
        assert_eq!(jobs[0].kind, "sync_to_obsidian");
        assert_eq!(jobs[0].scope, "incremental");
        assert_eq!(jobs[0].total, 1);
        assert_eq!(jobs[0].completed, 0);
        assert_eq!(jobs[0].pending, 1);
    }

    #[test]
    fn enqueue_analyze_sessions_filters_by_updated_after() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "recent".into(),
                    workdir: "/tmp/demo".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:00:01Z".into(),
                    raw_path: "/tmp/recent.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Analyze recent".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "old".into(),
                    workdir: "/tmp/demo".into(),
                    created_at: "2026-04-01T00:00:00Z".into(),
                    updated_at: "2026-04-01T00:00:01Z".into(),
                    raw_path: "/tmp/old.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Skip old".into(),
                    }],
                },
            ],
        )
        .unwrap();

        let result = enqueue_analyze_sessions(&connection, Some("2026-04-19T00:00:00Z")).unwrap();
        let claimed = claim_next_job(&connection).unwrap().unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(
            claimed.updated_after.as_deref(),
            Some("2026-04-19T00:00:00Z")
        );
    }

    #[test]
    fn enqueue_analyze_session_persists_single_session_job() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "single".into(),
                workdir: "/tmp/demo".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/session.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Analyze one".into(),
                }],
            }],
        )
        .unwrap();

        let result = enqueue_analyze_session(&connection, "single").unwrap();
        let jobs = list_active_jobs(&connection).unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(jobs[0].kind, "analyze_session");
        assert_eq!(jobs[0].scope, "single_session");
        assert_eq!(jobs[0].session_id.as_deref(), Some("single"));
    }

    #[test]
    fn enqueue_analyze_project_sessions_counts_only_that_project() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "target".into(),
                    workdir: "/tmp/target".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:00:01Z".into(),
                    raw_path: "/tmp/target.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Analyze target".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "other".into(),
                    workdir: "/tmp/other".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:00:01Z".into(),
                    raw_path: "/tmp/other.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Analyze other".into(),
                    }],
                },
            ],
        )
        .unwrap();
        let target = list_projects(&connection)
            .unwrap()
            .into_iter()
            .find(|project| project.workdir == "/tmp/target")
            .unwrap();

        let result = enqueue_analyze_project_sessions(&connection, &target.slug).unwrap();
        let jobs = list_active_jobs(&connection).unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(jobs[0].kind, "analyze_project_sessions");
        assert_eq!(jobs[0].scope, "project_unprocessed");
        assert_eq!(jobs[0].project_slug.as_deref(), Some(target.slug.as_str()));
    }

    #[test]
    fn enqueue_analyze_project_counts_pending_only_inside_latest_twenty_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let raw_sessions = (1..=22)
            .map(|index| RawSession {
                source: "codex".into(),
                session_id: format!("project-window-{index:02}"),
                workdir: "/tmp/project-window".into(),
                created_at: format!("2026-04-26T00:{index:02}:00Z"),
                updated_at: format!("2026-04-26T00:{index:02}:30Z"),
                raw_path: format!("/tmp/project-window-{index:02}.jsonl"),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: format!("Project window session {index:02}"),
                }],
            })
            .collect::<Vec<_>>();
        upsert_raw_sessions(&mut connection, &raw_sessions).unwrap();
        let project = list_projects(&connection).unwrap().remove(0);
        for index in 3..=22 {
            let stored = unprocessed_session_by_session_id(
                &connection,
                &format!("project-window-{index:02}"),
            )
            .unwrap()
            .remove(0);
            mark_session_processed_with_optional_task(
                &connection,
                stored.id,
                None,
                &format!("Project Window {index:02}"),
                "Summary",
                "/tmp/summary.md",
            )
            .unwrap();
        }

        let result = super::enqueue_analyze_project(&connection, &project.slug).unwrap();
        let sessions =
            super::project_sessions_needing_analysis_limited(&connection, &project.slug, 20)
                .unwrap();

        assert_eq!(result.total, 4);
        assert!(sessions.is_empty());
    }

    #[test]
    fn enqueue_review_project_persists_project_job() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let session = RawSession {
            source: "codex".into(),
            session_id: "review".into(),
            workdir: "/tmp/review-project".into(),
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:01Z".into(),
            raw_path: "/tmp/review.jsonl".into(),
            messages: vec![RawMessage {
                role: "user".into(),
                content: "Review project".into(),
            }],
        };
        upsert_raw_sessions(&mut connection, &[session]).unwrap();
        let project = list_projects(&connection).unwrap().remove(0);

        let result = enqueue_review_project(&connection, &project.slug).unwrap();
        let jobs = list_active_jobs(&connection).unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].kind, "review_project");
        assert_eq!(jobs[0].scope, "project_summary");
        assert_eq!(jobs[0].project_slug.as_deref(), Some(project.slug.as_str()));
    }

    #[test]
    fn enqueue_scan_sources_persists_source_scan_job() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let connection = open(&paths).unwrap();
        migrate(&connection).unwrap();

        let result = enqueue_scan_sources(&connection).unwrap();
        let jobs = list_active_jobs(&connection).unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].kind, "scan_sources");
        assert_eq!(jobs[0].scope, "source_scan");
    }

    #[test]
    fn enqueue_startup_maintenance_orders_scan_recent_session_analysis_and_project_coordinator() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let connection = open(&paths).unwrap();
        migrate(&connection).unwrap();

        super::enqueue_startup_maintenance(&connection, "2026-04-25T00:00:00Z").unwrap();
        let jobs = list_active_jobs(&connection).unwrap();

        assert_eq!(jobs.len(), 3);
        assert_eq!(jobs[0].kind, "scan_sources");
        assert_eq!(jobs[0].scope, "source_scan");
        assert_eq!(jobs[1].kind, "analyze_sessions");
        assert_eq!(jobs[1].scope, "all_unprocessed");
        assert_eq!(
            jobs[1].updated_after.as_deref(),
            Some("2026-04-25T00:00:00Z")
        );
        assert_eq!(jobs[2].kind, "analyze_recent_projects");
        assert_eq!(jobs[2].scope, "startup_recent_projects");
        assert_eq!(
            jobs[2].updated_after.as_deref(),
            Some("2026-04-25T00:00:00Z")
        );
        assert_eq!(jobs[2].total, 1);
    }

    #[test]
    fn enqueue_rebuild_memories_treats_missing_memory_as_epoch() {
        let temp = tempfile::tempdir().unwrap();
        let connection = test_connection_with_session(&temp, "rebuild-db");
        let stored = unprocessed_session_by_session_id(&connection, "rebuild-db")
            .unwrap()
            .remove(0);
        mark_session_processed_with_optional_task(
            &connection,
            stored.id,
            None,
            "Rebuild DB",
            "Summary",
            "/tmp/rebuild-db/summary.md",
        )
        .unwrap();

        let result = super::enqueue_rebuild_memories(&connection).unwrap();

        assert_eq!(result.total, 2);
    }

    #[test]
    fn enqueue_rebuild_memories_counts_only_memory_older_than_summary() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "fresh-memory".into(),
                    workdir: "/tmp/memory-refresh".into(),
                    created_at: "2026-04-27T00:00:00Z".into(),
                    updated_at: "2026-04-27T00:00:01Z".into(),
                    raw_path: "/tmp/fresh-memory.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Fresh memory".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "stale-memory".into(),
                    workdir: "/tmp/memory-refresh".into(),
                    created_at: "2026-04-27T00:00:00Z".into(),
                    updated_at: "2026-04-27T00:00:02Z".into(),
                    raw_path: "/tmp/stale-memory.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Stale memory".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "future-memory".into(),
                    workdir: "/tmp/memory-refresh".into(),
                    created_at: "2026-04-27T00:00:00Z".into(),
                    updated_at: "2026-04-27T00:00:04Z".into(),
                    raw_path: "/tmp/future-memory.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Future memory".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "missing-memory".into(),
                    workdir: "/tmp/memory-refresh".into(),
                    created_at: "2026-04-27T00:00:00Z".into(),
                    updated_at: "2026-04-27T00:00:03Z".into(),
                    raw_path: "/tmp/missing-memory.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Missing memory".into(),
                    }],
                },
            ],
        )
        .unwrap();

        let fresh = unprocessed_session_by_session_id(&connection, "fresh-memory")
            .unwrap()
            .remove(0);
        mark_session_processed_with_optional_task(
            &connection,
            fresh.id,
            None,
            "Fresh Memory",
            "Summary",
            "/tmp/fresh-memory/summary.md",
        )
        .unwrap();
        replace_session_memories(&connection, &fresh, &["fresh memory".to_string()]).unwrap();
        connection
            .execute(
                "UPDATE sessions SET processed_at = '2026-04-27T10:00:00Z' WHERE id = ?1",
                rusqlite::params![fresh.id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE session_memories SET created_at = '2026-04-27T10:00:00Z' WHERE session_row_id = ?1",
                rusqlite::params![fresh.id],
            )
            .unwrap();

        let stale = unprocessed_session_by_session_id(&connection, "stale-memory")
            .unwrap()
            .remove(0);
        mark_session_processed_with_optional_task(
            &connection,
            stale.id,
            None,
            "Stale Memory",
            "Summary",
            "/tmp/stale-memory/summary.md",
        )
        .unwrap();
        replace_session_memories(&connection, &stale, &["old memory".to_string()]).unwrap();
        connection
            .execute(
                "UPDATE sessions SET processed_at = '2026-04-27T11:00:00Z' WHERE id = ?1",
                rusqlite::params![stale.id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE session_memories SET created_at = '2026-04-27T10:30:00Z' WHERE session_row_id = ?1",
                rusqlite::params![stale.id],
            )
            .unwrap();

        let future = unprocessed_session_by_session_id(&connection, "future-memory")
            .unwrap()
            .remove(0);
        mark_session_processed_with_optional_task(
            &connection,
            future.id,
            None,
            "Future Memory",
            "Summary",
            "/tmp/future-memory/summary.md",
        )
        .unwrap();
        replace_session_memories(&connection, &future, &["future memory".to_string()]).unwrap();
        connection
            .execute(
                "UPDATE sessions SET processed_at = '2026-04-27T10:00:00Z' WHERE id = ?1",
                rusqlite::params![future.id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE session_memories SET created_at = '2026-04-27T10:30:00Z' WHERE session_row_id = ?1",
                rusqlite::params![future.id],
            )
            .unwrap();

        let missing = unprocessed_session_by_session_id(&connection, "missing-memory")
            .unwrap()
            .remove(0);
        mark_session_processed_with_optional_task(
            &connection,
            missing.id,
            None,
            "Missing Memory",
            "Summary",
            "/tmp/missing-memory/summary.md",
        )
        .unwrap();

        let result = super::enqueue_rebuild_memories(&connection).unwrap();
        let sessions = super::sessions_needing_memory_rebuild(&connection).unwrap();

        assert_eq!(result.total, 3);
        assert_eq!(
            sessions
                .into_iter()
                .map(|session| session.session_id)
                .collect::<Vec<_>>(),
            vec!["missing-memory".to_string(), "stale-memory".to_string()]
        );
    }

    #[test]
    fn enqueue_rebuild_memories_includes_entity_disambiguation_when_no_sessions_need_rebuild() {
        let temp = tempfile::tempdir().unwrap();
        let connection = test_connection_with_session(&temp, "fresh-rebuild-db");
        let stored = unprocessed_session_by_session_id(&connection, "fresh-rebuild-db")
            .unwrap()
            .remove(0);
        mark_session_processed_with_optional_task(
            &connection,
            stored.id,
            None,
            "Fresh Rebuild DB",
            "Summary",
            "/tmp/fresh-rebuild-db/summary.md",
        )
        .unwrap();
        replace_session_memories(&connection, &stored, &["fresh memory".to_string()]).unwrap();
        connection
            .execute(
                "UPDATE sessions SET processed_at = '2026-04-27T10:00:00Z' WHERE id = ?1",
                rusqlite::params![stored.id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE session_memories SET created_at = '2026-04-27T10:00:00Z' WHERE session_row_id = ?1",
                rusqlite::params![stored.id],
            )
            .unwrap();

        let result = super::enqueue_rebuild_memories(&connection).unwrap();

        assert_eq!(result.total, 1);
        let jobs = list_active_jobs(&connection).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].kind, "rebuild_memories");
        assert_eq!(jobs[0].total, 1);
    }

    #[test]
    fn memory_search_results_replace_latest_rows() {
        let temp = tempfile::tempdir().unwrap();
        let connection = test_connection_with_session(&temp, "search-db");
        let stored = unprocessed_session_by_session_id(&connection, "search-db")
            .unwrap()
            .remove(0);
        replace_session_memories(&connection, &stored, &["SQLite memory".to_string()]).unwrap();
        let job = super::enqueue_search_memories(&connection, "sqlite").unwrap();
        let search_id = super::create_memory_search(&connection, job.job_id, "sqlite").unwrap();

        super::replace_memory_search_results(
            &connection,
            search_id,
            "completed",
            "1 memory found",
            &[MemorySearchResultRecord {
                source_session: "search-db".into(),
                session_title: "search-db".into(),
                project_slug: "search-db".into(),
                memory: "SQLite memory".into(),
                ordinal: 0,
            }],
        )
        .unwrap();

        let latest = super::latest_memory_search(&connection).unwrap().unwrap();
        assert_eq!(latest.query, "sqlite");
        assert_eq!(latest.results[0].memory, "SQLite memory");
    }

    #[test]
    fn list_sessions_exposes_updated_at_and_status() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let session = RawSession {
            source: "codex".into(),
            session_id: "status".into(),
            workdir: "/tmp/status-project".into(),
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:01Z".into(),
            raw_path: "/tmp/status.jsonl".into(),
            messages: vec![RawMessage {
                role: "user".into(),
                content: "Check status".into(),
            }],
        };
        upsert_raw_sessions(&mut connection, &[session]).unwrap();

        let sessions = list_sessions(&connection).unwrap();

        assert_eq!(sessions[0].updated_at, "2026-04-26T00:00:01Z");
        assert_eq!(sessions[0].status, "pending");
    }

    #[test]
    fn unprocessed_sessions_returns_newest_updated_first() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "oldest".into(),
                    workdir: "/tmp/order-project".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:01:00Z".into(),
                    raw_path: "/tmp/oldest.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Oldest".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "newest".into(),
                    workdir: "/tmp/order-project".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:03:00Z".into(),
                    raw_path: "/tmp/newest.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Newest".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "middle".into(),
                    workdir: "/tmp/order-project".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:02:00Z".into(),
                    raw_path: "/tmp/middle.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Middle".into(),
                    }],
                },
            ],
        )
        .unwrap();

        let sessions = unprocessed_sessions(&connection).unwrap();

        assert_eq!(
            sessions
                .iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["newest", "middle", "oldest"]
        );
    }

    #[test]
    fn analyze_sessions_updated_after_includes_pending_and_failed_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "recent-pending".into(),
                    workdir: "/tmp/analyze-range".into(),
                    created_at: "2026-04-27T00:00:00Z".into(),
                    updated_at: "2026-04-27T00:00:01Z".into(),
                    raw_path: "/tmp/recent-pending.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Recent pending".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "recent-failed".into(),
                    workdir: "/tmp/analyze-range".into(),
                    created_at: "2026-04-27T00:00:00Z".into(),
                    updated_at: "2026-04-27T00:00:02Z".into(),
                    raw_path: "/tmp/recent-failed.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Recent failed".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "old-failed".into(),
                    workdir: "/tmp/analyze-range".into(),
                    created_at: "2026-04-10T00:00:00Z".into(),
                    updated_at: "2026-04-10T00:00:01Z".into(),
                    raw_path: "/tmp/old-failed.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Old failed".into(),
                    }],
                },
            ],
        )
        .unwrap();
        let recent_failed = unprocessed_session_by_session_id(&connection, "recent-failed")
            .unwrap()
            .remove(0);
        mark_session_failed(&connection, recent_failed.id, "temporary").unwrap();
        let old_failed = unprocessed_session_by_session_id(&connection, "old-failed")
            .unwrap()
            .remove(0);
        mark_session_failed(&connection, old_failed.id, "old").unwrap();

        let job = enqueue_analyze_sessions(&connection, Some("2026-04-20T00:00:00Z")).unwrap();
        let sessions =
            unprocessed_sessions_updated_after(&connection, "2026-04-20T00:00:00Z").unwrap();

        assert_eq!(job.total, 2);
        assert_eq!(
            sessions
                .iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["recent-failed", "recent-pending"]
        );
    }

    #[test]
    fn reset_all_sessions_deletes_session_records() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "reset-session".into(),
                workdir: "/tmp/reset-project".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/reset-session.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Reset session".into(),
                }],
            }],
        )
        .unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "reset-session")
            .unwrap()
            .remove(0);
        let (task_id, _) = upsert_task(
            &connection,
            stored.project_id,
            "reset-task",
            "Reset Task",
            "Brief",
            "done",
            "/tmp/reset-task.md",
        )
        .unwrap();
        mark_session_processed(
            &connection,
            stored.id,
            task_id,
            "Analyzed",
            "Summary",
            "/tmp/reset-session.md",
        )
        .unwrap();

        assert_eq!(reset_all_sessions(&connection).unwrap(), 1);

        assert!(list_sessions(&connection).unwrap().is_empty());
    }

    #[test]
    fn replace_session_memories_rewrites_rows_for_one_session() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "memory-db".into(),
                workdir: "/tmp/memory-db".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/memory-db.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Remember me".into(),
                }],
            }],
        )
        .unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "memory-db")
            .unwrap()
            .remove(0);

        replace_session_memories(
            &connection,
            &stored,
            &["first memory".to_string(), "second memory".to_string()],
        )
        .unwrap();
        replace_session_memories(&connection, &stored, &["replacement".to_string()]).unwrap();

        assert_eq!(
            session_memories_by_session_id(&connection, "memory-db").unwrap(),
            vec!["replacement".to_string()]
        );
        assert_eq!(reset_all_memories(&connection).unwrap(), 1);
        assert!(session_memories_by_session_id(&connection, "memory-db")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn reset_all_projects_deletes_projects_and_related_records() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "project-reset".into(),
                workdir: "/tmp/project-reset".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/project-reset.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Reset project".into(),
                }],
            }],
        )
        .unwrap();
        let (project_id, project) = super::get_project_by_slug(
            &connection,
            &list_projects(&connection).unwrap().remove(0).slug,
        )
        .unwrap()
        .unwrap();
        update_project_review(&connection, project_id, "/tmp/info.md").unwrap();
        update_project_progress(&connection, &project.slug, "/tmp/progress.md").unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "project-reset")
            .unwrap()
            .remove(0);
        let (task_id, _) = upsert_task(
            &connection,
            stored.project_id,
            "project-reset",
            "Project Reset",
            "Brief",
            "developing",
            "/tmp/project-reset.md",
        )
        .unwrap();
        mark_session_processed(
            &connection,
            stored.id,
            task_id,
            "Project Reset",
            "Summary",
            "/tmp/project-reset-session.md",
        )
        .unwrap();

        assert_eq!(reset_all_projects(&connection).unwrap(), 1);

        assert!(list_projects(&connection).unwrap().is_empty());
        assert!(list_sessions(&connection).unwrap().is_empty());
        assert!(list_tasks(&connection).unwrap().is_empty());
    }

    #[test]
    fn reset_all_tasks_deletes_task_records() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "task-reset".into(),
                workdir: "/tmp/task-reset".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/task-reset.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Reset task".into(),
                }],
            }],
        )
        .unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "task-reset")
            .unwrap()
            .remove(0);
        upsert_task(
            &connection,
            stored.project_id,
            "task-reset",
            "Task Reset",
            "Brief",
            "done",
            "/tmp/task-reset.md",
        )
        .unwrap();

        assert_eq!(reset_all_tasks(&connection).unwrap(), 1);

        assert!(super::list_tasks(&connection).unwrap().is_empty());
    }

    #[test]
    fn update_task_status_rejects_non_discussing_empty_tasks() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: "empty-task".into(),
                workdir: "/tmp/empty-task".into(),
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:01Z".into(),
                raw_path: "/tmp/empty-task.jsonl".into(),
                messages: vec![RawMessage {
                    role: "user".into(),
                    content: "Create empty task".into(),
                }],
            }],
        )
        .unwrap();
        let stored = unprocessed_session_by_session_id(&connection, "empty-task")
            .unwrap()
            .remove(0);
        upsert_task(
            &connection,
            stored.project_id,
            "empty-task",
            "Empty Task",
            "Brief",
            "discussing",
            "/tmp/empty-task.md",
        )
        .unwrap();

        let error = super::update_task_status(&connection, "empty-task", "empty-task", "done")
            .unwrap_err()
            .to_string();
        let task = super::list_tasks(&connection).unwrap().remove(0);

        assert!(error.contains("empty task"));
        assert_eq!(task.status, "discussing");
    }

    #[test]
    fn saved_empty_tasks_can_move_between_all_task_statuses() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        let project_id = ensure_project_for_workdir(
            &connection,
            "/tmp/status-project",
            "codex",
            "2026-04-28T08:00:00Z",
        )
        .unwrap();
        let task_dir = paths
            .projects_dir
            .join("status-project")
            .join("tasks")
            .join("saved");
        std::fs::create_dir_all(&task_dir).unwrap();
        let description = task_dir.join("description.md");
        let session = task_dir.join("session.json");
        std::fs::write(&description, "Description").unwrap();
        std::fs::write(&session, "{}").unwrap();
        upsert_task(
            &connection,
            project_id,
            "saved",
            "Saved",
            "brief",
            "discussing",
            &description.to_string_lossy(),
        )
        .unwrap();

        assert!(update_task_status(&connection, "status-project", "saved", "developing").unwrap());
        assert!(update_task_status(&connection, "status-project", "saved", "done").unwrap());
    }

    #[test]
    fn delete_task_if_empty_deletes_only_zero_session_tasks() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[
                RawSession {
                    source: "codex".into(),
                    session_id: "linked-task".into(),
                    workdir: "/tmp/delete-task".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:00:01Z".into(),
                    raw_path: "/tmp/linked-task.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Linked task".into(),
                    }],
                },
                RawSession {
                    source: "codex".into(),
                    session_id: "empty-task".into(),
                    workdir: "/tmp/delete-task".into(),
                    created_at: "2026-04-26T00:00:00Z".into(),
                    updated_at: "2026-04-26T00:00:01Z".into(),
                    raw_path: "/tmp/empty-task.jsonl".into(),
                    messages: vec![RawMessage {
                        role: "user".into(),
                        content: "Empty task".into(),
                    }],
                },
            ],
        )
        .unwrap();
        let linked = unprocessed_session_by_session_id(&connection, "linked-task")
            .unwrap()
            .remove(0);
        let empty = unprocessed_session_by_session_id(&connection, "empty-task")
            .unwrap()
            .remove(0);
        let (linked_task_id, _) = upsert_task(
            &connection,
            linked.project_id,
            "linked-task",
            "Linked Task",
            "Brief",
            "developing",
            "/tmp/linked-task.md",
        )
        .unwrap();
        upsert_task(
            &connection,
            empty.project_id,
            "empty-task",
            "Empty Task",
            "Brief",
            "discussing",
            "/tmp/empty-task.md",
        )
        .unwrap();
        mark_session_processed(
            &connection,
            linked.id,
            linked_task_id,
            "Linked",
            "Summary",
            "/tmp/linked-session.md",
        )
        .unwrap();

        let linked_error = delete_task_if_empty(&connection, "delete-task", "linked-task")
            .unwrap_err()
            .to_string();
        let deleted = delete_task_if_empty(&connection, "delete-task", "empty-task").unwrap();
        let remaining = super::list_tasks(&connection).unwrap();

        assert!(linked_error.contains("has sessions"));
        assert!(deleted);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].slug, "linked-task");
    }

    #[test]
    fn active_jobs_omits_completed_and_recovers_running_jobs() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let connection = open(&paths).unwrap();
        migrate(&connection).unwrap();

        let first = enqueue_analyze_sessions(&connection, None).unwrap();
        let claimed = claim_next_job(&connection).unwrap().unwrap();
        assert_eq!(claimed.id, first.job_id);
        update_job_progress(&connection, claimed.id, 0, 0, "waiting").unwrap();

        mark_stale_running_jobs_queued(&connection).unwrap();
        let reclaimed = claim_next_job(&connection).unwrap().unwrap();
        assert_eq!(reclaimed.status, "running");

        super::complete_job(&connection, reclaimed.id, "completed").unwrap();
        assert!(list_active_jobs(&connection).unwrap().is_empty());
    }

    fn test_connection_with_session(
        temp: &tempfile::TempDir,
        session_id: &str,
    ) -> rusqlite::Connection {
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let mut connection = open(&paths).unwrap();
        migrate(&connection).unwrap();
        upsert_raw_sessions(
            &mut connection,
            &[RawSession {
                source: "codex".into(),
                session_id: session_id.into(),
                workdir: format!("/tmp/{session_id}"),
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
        connection
    }

    #[test]
    fn cancel_job_hides_active_job() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let connection = open(&paths).unwrap();
        migrate(&connection).unwrap();

        let job = enqueue_analyze_sessions(&connection, None).unwrap();

        assert!(cancel_job(&connection, job.job_id).unwrap());
        assert!(list_active_jobs(&connection).unwrap().is_empty());
    }

    #[test]
    fn provider_call_counts_return_positive_counts_by_provider() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let connection = open(&paths).unwrap();
        migrate(&connection).unwrap();

        record_llm_provider_call(&connection, "OpenRouter").unwrap();
        record_llm_provider_call(&connection, "OpenRouter").unwrap();
        record_llm_provider_call(&connection, "Anthropic").unwrap();
        record_llm_provider_call(&connection, "   ").unwrap();

        let counts = list_llm_provider_calls(&connection).unwrap();

        assert_eq!(counts.len(), 2);
        assert_eq!(counts[0].provider, "OpenRouter");
        assert_eq!(counts[0].calls, 2);
        assert_eq!(counts[1].provider, "Anthropic");
        assert_eq!(counts[1].calls, 1);
    }
}
