pub fn default_paths() -> AppPaths {
    let home = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    AppPaths::from_data_dir(home.join(".kittynest"))
}

pub fn initialize_workspace(paths: &AppPaths) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.projects_dir)?;
    std::fs::create_dir_all(&paths.memories_dir)?;

    if !paths.config_path.exists() {
        write_llm_settings(paths, &default_llm_settings())?;
    }

    let connection = crate::db::open(paths)?;
    crate::db::migrate(&connection)?;
    Ok(())
}
