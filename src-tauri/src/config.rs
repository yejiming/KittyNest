use crate::models::{AppPaths, LlmModelSettings, LlmScenarioModels, LlmSettings};

const DEFAULT_MAX_CONTEXT: usize = 128_000;
const DEFAULT_MAX_TOKENS: usize = 4_096;
const DEFAULT_TEMPERATURE: f64 = 0.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LlmScenario {
    Default,
    Project,
    Session,
    Memory,
    Task,
}

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

    let integer_field = |key: &str| {
        llm.and_then(|table| table.get(key))
            .and_then(toml::Value::as_integer)
            .and_then(|value| usize::try_from(value).ok())
    };
    let float_field = |key: &str| {
        llm.and_then(|table| table.get(key)).and_then(|value| {
            value
                .as_float()
                .or_else(|| value.as_integer().map(|value| value as f64))
        })
    };

    let mut settings = default_llm_settings();
    let id = field("id");
    if !id.is_empty() {
        settings.id = id;
    }
    let remark = field("remark");
    if !remark.is_empty() {
        settings.remark = remark;
    }
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
    settings.max_context = integer_field("max_context").unwrap_or(DEFAULT_MAX_CONTEXT);
    settings.max_tokens = integer_field("max_tokens").unwrap_or(DEFAULT_MAX_TOKENS);
    settings.temperature = float_field("temperature").unwrap_or(DEFAULT_TEMPERATURE);

    settings.models = llm
        .and_then(|table| table.get("models"))
        .and_then(toml::Value::as_array)
        .map(|items| items.iter().filter_map(model_from_toml).collect())
        .unwrap_or_default();

    if let Some(scenario) = llm
        .and_then(|table| table.get("scenario_models"))
        .and_then(toml::Value::as_table)
    {
        settings.scenario_models.default_model = scenario_string(scenario, "default_model");
        settings.scenario_models.project_model = scenario_string(scenario, "project_model");
        settings.scenario_models.session_model = scenario_string(scenario, "session_model");
        settings.scenario_models.memory_model = scenario_string(scenario, "memory_model");
        settings.scenario_models.task_model = scenario_string(scenario, "task_model");
    }

    if settings.models.is_empty()
        && (!settings.model.trim().is_empty() || !settings.api_key.trim().is_empty())
    {
        settings.models.push(model_from_settings(&settings));
    }
    if settings.scenario_models.default_model.trim().is_empty() {
        settings.scenario_models.default_model =
            settings.models.first().map(|model| model.id.clone()).unwrap_or_default();
    }
    if let Some(model) = default_model(&settings).cloned() {
        apply_model(&mut settings, &model);
    }
    Ok(settings)
}

pub fn write_llm_settings(paths: &AppPaths, settings: &LlmSettings) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.data_dir)?;
    let models = settings
        .models
        .iter()
        .map(|model| {
            serde_json::json!({
                "id": model.id.clone(),
                "remark": model.remark.clone(),
                "provider": model.provider.clone(),
                "base_url": model.base_url.clone(),
                "interface": model.interface.clone(),
                "model": model.model.clone(),
                "api_key": model.api_key.clone()
            })
        })
        .collect::<Vec<_>>();
    let config = serde_json::json!({
        "llm": {
            "id": settings.id.clone(),
            "remark": settings.remark.clone(),
            "provider": settings.provider.clone(),
            "base_url": settings.base_url.clone(),
            "interface": settings.interface.clone(),
            "model": settings.model.clone(),
            "api_key": settings.api_key.clone(),
            "max_context": settings.max_context,
            "max_tokens": settings.max_tokens,
            "temperature": settings.temperature,
            "models": models,
            "scenario_models": {
                "default_model": settings.scenario_models.default_model.clone(),
                "project_model": settings.scenario_models.project_model.clone(),
                "session_model": settings.scenario_models.session_model.clone(),
                "memory_model": settings.scenario_models.memory_model.clone(),
                "task_model": settings.scenario_models.task_model.clone()
            }
        }
    });
    std::fs::write(&paths.config_path, toml::to_string_pretty(&config)?)?;
    Ok(())
}

pub fn default_llm_settings() -> LlmSettings {
    LlmSettings {
        id: "openrouter-default".into(),
        remark: "Default".into(),
        provider: "OpenRouter".into(),
        base_url: "https://openrouter.ai/api/v1".into(),
        interface: "openai".into(),
        model: String::new(),
        api_key: String::new(),
        max_context: DEFAULT_MAX_CONTEXT,
        max_tokens: DEFAULT_MAX_TOKENS,
        temperature: DEFAULT_TEMPERATURE,
        models: Vec::new(),
        scenario_models: LlmScenarioModels {
            default_model: String::new(),
            project_model: String::new(),
            session_model: String::new(),
            memory_model: String::new(),
            task_model: String::new(),
        },
    }
}

pub fn resolve_llm_settings(settings: &LlmSettings, scenario: LlmScenario) -> LlmSettings {
    let scenario_model = match scenario {
        LlmScenario::Default => &settings.scenario_models.default_model,
        LlmScenario::Project => &settings.scenario_models.project_model,
        LlmScenario::Session => &settings.scenario_models.session_model,
        LlmScenario::Memory => &settings.scenario_models.memory_model,
        LlmScenario::Task => &settings.scenario_models.task_model,
    };
    let model = settings
        .models
        .iter()
        .find(|model| model.id == *scenario_model)
        .or_else(|| default_model(settings));
    let Some(model) = model else {
        return settings.clone();
    };

    let mut resolved = settings.clone();
    apply_model(&mut resolved, model);
    resolved
}

fn default_model(settings: &LlmSettings) -> Option<&LlmModelSettings> {
    settings
        .models
        .iter()
        .find(|model| model.id == settings.scenario_models.default_model)
        .or_else(|| settings.models.first())
}

fn apply_model(settings: &mut LlmSettings, model: &LlmModelSettings) {
    settings.id = model.id.clone();
    settings.remark = model.remark.clone();
    settings.provider = model.provider.clone();
    settings.base_url = model.base_url.clone();
    settings.interface = model.interface.clone();
    settings.model = model.model.clone();
    settings.api_key = model.api_key.clone();
}

fn model_from_settings(settings: &LlmSettings) -> LlmModelSettings {
    LlmModelSettings {
        id: if settings.id.trim().is_empty() {
            llm_model_id(&settings.provider, &settings.remark)
        } else {
            settings.id.clone()
        },
        remark: if settings.remark.trim().is_empty() {
            "Default".into()
        } else {
            settings.remark.clone()
        },
        provider: settings.provider.clone(),
        base_url: settings.base_url.clone(),
        interface: settings.interface.clone(),
        model: settings.model.clone(),
        api_key: settings.api_key.clone(),
    }
}

fn model_from_toml(value: &toml::Value) -> Option<LlmModelSettings> {
    let table = value.as_table()?;
    let provider = table_string(table, "provider");
    let remark = table_string(table, "remark");
    if provider.trim().is_empty() || remark.trim().is_empty() {
        return None;
    }
    Some(LlmModelSettings {
        id: table_string(table, "id").if_empty_then(|| llm_model_id(&provider, &remark)),
        remark,
        provider,
        base_url: table_string(table, "base_url"),
        interface: table_string(table, "interface").if_empty_then(|| "openai".into()),
        model: table_string(table, "model"),
        api_key: table_string(table, "api_key"),
    })
}

fn scenario_string(table: &toml::map::Map<String, toml::Value>, key: &str) -> String {
    table_string(table, key)
}

fn table_string(table: &toml::map::Map<String, toml::Value>, key: &str) -> String {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn llm_model_id(provider: &str, remark: &str) -> String {
    let provider = provider.trim().to_lowercase().replace(' ', "-");
    let remark = remark.trim().to_lowercase().replace(' ', "-");
    format!("{provider}-{remark}")
}

trait EmptyStringExt {
    fn if_empty_then<F: FnOnce() -> String>(self, fallback: F) -> String;
}

impl EmptyStringExt for String {
    fn if_empty_then<F: FnOnce() -> String>(self, fallback: F) -> String {
        if self.trim().is_empty() {
            fallback()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_llm_settings, initialize_workspace, read_llm_settings, resolve_llm_settings,
        write_llm_settings, LlmScenario,
    };
    use crate::models::{AppPaths, LlmModelSettings};

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

    #[test]
    fn persists_saved_models_global_limits_and_scenario_fallbacks() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let mut settings = default_llm_settings();
        settings.id = "openrouter-fast".into();
        settings.remark = "Fast".into();
        settings.model = "openai/gpt-4o-mini".into();
        settings.api_key = "sk-openrouter".into();
        settings.max_context = 64_000;
        settings.max_tokens = 2_048;
        settings.temperature = 0.45;
        settings.models = vec![
            LlmModelSettings {
                id: "openrouter-fast".into(),
                provider: "OpenRouter".into(),
                remark: "Fast".into(),
                base_url: "https://openrouter.ai/api/v1".into(),
                interface: "openai".into(),
                model: "openai/gpt-4o-mini".into(),
                api_key: "sk-openrouter".into(),
            },
            LlmModelSettings {
                id: "anthropic-deep".into(),
                provider: "Anthropic".into(),
                remark: "Deep".into(),
                base_url: "https://api.anthropic.com".into(),
                interface: "anthropic".into(),
                model: "claude-3-5-sonnet-latest".into(),
                api_key: "sk-anthropic".into(),
            },
        ];
        settings.scenario_models.default_model = "openrouter-fast".into();
        settings.scenario_models.project_model = "anthropic-deep".into();

        write_llm_settings(&paths, &settings).unwrap();
        let read = read_llm_settings(&paths).unwrap();

        assert_eq!(read.max_context, 64_000);
        assert_eq!(read.max_tokens, 2_048);
        assert_eq!(read.temperature, 0.45);
        assert_eq!(read.models.len(), 2);
        assert_eq!(
            resolve_llm_settings(&read, LlmScenario::Project).model,
            "claude-3-5-sonnet-latest"
        );
        assert_eq!(
            resolve_llm_settings(&read, LlmScenario::Session).model,
            "openai/gpt-4o-mini"
        );
    }
}
