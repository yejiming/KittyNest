pub fn import_historical_sessions(paths: &AppPaths) -> anyhow::Result<ImportSummary> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let sessions = crate::db::unprocessed_sessions(&connection)?;
    let settings = crate::config::resolve_llm_settings(
        &crate::config::read_llm_settings(paths)?,
        crate::config::LlmScenario::Memory,
    );
    let mut summary = ImportSummary::default();

    for session in sessions {
        let (_project_slug, created) =
            analyze_and_store_session(paths, &connection, &settings, &session)?;
        if created {
            summary.tasks_created += 1;
        }
        summary.sessions_written += 1;
    }

    Ok(summary)
}

pub fn run_next_analysis_job(paths: &AppPaths) -> anyhow::Result<bool> {
    crate::config::initialize_workspace(paths)?;
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let Some(job) = crate::db::claim_next_job(&connection)? else {
        return Ok(false);
    };

    if job.kind == "scan_sources" {
        match crate::commands::scan_sources_inner(paths) {
            Ok(result) => {
                crate::db::update_job_progress(
                    &connection,
                    job.id,
                    1,
                    0,
                    &format!("Scan complete: {result}"),
                )?;
                crate::db::complete_job(&connection, job.id, "Session scan completed")?;
            }
            Err(error) => {
                crate::db::fail_job(
                    &connection,
                    job.id,
                    &format!("Session scan failed: {error}"),
                )?;
            }
        }
        return Ok(true);
    }

    if job.kind == "generate_task_prompt" {
        let Some(project_slug) = job.project_slug.as_deref() else {
            crate::db::fail_job(&connection, job.id, "Task prompt job has no project slug")?;
            return Ok(true);
        };
        let Some(task_slug) = job.task_slug.as_deref() else {
            crate::db::fail_job(&connection, job.id, "Task prompt job has no task slug")?;
            return Ok(true);
        };
        match generate_task_prompt(paths, project_slug, task_slug) {
            Ok(_) => {
                crate::db::update_job_progress(&connection, job.id, 1, 0, "Task prompt written")?;
                crate::db::complete_job(&connection, job.id, "Task prompt written")?;
            }
            Err(error) => {
                crate::db::fail_job(&connection, job.id, &format!("Task prompt failed: {error}"))?;
            }
        }
        return Ok(true);
    }

    if job.kind == "save_agent_session" {
        let Some(project_slug) = job.project_slug.as_deref() else {
            crate::db::fail_job(&connection, job.id, "Assistant session save job has no project slug")?;
            return Ok(true);
        };
        let Some(session_id) = job.session_id.as_deref() else {
            crate::db::fail_job(&connection, job.id, "Assistant session save job has no session id")?;
            return Ok(true);
        };
        match crate::commands::run_save_agent_session_job(paths, job.id, session_id, project_slug) {
            Ok(_) => {
                crate::db::update_job_progress(&connection, job.id, 1, 0, "Assistant session saved")?;
                crate::db::complete_job(&connection, job.id, "Assistant session saved")?;
            }
            Err(error) => {
                crate::db::fail_job(
                    &connection,
                    job.id,
                    &format!("Assistant session save failed: {error}"),
                )?;
            }
        }
        return Ok(true);
    }

    if job.kind == "review_project" {
        let Some(project_slug) = job.project_slug.as_deref() else {
            crate::db::fail_job(
                &connection,
                job.id,
                "Project summary job has no project slug",
            )?;
            return Ok(true);
        };
        match review_project(paths, project_slug) {
            Ok(_) => {
                crate::db::update_job_progress(
                    &connection,
                    job.id,
                    1,
                    0,
                    "Project summary written",
                )?;
                crate::db::complete_job(&connection, job.id, "Project summary written")?;
            }
            Err(error) => {
                crate::db::fail_job(
                    &connection,
                    job.id,
                    &format!("Project summary failed: {error}"),
                )?;
            }
        }
        return Ok(true);
    }

    if job.kind == "rebuild_memories" {
        let settings = crate::config::read_llm_settings(paths)?;
        let sessions = crate::db::sessions_needing_memory_rebuild(&connection)?;
        let mut completed = job.completed;
        let mut failed = job.failed;
        for session in sessions {
            if !crate::db::job_is_active(&connection, job.id)? {
                return Ok(true);
            }
            let memory_updated_at = crate::db::session_processed_at(&connection, session.id)?
                .unwrap_or_else(crate::utils::now_rfc3339);
            match clear_session_memory_artifacts(paths, &connection, &session)
                .and_then(|_| rebuild_session_memory(paths, &settings, &session))
                .and_then(|memory| {
                    crate::memory::generate_session_memory_at(
                        paths,
                        &connection,
                        &session,
                        &memory,
                        &memory_updated_at,
                    )
                }) {
                Ok(_) => completed += 1,
                Err(_) => failed += 1,
            }
            crate::db::update_job_progress(
                &connection,
                job.id,
                completed,
                failed,
                &format!("Rebuilt {completed} of {}", job.total),
            )?;
        }
        if !crate::db::job_is_active(&connection, job.id)? {
            return Ok(true);
        }
        let rebuilt = completed;
        if let Err(error) = disambiguate_memory_entities(paths, &settings) {
            failed += 1;
            crate::db::update_job_progress(
                &connection,
                job.id,
                completed,
                failed,
                "Entity disambiguation failed",
            )?;
            crate::db::fail_job(
                &connection,
                job.id,
                &format!("Entity disambiguation failed: {error}"),
            )?;
            return Ok(true);
        }
        completed += 1;
        crate::db::update_job_progress(
            &connection,
            job.id,
            completed,
            failed,
            "Entities disambiguated",
        )?;
        crate::db::complete_job(
            &connection,
            job.id,
            &format!(
                "Rebuilt {rebuilt} memory session{}; entities disambiguated",
                if rebuilt == 1 { "" } else { "s" },
            ),
        )?;
        return Ok(true);
    }

    if job.kind == "search_memories" {
        let result = run_memory_search_job(paths, &connection, job.id);
        match result {
            Ok(count) => {
                crate::db::update_job_progress(
                    &connection,
                    job.id,
                    1,
                    0,
                    &format!("{count} memory found{}", if count == 1 { "" } else { "s" }),
                )?;
                crate::db::complete_job(
                    &connection,
                    job.id,
                    &format!("{count} memory found{}", if count == 1 { "" } else { "s" }),
                )?;
            }
            Err(error) => {
                crate::db::fail_job(
                    &connection,
                    job.id,
                    &format!("Memory search failed: {error}"),
                )?;
            }
        }
        return Ok(true);
    }

    if job.kind == "analyze_project" {
        let Some(project_slug) = job.project_slug.as_deref() else {
            crate::db::fail_job(
                &connection,
                job.id,
                "Project analyze job has no project slug",
            )?;
            return Ok(true);
        };
        let settings = crate::config::read_llm_settings(paths)?;
        let sessions = crate::db::project_sessions_needing_analysis_limited(
            &connection,
            project_slug,
            crate::db::PROJECT_ANALYZE_SESSION_LIMIT,
        )?;
        let (mut completed, failed) = if sessions.is_empty() {
            (job.completed, job.failed)
        } else {
            process_session_job(
                paths,
                job.id,
                job.total,
                job.completed,
                job.failed,
                sessions,
                settings.clone(),
            )
        };
        if !crate::db::job_is_active(&connection, job.id)? {
            return Ok(true);
        }

        let project_slug = project_slug.to_string();
        let review_paths = paths.clone();
        let progress_paths = paths.clone();
        let preference_paths = paths.clone();
        let progress_settings = settings.clone();
        let preference_settings = settings.clone();
        let review_slug = project_slug.clone();
        let progress_slug = project_slug.clone();
        let preference_slug = project_slug.clone();
        let review_handle = std::thread::spawn(move || review_project(&review_paths, &review_slug));
        let progress_handle = std::thread::spawn(move || {
            write_progress(&progress_paths, &progress_settings, &progress_slug)
        });
        let preference_handle = std::thread::spawn(move || {
            write_user_preference(&preference_paths, &preference_settings, &preference_slug)
        });
        let review_result = review_handle.join();
        let progress_result = progress_handle.join();
        let preference_result = preference_handle.join();
        if review_result.is_err() {
            crate::db::fail_job(&connection, job.id, "Project summary worker panicked")?;
            return Ok(true);
        }
        if progress_result.is_err() {
            crate::db::fail_job(&connection, job.id, "Project progress worker panicked")?;
            return Ok(true);
        }
        if preference_result.is_err() {
            crate::db::fail_job(&connection, job.id, "User preference worker panicked")?;
            return Ok(true);
        }
        if !crate::db::job_is_active(&connection, job.id)? {
            return Ok(true);
        }

        let mut failure: Option<String> = None;
        match review_result.expect("review worker join checked") {
            Ok(_) => {
                completed += 1;
                crate::db::update_job_progress(
                    &connection,
                    job.id,
                    completed,
                    failed,
                    "Project summary written",
                )?;
            }
            Err(error) => {
                failure = Some(format!("Project summary failed: {error}"));
            }
        }
        match progress_result.expect("progress worker join checked") {
            Ok(_) => {
                completed += 1;
                crate::db::update_job_progress(
                    &connection,
                    job.id,
                    completed,
                    failed,
                    "Project progress written",
                )?;
            }
            Err(error) if failure.is_none() => {
                failure = Some(format!("Project progress failed: {error}"));
            }
            Err(_) => {}
        }
        match preference_result.expect("preference worker join checked") {
            Ok(_) => {
                completed += 1;
                crate::db::update_job_progress(
                    &connection,
                    job.id,
                    completed,
                    failed,
                    "User preference written",
                )?;
            }
            Err(error) if failure.is_none() => {
                failure = Some(format!("User preference failed: {error}"));
            }
            Err(_) => {}
        }
        if failure.is_none() {
            match write_project_agents(paths, &settings, &project_slug) {
                Ok(_) => {
                    completed += 1;
                    crate::db::update_job_progress(
                        &connection,
                        job.id,
                        completed,
                        failed,
                        "AGENTS.md written",
                    )?;
                }
                Err(error) => {
                    failure = Some(format!("AGENTS.md failed: {error}"));
                }
            }
        }
        if let Some(message) = failure {
            crate::db::fail_job(&connection, job.id, &message)?;
        } else {
            crate::db::complete_job(&connection, job.id, "Project analysis complete")?;
        }
        return Ok(true);
    }

    let sessions = match job.scope.as_str() {
        "single_session" => match job.session_id.as_deref() {
            Some(session_id) => {
                crate::db::unprocessed_session_by_session_id(&connection, session_id)?
            }
            None => Vec::new(),
        },
        "project_unprocessed" => match job.project_slug.as_deref() {
            Some(project_slug) => {
                crate::db::unprocessed_sessions_by_project_slug(&connection, project_slug)?
            }
            None => Vec::new(),
        },
        _ => match job.updated_after.as_deref() {
            Some(updated_after) => {
                crate::db::unprocessed_sessions_updated_after(&connection, updated_after)?
            }
            None => crate::db::unprocessed_sessions(&connection)?,
        },
    };

    if sessions.is_empty() {
        crate::db::complete_job(&connection, job.id, "No sessions to analyze")?;
        return Ok(true);
    }

    let settings = crate::config::resolve_llm_settings(
        &crate::config::read_llm_settings(paths)?,
        crate::config::LlmScenario::Memory,
    );
    let (completed, failed) = process_session_job(
        paths,
        job.id,
        job.total,
        job.completed,
        job.failed,
        sessions,
        settings,
    );
    if crate::db::job_is_active(&connection, job.id)? {
        crate::db::complete_job(
            &connection,
            job.id,
            &format!(
                "Analyzed {completed} session{}",
                if completed == 1 { "" } else { "s" }
            ),
        )?;
        crate::db::update_job_progress(
            &connection,
            job.id,
            completed,
            failed,
            &format!("Analyzed {completed} of {}", job.total),
        )?;
    }
    Ok(true)
}

pub(crate) fn session_worker_count(total: usize) -> usize {
    if total == 0 {
        0
    } else {
        total.min(5)
    }
}

fn process_session_job(
    paths: &AppPaths,
    job_id: i64,
    total: usize,
    completed: usize,
    failed: usize,
    sessions: Vec<crate::models::StoredSession>,
    settings: crate::models::LlmSettings,
) -> (usize, usize) {
    let queue = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from(
        sessions,
    )));
    let progress = std::sync::Arc::new(std::sync::Mutex::new((completed, failed)));
    let store_lock = std::sync::Arc::new(std::sync::Mutex::new(()));
    let worker_count = session_worker_count(queue.lock().map(|queue| queue.len()).unwrap_or(0));

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = std::sync::Arc::clone(&queue);
            let progress = std::sync::Arc::clone(&progress);
            let store_lock = std::sync::Arc::clone(&store_lock);
            let paths = paths.clone();
            let settings = settings.clone();
            scope.spawn(move || {
                let Ok(connection) = crate::db::open(&paths) else {
                    return;
                };
                if crate::db::migrate(&connection).is_err() {
                    return;
                }
                loop {
                    let Ok(active) = crate::db::job_is_active(&connection, job_id) else {
                        break;
                    };
                    if !active {
                        break;
                    }
                    let session = {
                        let Ok(mut queue) = queue.lock() else {
                            break;
                        };
                        queue.pop_front()
                    };
                    let Some(session) = session else {
                        break;
                    };
                    let analysis = analyze_session(&paths, &settings, &session);
                    let result = {
                        let Ok(_guard) = store_lock.lock() else {
                            break;
                        };
                        match (
                            crate::db::job_is_active(&connection, job_id),
                            crate::db::session_is_unprocessed(&connection, session.id),
                        ) {
                            (Ok(true), Ok(true)) => match analysis {
                                Ok(analysis) => {
                                    store_session_analysis(&paths, &connection, &session, analysis)
                                }
                                Err(error) => {
                                    let message = error.to_string();
                                    crate::db::mark_session_failed(
                                        &connection,
                                        session.id,
                                        &message,
                                    )
                                    .map(|_| Err(error))
                                    .unwrap_or_else(Err)
                                }
                            },
                            _ => break,
                        }
                    };
                    let (completed, failed, message) = {
                        let Ok(mut progress) = progress.lock() else {
                            break;
                        };
                        match result {
                            Ok(_) => {
                                progress.0 += 1;
                                (
                                    progress.0,
                                    progress.1,
                                    format!("Analyzed {} of {total}", progress.0),
                                )
                            }
                            Err(error) => {
                                progress.1 += 1;
                                (
                                    progress.0,
                                    progress.1,
                                    format!("Failed {}: {error}", session.session_id),
                                )
                            }
                        }
                    };
                    let _ = crate::db::update_job_progress(
                        &connection,
                        job_id,
                        completed,
                        failed,
                        &message,
                    );
                }
            });
        }
    });

    progress
        .lock()
        .map(|progress| *progress)
        .unwrap_or((completed, failed))
}

