pub mod analysis;
pub mod assistant;
pub mod assistant_context;
pub mod assistant_llm;
pub mod assistant_tools;
pub mod commands;
pub mod config;
pub mod db;
pub mod errors;
pub mod graph;
pub mod llm;
pub mod markdown;
pub mod memory;
pub mod models;
pub mod presets;
pub mod scanner;
pub mod services;
pub mod utils;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(services::AppServices::default())
        .setup(|app| {
            let services = app.state::<services::AppServices>().inner().clone();
            std::thread::spawn(move || {
                if let Ok(connection) = crate::db::open(&services.paths) {
                    let _ = crate::db::migrate(&connection);
                    let _ = crate::db::mark_stale_running_jobs_queued(&connection);
                }
                loop {
                    match crate::analysis::run_next_analysis_job(&services.paths) {
                        Ok(true) => {}
                        Ok(false) => std::thread::sleep(std::time::Duration::from_secs(2)),
                        Err(_) => std::thread::sleep(std::time::Duration::from_secs(5)),
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::get_cached_app_state,
            commands::scan_sources,
            commands::review_project,
            commands::import_historical_sessions,
            commands::enqueue_analyze_sessions,
            commands::enqueue_scan_sources,
            commands::enqueue_analyze_project_sessions,
            commands::enqueue_analyze_project,
            commands::enqueue_analyze_session,
            commands::enqueue_review_project,
            commands::enqueue_rebuild_memories,
            commands::enqueue_search_memories,
            commands::get_memory_search,
            commands::get_session_memory,
            commands::list_memory_entities,
            commands::list_entity_sessions,
            commands::get_active_jobs,
            commands::stop_job,
            commands::start_agent_run,
            commands::stop_agent_run,
            commands::resolve_agent_permission,
            commands::resolve_agent_ask_user,
            commands::read_markdown_file,
            commands::save_llm_settings,
            commands::update_task_status,
            commands::create_task,
            commands::delete_task,
            commands::reset_sessions,
            commands::reset_projects,
            commands::reset_tasks,
            commands::reset_memories,
            commands::rebuild_memories
        ])
        .run(tauri::generate_context!())
        .expect("error while running KittyNest");
}
