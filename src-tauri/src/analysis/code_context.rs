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

