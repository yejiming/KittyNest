use crate::models::ProviderPreset;

pub fn provider_presets() -> Vec<ProviderPreset> {
    [
        ("DeepSeek", "https://api.deepseek.com/v1", "openai"),
        (
            "Zhipu GLM",
            "https://open.bigmodel.cn/api/paas/v4",
            "openai",
        ),
        ("Zhipu GLM en", "https://api.z.ai/v1", "openai"),
        (
            "Bailian",
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            "openai",
        ),
        ("Kimi", "https://api.moonshot.cn/v1", "openai"),
        (
            "Kimi For Coding",
            "https://api.kimi.com/coding",
            "anthropic",
        ),
        ("StepFun", "https://api.stepfun.ai/v1", "openai"),
        ("Minimax", "https://api.minimaxi.com/v1", "openai"),
        ("Minimax en", "https://platform.minimax.io", "openai"),
        (
            "DouBaoSeed",
            "https://ark.cn-beijing.volces.com/api/v3",
            "openai",
        ),
        ("Xiaomi MiMo", "https://api.xiaomimimo.com/v1", "openai"),
        (
            "ModelScope",
            "https://api-inference.modelscope.cn/v1",
            "openai",
        ),
        ("OpenRouter", "https://openrouter.ai/api/v1", "openai"),
        ("Ollama", "http://localhost:11434/v1", "openai"),
    ]
    .into_iter()
    .map(|(provider, base_url, interface)| ProviderPreset {
        provider: provider.into(),
        base_url: base_url.into(),
        interface: interface.into(),
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::provider_presets;

    #[test]
    fn includes_required_provider_presets() {
        let presets = provider_presets();
        let names: Vec<_> = presets
            .iter()
            .map(|preset| preset.provider.as_str())
            .collect();

        for required in [
            "DeepSeek",
            "Zhipu GLM",
            "Bailian",
            "Kimi",
            "StepFun",
            "Minimax",
            "DouBaoSeed",
            "ModelScope",
            "OpenRouter",
            "Ollama",
        ] {
            assert!(
                names.contains(&required),
                "missing provider preset {required}"
            );
        }

        assert!(presets.iter().all(|preset| {
            matches!(preset.interface.as_str(), "openai" | "anthropic")
                && !preset.base_url.trim().is_empty()
        }));
    }
}
