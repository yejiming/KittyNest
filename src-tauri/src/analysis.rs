use crate::models::AppPaths;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub projects_updated: usize,
    pub tasks_created: usize,
    pub sessions_written: usize,
}

pub fn import_historical_sessions(paths: &AppPaths) -> anyhow::Result<ImportSummary> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let sessions = crate::db::unprocessed_sessions(&connection)?;
    let settings = crate::config::read_llm_settings(paths)?;
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
                .and_then(|_| rebuild_session_memory(&settings, &session))
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
        let progress_settings = settings.clone();
        let review_slug = project_slug.clone();
        let progress_slug = project_slug.clone();
        let review_handle = std::thread::spawn(move || review_project(&review_paths, &review_slug));
        let progress_handle = std::thread::spawn(move || {
            write_progress(&progress_paths, &progress_settings, &progress_slug)
        });
        let review_result = review_handle.join();
        let progress_result = progress_handle.join();
        if review_result.is_err() {
            crate::db::fail_job(&connection, job.id, "Project summary worker panicked")?;
            return Ok(true);
        }
        if progress_result.is_err() {
            crate::db::fail_job(&connection, job.id, "Project progress worker panicked")?;
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
        if failure.is_none() {
            match write_user_preference(paths, &settings, &project_slug) {
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
                Err(error) => {
                    failure = Some(format!("User preference failed: {error}"));
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

    let settings = crate::config::read_llm_settings(paths)?;
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
                    let analysis = analyze_session(&settings, &session);
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

pub fn review_project(paths: &AppPaths, project_slug: &str) -> anyhow::Result<String> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let (project_id, project) = crate::db::get_project_by_slug(&connection, project_slug)?
        .ok_or_else(|| anyhow::anyhow!("project not found: {project_slug}"))?;
    let project_dir = paths.projects_dir.join(&project.slug);
    std::fs::create_dir_all(&project_dir)?;
    let info_path = project_dir.join("summary.md");
    let code_context = code_context(&project.workdir)?;
    let settings = crate::config::read_llm_settings(paths)?;
    let body = strip_llm_think_blocks(&remote_project_review(&settings, &project, &code_context)?);
    let markdown = crate::markdown::render_frontmatter_markdown(
        &[
            ("project_name", project.slug.clone()),
            ("workdir", project.workdir.clone()),
            ("reviewed_at", crate::utils::now_rfc3339()),
        ],
        &body,
    );
    std::fs::write(&info_path, markdown)?;
    crate::db::update_project_review(&connection, project_id, &info_path.to_string_lossy())?;
    Ok(info_path.to_string_lossy().to_string())
}

pub fn create_manual_task(
    paths: &AppPaths,
    project_slug: &str,
    user_prompt: &str,
) -> anyhow::Result<crate::models::CreateTaskResult> {
    let prompt = user_prompt.trim();
    if prompt.is_empty() {
        anyhow::bail!("task prompt cannot be empty");
    }
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let (project_id, project) = crate::db::get_project_by_slug(&connection, project_slug)?
        .ok_or_else(|| anyhow::anyhow!("project not found: {project_slug}"))?;
    if project.review_status != "reviewed" {
        anyhow::bail!("manual tasks require a reviewed project");
    }

    let title = task_title_from_prompt(prompt);
    let base_slug = crate::utils::slugify_lower(&title);
    let task_slug = crate::db::unique_task_slug(&connection, project_id, &base_slug)?;
    let task_dir = paths
        .projects_dir
        .join(&project.slug)
        .join("tasks")
        .join(&task_slug);
    std::fs::create_dir_all(&task_dir)?;
    let user_prompt_path = task_dir.join("user_prompt.md");
    let llm_prompt_path = task_dir.join("llm_prompt.md");
    std::fs::write(&user_prompt_path, format!("{prompt}\n"))?;
    crate::db::upsert_task(
        &connection,
        project_id,
        &task_slug,
        &title,
        prompt,
        "discussing",
        &llm_prompt_path.to_string_lossy(),
    )?;
    let job = crate::db::enqueue_generate_task_prompt(&connection, &project.slug, &task_slug)?;

    Ok(crate::models::CreateTaskResult {
        project_slug: project.slug,
        task_slug,
        job_id: job.job_id,
        total: job.total,
        user_prompt_path: user_prompt_path.to_string_lossy().to_string(),
        llm_prompt_path: llm_prompt_path.to_string_lossy().to_string(),
    })
}

pub fn rebuild_memories(paths: &AppPaths) -> anyhow::Result<usize> {
    crate::config::initialize_workspace(paths)?;
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let settings = crate::config::read_llm_settings(paths)?;
    let sessions = crate::db::sessions_needing_memory_rebuild(&connection)?;
    let mut rebuilt = 0usize;
    for session in sessions {
        clear_session_memory_artifacts(paths, &connection, &session)?;
        let memory = rebuild_session_memory(&settings, &session)?;
        let memory_updated_at = crate::db::session_processed_at(&connection, session.id)?
            .unwrap_or_else(crate::utils::now_rfc3339);
        crate::memory::generate_session_memory_at(
            paths,
            &connection,
            &session,
            &memory,
            &memory_updated_at,
        )?;
        rebuilt += 1;
    }
    disambiguate_memory_entities(paths, &settings)?;
    Ok(rebuilt)
}

fn clear_session_memory_artifacts(
    paths: &AppPaths,
    connection: &rusqlite::Connection,
    session: &crate::models::StoredSession,
) -> anyhow::Result<()> {
    crate::memory::delete_session_memory_file(paths, session)?;
    crate::db::delete_session_memories(connection, session)?;
    crate::graph::delete_session_entities(paths, &session.session_id)
}

fn generate_task_prompt(
    paths: &AppPaths,
    project_slug: &str,
    task_slug: &str,
) -> anyhow::Result<()> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let (_, project) = crate::db::get_project_by_slug(&connection, project_slug)?
        .ok_or_else(|| anyhow::anyhow!("project not found: {project_slug}"))?;
    let task_dir = paths
        .projects_dir
        .join(&project.slug)
        .join("tasks")
        .join(task_slug);
    let user_prompt_path = task_dir.join("user_prompt.md");
    let llm_prompt_path = task_dir.join("llm_prompt.md");
    let user_prompt = std::fs::read_to_string(&user_prompt_path)?;
    let project_summary = read_optional_markdown(project.info_path.as_deref())?;
    let project_progress = read_optional_markdown(project.progress_path.as_deref())?;
    let settings = crate::config::resolve_llm_settings(
        &crate::config::read_llm_settings(paths)?,
        crate::config::LlmScenario::Task,
    );
    let response = crate::llm::request_markdown(
        &settings,
        "Rewrite the user's task prompt so it fits the real project. Return Markdown only with a concrete actionable prompt.",
        &format!(
            "Project: {}\nWorkdir: {}\n\nProject Summary:\n{}\n\nProject Progress:\n{}\n\nUser Prompt:\n{}",
            project.display_title,
            project.workdir,
            if project_summary.trim().is_empty() {
                "No project summary is available."
            } else {
                project_summary.trim()
            },
            if project_progress.trim().is_empty() {
                "No project progress is available."
            } else {
                project_progress.trim()
            },
            user_prompt.trim()
        ),
    )?;
    let body = strip_llm_think_blocks(&response.content);
    std::fs::write(&llm_prompt_path, format!("{}\n", body.trim()))?;
    Ok(())
}

fn read_optional_markdown(path: Option<&str>) -> anyhow::Result<String> {
    let Some(path) = path else {
        return Ok(String::new());
    };
    if path.trim().is_empty() {
        return Ok(String::new());
    }
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

fn task_title_from_prompt(prompt: &str) -> String {
    prompt
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.chars().take(80).collect::<String>())
            }
        })
        .unwrap_or_else(|| "Task".into())
}

fn analyze_and_store_session(
    paths: &AppPaths,
    connection: &rusqlite::Connection,
    settings: &crate::models::LlmSettings,
    session: &crate::models::StoredSession,
) -> anyhow::Result<(String, bool)> {
    let analyzed = analyze_session(settings, session)?;
    store_session_analysis(paths, connection, session, analyzed)
}

fn analyze_session(
    settings: &crate::models::LlmSettings,
    session: &crate::models::StoredSession,
) -> anyhow::Result<SessionAnalysis> {
    let settings =
        crate::config::resolve_llm_settings(settings, crate::config::LlmScenario::Session);
    remote_session_analysis(&settings, session)
}

fn store_session_analysis(
    paths: &AppPaths,
    connection: &rusqlite::Connection,
    session: &crate::models::StoredSession,
    analyzed: SessionAnalysis,
) -> anyhow::Result<(String, bool)> {
    let session_title = analyzed.session_title;
    let session_summary = analyzed.session_summary;
    let session_memory = analyzed.memory;
    let session_slug = crate::utils::slugify_lower(&session.session_id);
    let project_dir = paths.projects_dir.join(&session.project_slug);
    let session_dir = project_dir.join("sessions").join(&session_slug);
    std::fs::create_dir_all(&session_dir)?;
    let session_path = session_dir.join("summary.md");
    let session_markdown = crate::markdown::render_frontmatter_markdown(
        &[
            ("source", session.source.clone()),
            ("session_id", session.session_id.clone()),
            ("workdir", session.workdir.clone()),
            ("updated_at", session.updated_at.clone()),
        ],
        &format!("# {session_title}\n\n{session_summary}\n"),
    );
    std::fs::write(&session_path, session_markdown)?;
    let analyzed_at = crate::utils::now_rfc3339();
    crate::memory::generate_session_memory_at(
        paths,
        connection,
        session,
        &session_memory,
        &analyzed_at,
    )?;
    crate::db::mark_session_processed_with_optional_task_at(
        connection,
        session.id,
        session.task_id,
        &session_title,
        &session_summary,
        &session_path.to_string_lossy(),
        &analyzed_at,
    )?;

    Ok((session.project_slug.clone(), false))
}

fn write_progress(
    paths: &AppPaths,
    settings: &crate::models::LlmSettings,
    project_slug: &str,
) -> anyhow::Result<()> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let project_dir = paths.projects_dir.join(project_slug);
    std::fs::create_dir_all(&project_dir)?;
    let progress_path = project_dir.join("progress.md");
    let summaries = crate::db::analyzed_session_summaries_by_project_slug(
        &connection,
        project_slug,
        crate::db::PROJECT_ANALYZE_SESSION_LIMIT,
    )?;
    let body = strip_llm_think_blocks(&remote_project_progress(
        settings,
        project_slug,
        &summaries,
    )?);
    let markdown = crate::markdown::render_frontmatter_markdown(
        &[
            ("project", project_slug.into()),
            ("updated_at", crate::utils::now_rfc3339()),
        ],
        &body,
    );
    std::fs::write(&progress_path, markdown)?;
    crate::db::update_project_progress(&connection, project_slug, &progress_path.to_string_lossy())
}

fn write_user_preference(
    paths: &AppPaths,
    settings: &crate::models::LlmSettings,
    project_slug: &str,
) -> anyhow::Result<()> {
    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    let project_dir = paths.projects_dir.join(project_slug);
    std::fs::create_dir_all(&project_dir)?;
    let user_preference_path = project_dir.join("user_preference.md");
    let sessions = crate::db::project_sessions_by_project_slug(
        &connection,
        project_slug,
        crate::db::PROJECT_ANALYZE_SESSION_LIMIT,
    )?;
    let body = strip_llm_think_blocks(&remote_project_user_preference(
        settings,
        project_slug,
        &sessions,
    )?);
    let markdown = crate::markdown::render_frontmatter_markdown(
        &[
            ("project", project_slug.into()),
            ("updated_at", crate::utils::now_rfc3339()),
        ],
        &body,
    );
    std::fs::write(&user_preference_path, markdown)?;
    crate::db::update_project_user_preference(
        &connection,
        project_slug,
        &user_preference_path.to_string_lossy(),
    )
}

fn remote_session_analysis(
    settings: &crate::models::LlmSettings,
    session: &crate::models::StoredSession,
) -> anyhow::Result<SessionAnalysis> {
    let transcript = session_transcript(session);
    let system_prompt = crate::memory::session_memory_system_prompt();
    let base_prompt = crate::memory::session_memory_user_prompt(session, &transcript);
    let mut previous_error: Option<String> = None;

    for attempt in 1..=3 {
        let user_prompt = match previous_error.as_deref() {
            Some(error) => format!(
                "{base_prompt}\n\nPrevious LLM response error: {error}\nReturn corrected JSON only."
            ),
            None => base_prompt.clone(),
        };
        match crate::llm::request_json(settings, system_prompt, &user_prompt)
            .and_then(|response| session_analysis_from_json(&response.content))
        {
            Ok(analysis) => return Ok(analysis),
            Err(error) if attempt < 3 => previous_error = Some(error.to_string()),
            Err(error) => return Err(error),
        }
    }

    anyhow::bail!("LLM session analysis failed after 3 attempts")
}

fn rebuild_session_memory(
    settings: &crate::models::LlmSettings,
    session: &crate::models::StoredSession,
) -> anyhow::Result<crate::memory::SessionMemoryDraft> {
    let settings =
        crate::config::resolve_llm_settings(settings, crate::config::LlmScenario::Session);
    let transcript = session_transcript(session);
    let system_prompt = crate::memory::session_memory_rebuild_system_prompt();
    let base_prompt = crate::memory::session_memory_user_prompt(session, &transcript);
    let mut previous_error: Option<String> = None;

    for attempt in 1..=3 {
        let user_prompt = match previous_error.as_deref() {
            Some(error) => format!(
                "{base_prompt}\n\nPrevious LLM response error: {error}\nReturn corrected JSON only."
            ),
            None => base_prompt.clone(),
        };
        match crate::llm::request_json(&settings, system_prompt, &user_prompt)
            .and_then(|response| crate::memory::session_memory_from_json(&response.content))
        {
            Ok(memory) => return Ok(memory),
            Err(error) if attempt < 3 => previous_error = Some(error.to_string()),
            Err(error) => return Err(error),
        }
    }

    anyhow::bail!("LLM memory rebuild failed after 3 attempts")
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
    let result = remote_entity_alias_groups(&settings, &entities)
        .and_then(|groups| crate::graph::write_entity_aliases(paths, &groups));
    if let Err(error) = &result {
        append_error_log(
            paths,
            "Entity disambiguation failed",
            &format_entity_disambiguation_error(error, &entities),
        );
    }
    result
}

fn remote_entity_alias_groups(
    settings: &crate::models::LlmSettings,
    entities: &[crate::graph::EntityForDisambiguation],
) -> anyhow::Result<Vec<crate::graph::EntityAliasGroup>> {
    let entity_names = entities
        .iter()
        .map(|entity| entity.name.as_str())
        .collect::<Vec<_>>();
    let entity_names_json = serde_json::to_string_pretty(&entity_names)?;
    let response = crate::llm::request_json(
        settings,
        "Return only JSON with groups. Merge synonymous entities. groups must be an array of {canonical_id, canonical_name, aliases}. canonical_id must be unique. canonical_name must be unique and human-facing. aliases must include every synonym string from the supplied entities that belongs to the group. Example response: {\"groups\":[{\"canonical_id\":\"sqlite\",\"canonical_name\":\"SQLite\",\"aliases\":[\"sqlite\",\"SQLite DB\",\"SQLite database\"]},{\"canonical_id\":\"react\",\"canonical_name\":\"React\",\"aliases\":[\"react\",\"React.js\"]}]}",
        &format!("Existing entity names:\n{entity_names_json}"),
    )?;
    entity_alias_groups_from_json(&response.content, entities)
        .map_err(|error| anyhow::anyhow!("{error}; raw_entity_alias_response={}", response.content))
}

fn format_entity_disambiguation_error(
    error: &anyhow::Error,
    entities: &[crate::graph::EntityForDisambiguation],
) -> String {
    let entities_json =
        serde_json::to_string_pretty(entities).unwrap_or_else(|_| "<entities json failed>".into());
    format!(
        "stage: entity_disambiguation\nerror: {error:#}\nentity_count: {}\nentity_disambiguation_input_json:\n{entities_json}",
        entities.len()
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

fn run_memory_search_job(
    paths: &AppPaths,
    connection: &rusqlite::Connection,
    job_id: i64,
) -> anyhow::Result<usize> {
    let search = crate::db::memory_search_for_job(connection, job_id)?
        .ok_or_else(|| anyhow::anyhow!("memory search row not found for job {job_id}"))?;
    let settings = crate::config::read_llm_settings(paths)?;
    let entities = memory_search_entities(&settings, &search.query)?;
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
    settings: &crate::models::LlmSettings,
    query: &str,
) -> anyhow::Result<Vec<String>> {
    let mut entities = extract_memory_search_entities(settings, query)?;
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

fn extract_memory_search_entities(
    settings: &crate::models::LlmSettings,
    query: &str,
) -> anyhow::Result<Vec<String>> {
    let response = crate::llm::request_json(
        settings,
        "Return only JSON with entities. entities must be an array of entity strings extracted from the user's memory search query.",
        query,
    )?;
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
        .collect())
}

fn session_transcript(session: &crate::models::StoredSession) -> String {
    session
        .messages
        .iter()
        .filter(|message| matches!(message.role.as_str(), "user" | "assistant"))
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn session_analysis_from_json(value: &serde_json::Value) -> anyhow::Result<SessionAnalysis> {
    Ok(SessionAnalysis {
        session_title: required_json_string(value, "session_title")?,
        session_summary: required_json_string(value, "summary")?,
        memory: crate::memory::session_memory_from_json(value)?,
    })
}

fn required_json_string(value: &serde_json::Value, key: &str) -> anyhow::Result<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("LLM JSON missing required string field `{key}`"))
}

fn remote_project_review(
    settings: &crate::models::LlmSettings,
    project: &crate::models::ProjectRecord,
    code_context: &CodeContext,
) -> anyhow::Result<String> {
    let settings =
        crate::config::resolve_llm_settings(settings, crate::config::LlmScenario::Project);
    let excerpts = code_context
        .excerpts
        .iter()
        .map(|file| format!("### {}\n```text\n{}\n```", file.path, file.content))
        .collect::<Vec<_>>()
        .join("\n\n");
    let response = crate::llm::request_markdown(
        &settings,
        "Review the project from the supplied file index and file excerpts. Return Markdown only. Use exactly these five second-level sections: ## Summary, ## Tech Stack, ## Architecture, ## Code Quality, ## Risks. Do not return JSON.",
        &format!(
            "Project: {}\nWorkdir: {}\n\nFile index:\n{}\n\nFile excerpts:\n{}",
            project.display_title,
            project.workdir,
            code_context.index.join("\n"),
            if excerpts.is_empty() {
                "No readable file excerpts were found.".to_string()
            } else {
                excerpts
            }
        ),
    )?;
    Ok(response.content.trim().to_string())
}

fn remote_project_progress(
    settings: &crate::models::LlmSettings,
    project_slug: &str,
    summaries: &[crate::models::ProjectSessionSummary],
) -> anyhow::Result<String> {
    let settings =
        crate::config::resolve_llm_settings(settings, crate::config::LlmScenario::Project);
    let timeline = if summaries.is_empty() {
        "No analyzed session summaries yet.".to_string()
    } else {
        summaries
            .iter()
            .map(|session| {
                format!(
                    "## {} - {}\nSession: {}\nTask: {}\nUpdated: {}\n\n{}",
                    session.created_at,
                    session.title,
                    session.session_id,
                    session.task_slug.as_deref().unwrap_or("unassigned"),
                    session.updated_at,
                    session.summary
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let narrative = crate::llm::request_markdown(
        &settings,
        "Aggregate all analyzed session summaries into narrative Project Progress. Sessions are already ordered chronologically. Use the majority language of the project's sessions. Return Markdown only.",
        &format!("Project: {project_slug}\n\nChronological session summaries:\n\n{timeline}"),
    )?;
    Ok(strip_llm_think_blocks(&narrative.content))
}

fn remote_project_user_preference(
    settings: &crate::models::LlmSettings,
    project_slug: &str,
    sessions: &[crate::models::StoredSession],
) -> anyhow::Result<String> {
    let settings =
        crate::config::resolve_llm_settings(settings, crate::config::LlmScenario::Project);
    let transcript = if sessions.is_empty() {
        "No sessions yet.".to_string()
    } else {
        sessions
            .iter()
            .map(|session| {
                format!(
                    "## {} - {}\nSession: {}\n\n{}",
                    session.created_at,
                    session.session_id,
                    session.session_id,
                    session_transcript(session)
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let response = crate::llm::request_markdown(
        &settings,
        "Summarize durable user preferences from all supplied user and assistant messages. Focus on preferences, working style, constraints, recurring goals, and communication style. Use the majority language of the project's sessions. Return Markdown only.",
        &format!("Project: {project_slug}\n\nSession transcripts:\n\n{transcript}"),
    )?;
    Ok(strip_llm_think_blocks(&response.content))
}

fn strip_llm_think_blocks(markdown: &str) -> String {
    let mut output = String::with_capacity(markdown.len());
    let mut rest = markdown;

    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(start) = lower.find("<think>") else {
            output.push_str(rest);
            break;
        };

        output.push_str(&rest[..start]);
        let after_open_index = start + "<think>".len();
        let after_open = &rest[after_open_index..];
        let lower_after_open = &lower[after_open_index..];

        let Some(end) = lower_after_open.find("</think>") else {
            break;
        };
        rest = &after_open[end + "</think>".len()..];
    }

    output.trim().to_string()
}

struct SessionAnalysis {
    session_title: String,
    session_summary: String,
    memory: crate::memory::SessionMemoryDraft,
}

struct CodeContext {
    index: Vec<String>,
    excerpts: Vec<CodeExcerpt>,
}

struct CodeExcerpt {
    path: String,
    content: String,
}

fn code_context(workdir: &str) -> anyhow::Result<CodeContext> {
    let root = std::path::Path::new(workdir);
    if !root.exists() {
        return Ok(CodeContext {
            index: Vec::new(),
            excerpts: Vec::new(),
        });
    }
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .max_depth(3)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !matches!(name.as_ref(), ".git" | "node_modules" | "target" | "dist")
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .take(80)
    {
        if let Ok(relative) = entry.path().strip_prefix(root) {
            files.push((relative.to_path_buf(), entry.path().to_path_buf()));
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let index = files
        .iter()
        .map(|(relative, _)| format!("- `{}`", relative.to_string_lossy()))
        .collect::<Vec<_>>();
    let mut excerpts = Vec::new();
    let mut total_chars = 0usize;
    for (relative, full_path) in files.into_iter().take(30) {
        if !is_source_excerpt_candidate(&relative) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&full_path) else {
            continue;
        };
        if bytes.iter().any(|byte| *byte == 0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let excerpt = text.chars().take(6000).collect::<String>();
        if excerpt.trim().is_empty() {
            continue;
        }
        total_chars += excerpt.chars().count();
        if total_chars > 120_000 {
            break;
        }
        excerpts.push(CodeExcerpt {
            path: relative.to_string_lossy().to_string(),
            content: excerpt,
        });
    }

    Ok(CodeContext { index, excerpts })
}

fn is_source_excerpt_candidate(path: &std::path::Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if matches!(
        file_name,
        "Cargo.toml" | "package.json" | "tauri.conf.json" | "vite.config.ts"
    ) {
        return true;
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    matches!(
        extension,
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "json"
            | "toml"
            | "md"
            | "css"
            | "html"
            | "yml"
            | "yaml"
            | "sql"
            | "py"
            | "go"
            | "java"
            | "swift"
            | "kt"
            | "rb"
            | "php"
            | "c"
            | "h"
            | "hpp"
            | "cpp"
            | "mjs"
            | "cjs"
            | "vue"
            | "svelte"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        analyze_session, create_manual_task, import_historical_sessions, rebuild_memories,
        review_project, run_next_analysis_job, session_worker_count, store_session_analysis,
        write_progress, SessionAnalysis,
    };
    use crate::{
        db::{migrate, open, upsert_raw_sessions},
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
        let settings = empty_settings();
        let session = stored_test_session("retry-json");

        let analysis = analyze_session(&settings, &session).unwrap();
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
        let settings = empty_settings();
        let session = stored_test_session("session-only-json");

        let analysis = analyze_session(&settings, &session).unwrap();
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
        seed_rebuildable_session(&paths, "sqlite-db-session", "MemoryProject");
        crate::llm::test_support::set_json_responses(vec![
            serde_json::json!({
                "memories": ["SQLite stores local memory."],
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

        assert_eq!(requests.len(), 3);
        assert!(requests[2].system_prompt.contains("canonical_id"));
        assert!(requests[2].system_prompt.contains("Example response"));
        assert!(requests[2].system_prompt.contains("SQLite database"));
        assert!(requests[2]
            .user_prompt
            .starts_with("Existing entity names:\n["));
        assert!(requests[2].user_prompt.contains("\"sqlite\""));
        assert!(requests[2].user_prompt.contains("\"sqlite db\""));
        assert!(!requests[2].user_prompt.contains("sourceSession"));
        assert!(!requests[2].user_prompt.contains("entityType"));
        assert!(!requests[2].user_prompt.contains("sourceProject"));
        assert!(!requests[2].user_prompt.contains("\"id\""));
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity, "SQLite");
        assert_eq!(entities[0].session_count, 2);
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
        assert!(log.contains("entity_disambiguation_input_json:"));
        assert!(log.contains("LLM JSON missing required array field `groups`"));
        assert!(log.contains("\"unexpected\""));
        assert!(log.contains("\"name\": \"sqlite\""));
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
        crate::llm::test_support::set_markdown_responses(vec![
            "## summary\n\nProject analyzed.\n\n## tech_stack\n\nRust.\n\n## architecture\n\nTauri.\n\n## code_quality\n\nFocused.\n\n## risks\n\nNone.",
            "# Progress\n\nCurrent project progress.",
            "# User Preference\n\nPrefers concise implementation notes.",
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

        assert_eq!(enqueued.total, 23);
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
        assert!(
            std::fs::read_to_string(reviewed.user_preference_path.unwrap())
                .unwrap()
                .contains("Prefers concise implementation notes.")
        );
        let preference_request = markdown_requests
            .iter()
            .find(|request| request.system_prompt.contains("durable user preferences"))
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

        let analysis = analyze_session(&settings, &session).unwrap();
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
