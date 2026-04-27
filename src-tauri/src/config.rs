use crate::models::{AppPaths, LlmSettings};

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

pub fn read_llm_settings(paths: &AppPaths) -> anyhow::Result<LlmSettings> {
    if !paths.config_path.exists() {
        return Ok(default_llm_settings());
    }

    let text = std::fs::read_to_string(&paths.config_path)?;
    let value: toml::Value = toml::from_str(&text)?;
    let llm = value.get("llm").and_then(toml::Value::as_table);
    let field = |key: &str| {
        llm.and_then(|table| table.get(key))
            .and_then(toml::Value::as_str)
            .unwrap_or("")
            .to_string()
    };

    let mut settings = default_llm_settings();
    let provider = field("provider");
    if !provider.is_empty() {
        settings.provider = provider;
    }
    let base_url = field("base_url");
    if !base_url.is_empty() {
        settings.base_url = base_url;
    }
    let interface = field("interface");
    if !interface.is_empty() {
        settings.interface = interface;
    }
    settings.model = field("model");
    settings.api_key = field("api_key");
    Ok(settings)
}

pub fn write_llm_settings(paths: &AppPaths, settings: &LlmSettings) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.data_dir)?;
    let config = serde_json::json!({
        "llm": {
            "provider": settings.provider,
            "base_url": settings.base_url,
            "interface": settings.interface,
            "model": settings.model,
            "api_key": settings.api_key
        }
    });
    std::fs::write(&paths.config_path, toml::to_string_pretty(&config)?)?;
    Ok(())
}

pub fn default_llm_settings() -> LlmSettings {
    LlmSettings {
        provider: "OpenRouter".into(),
        base_url: "https://openrouter.ai/api/v1".into(),
        interface: "openai".into(),
        model: String::new(),
        api_key: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{initialize_workspace, read_llm_settings};
    use crate::models::AppPaths;

    #[test]
    fn initialize_workspace_creates_config_database_and_markdown_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));

        initialize_workspace(&paths).unwrap();

        assert!(paths.config_path.exists());
        assert!(paths.db_path.exists());
        assert!(paths.projects_dir.is_dir());
        assert!(paths.memories_dir.is_dir());

        let settings = read_llm_settings(&paths).unwrap();
        assert_eq!(settings.provider, "OpenRouter");
        assert_eq!(settings.interface, "openai");
        assert_eq!(settings.base_url, "https://openrouter.ai/api/v1");
    }
}
