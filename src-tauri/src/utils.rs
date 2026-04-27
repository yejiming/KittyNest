use chrono::Utc;

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn project_slug_from_workdir(workdir: &str) -> String {
    let name = std::path::Path::new(workdir)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("UnknownProject");
    sanitize_preserving_case(name)
}

pub fn slugify_lower(input: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in input.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "untitled".into()
    } else {
        slug
    }
}

pub fn title_from_slug(slug: &str) -> String {
    slug.split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn first_words_slug(text: &str, fallback: &str, max_words: usize) -> String {
    let words: Vec<&str> = text
        .split_whitespace()
        .filter(|word| word.chars().any(|ch| ch.is_ascii_alphanumeric()))
        .take(max_words)
        .collect();
    let candidate = if words.is_empty() {
        fallback.to_string()
    } else {
        words.join(" ")
    };
    slugify_lower(&candidate)
}

fn sanitize_preserving_case(input: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            output.push(ch);
            last_dash = false;
        } else if !last_dash && !output.is_empty() {
            output.push('-');
            last_dash = true;
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    if output.is_empty() {
        "UnknownProject".into()
    } else {
        output
    }
}
