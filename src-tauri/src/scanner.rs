use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::Value;
use walkdir::WalkDir;

use crate::models::{RawMessage, RawSession};

pub fn scan_codex_sessions(root: &Path) -> anyhow::Result<Vec<RawSession>> {
    scan_sessions(root, "codex", None)
}

pub fn scan_claude_sessions(root: &Path) -> anyhow::Result<Vec<RawSession>> {
    scan_sessions(root, "claude", Some(root))
}

fn scan_sessions(
    root: &Path,
    source: &str,
    claude_root: Option<&Path>,
) -> anyhow::Result<Vec<RawSession>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !matches!(extension, "jsonl" | "json") {
            continue;
        }
        if let Some(session) = parse_session_file(path, source, claude_root)? {
            sessions.push(session);
        }
    }
    Ok(sessions)
}

fn parse_session_file(
    path: &Path,
    source: &str,
    claude_root: Option<&Path>,
) -> anyhow::Result<Option<RawSession>> {
    let text = std::fs::read_to_string(path)?;
    let values = if path.extension().and_then(|value| value.to_str()) == Some("json") {
        parse_json_values(&text)
    } else {
        text.lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .collect()
    };

    let mut session_id = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("session")
        .to_string();
    let mut workdir: Option<String> = None;
    let mut created_at: Option<String> = None;
    let mut messages = Vec::new();

    for value in values {
        if let Some(id) = find_string(&value, &["id", "session_id", "sessionId", "uuid"]) {
            session_id = id;
        }
        let record_type = value.get("type").and_then(Value::as_str);
        if let Some(timestamp) =
            find_string(&value, &["timestamp", "created_at", "createdAt", "time"])
        {
            if source != "claude" && source != "codex" && created_at.is_none() {
                created_at = Some(normalize_timestamp(&timestamp));
            }
        }

        if source == "codex" && record_type == Some("session_meta") {
            if let Some(id) = nested_string(&value, &["payload", "id"]) {
                session_id = id;
            }
            workdir = nested_string(&value, &["payload", "cwd"]);
            if let Some(timestamp) = nested_string(&value, &["payload", "timestamp"]) {
                created_at = Some(normalize_timestamp(&timestamp));
            }
            continue;
        }

        if source == "claude" && record_type == Some("user") {
            if workdir.is_none() {
                workdir = find_string(&value, &["cwd", "workdir"]);
            }
            if created_at.is_none() {
                if let Some(timestamp) = find_string(&value, &["timestamp"]) {
                    created_at = Some(normalize_timestamp(&timestamp));
                }
            }
        }

        if workdir.is_none() && source != "claude" {
            workdir = find_string(&value, &["cwd", "workdir"])
                .or_else(|| nested_string(&value, &["metadata", "cwd"]))
                .or_else(|| nested_string(&value, &["metadata", "workdir"]))
                .or_else(|| nested_string(&value, &["context", "cwd"]));
        }

        if let Some(items) = value.get("messages").and_then(Value::as_array) {
            for item in items {
                push_message(item, &mut messages);
            }
        } else if record_type == Some("response_item") {
            if let Some(payload) = value.get("payload") {
                push_message(payload, &mut messages);
            }
        } else {
            push_message(&value, &mut messages);
        }
    }

    if messages.is_empty() {
        return Ok(None);
    }

    let modified_time = file_modified_rfc3339(path);
    let workdir = workdir
        .or_else(|| claude_root.and_then(|root| decode_claude_project_workdir(root, path)))
        .unwrap_or_else(|| "UnknownProject".into());
    Ok(Some(RawSession {
        source: source.into(),
        session_id,
        workdir,
        created_at: created_at.unwrap_or_else(|| modified_time.clone()),
        updated_at: modified_time,
        raw_path: path.to_string_lossy().to_string(),
        messages,
    }))
}

fn parse_json_values(text: &str) -> Vec<Value> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    match value {
        Value::Array(values) => values,
        value => vec![value],
    }
}

fn push_message(value: &Value, messages: &mut Vec<RawMessage>) {
    let message = value.get("message").unwrap_or(value);
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .or_else(|| value.get("role").and_then(Value::as_str))
        .or_else(|| value.get("type").and_then(Value::as_str));
    let Some(role) = role else {
        return;
    };
    if !matches!(role, "user" | "assistant") {
        return;
    }
    let content = content_to_text(message.get("content").or_else(|| value.get("content")));
    if content.trim().is_empty() {
        return;
    }
    messages.push(RawMessage {
        role: role.to_string(),
        content,
    });
}

fn content_to_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                item.as_str()
                    .map(ToString::to_string)
                    .or_else(|| {
                        item.get("text")
                            .and_then(Value::as_str)
                            .map(ToString::to_string)
                    })
                    .or_else(|| {
                        item.get("content")
                            .and_then(Value::as_str)
                            .map(ToString::to_string)
                    })
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(Value::Object(_)) => value
            .and_then(|item| item.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(ToString::to_string)
}

fn nested_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_str().map(ToString::to_string)
}

fn normalize_timestamp(timestamp: &str) -> String {
    DateTime::parse_from_rfc3339(timestamp)
        .map(|value| {
            value
                .with_timezone(&Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
        .unwrap_or_else(|_| timestamp.to_string())
}

fn file_modified_rfc3339(path: &Path) -> String {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .map(DateTime::<Utc>::from)
        .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|_| crate::utils::now_rfc3339())
}

fn decode_claude_project_workdir(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut components = relative.components();
    if components.next()?.as_os_str() != "projects" {
        return None;
    }
    let encoded = components.next()?.as_os_str().to_str()?;
    if encoded.starts_with('-') {
        Some(encoded.replace('-', "/"))
    } else {
        Some(encoded.replace('-', "/"))
    }
}

#[cfg(test)]
mod tests {
    use super::{scan_claude_sessions, scan_codex_sessions};

    #[test]
    fn scan_codex_jsonl_reads_project_and_start_time_from_session_meta() {
        let temp = tempfile::tempdir().unwrap();
        let codex_home = temp.path().join("codex");
        let session_dir = codex_home.join("sessions/2026/04/26");
        std::fs::create_dir_all(&session_dir).unwrap();
        let file = session_dir.join("codex-1.jsonl");
        std::fs::write(
            &file,
            r#"{"type":"session_meta","payload":{"id":"codex-1","cwd":"/Users/kc/project","timestamp":"2026-04-26T01:00:00Z"}}"#
                .to_owned()
                + "\n"
                + r#"{"id":"codex-1","timestamp":"2026-04-26T01:01:00Z","message":{"role":"user","content":"Review this"}}"#
                + "\n"
                + r#"{"id":"codex-1","timestamp":"2026-04-26T01:02:00Z","message":{"role":"assistant","content":"Reviewed"}}"#,
        )
        .unwrap();

        let sessions = scan_codex_sessions(&codex_home.join("sessions")).unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].source, "codex");
        assert_eq!(sessions[0].session_id, "codex-1");
        assert_eq!(sessions[0].workdir, "/Users/kc/project");
        assert_eq!(sessions[0].created_at, "2026-04-26T01:00:00Z");
        assert_eq!(sessions[0].messages.len(), 2);
        assert_eq!(sessions[0].messages[0].role, "user");
        assert_eq!(sessions[0].messages[1].role, "assistant");
    }

    #[test]
    fn scan_codex_jsonl_reads_response_item_payload_messages() {
        let temp = tempfile::tempdir().unwrap();
        let session_dir = temp.path().join("sessions/2026/04/26");
        std::fs::create_dir_all(&session_dir).unwrap();
        let file = session_dir.join("rollout-codex-2.jsonl");
        std::fs::write(
            &file,
            r#"{"type":"session_meta","payload":{"id":"codex-2","cwd":"/Users/kc/KittyNest","timestamp":"2026-04-26T01:00:00Z"}}"#
                .to_owned()
                + "\n"
                + r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Import Codex sessions"}]}}"#
                + "\n"
                + r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Imported"}]}}"#,
        )
        .unwrap();

        let sessions = scan_codex_sessions(&temp.path().join("sessions")).unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].source, "codex");
        assert_eq!(sessions[0].session_id, "codex-2");
        assert_eq!(sessions[0].workdir, "/Users/kc/KittyNest");
        assert_eq!(sessions[0].created_at, "2026-04-26T01:00:00Z");
        assert_eq!(sessions[0].messages.len(), 2);
        assert_eq!(sessions[0].messages[0].content, "Import Codex sessions");
        assert_eq!(sessions[0].messages[1].content, "Imported");
    }

    #[test]
    fn scan_claude_project_sessions_reads_project_and_start_time_from_second_user_line() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("projects/not-the-workdir");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(
            project_dir.join("claude-1.jsonl"),
            r#"{"uuid":"claude-1","timestamp":"2026-04-26T01:59:00Z","type":"summary","summary":"ignored"}"#
                .to_owned()
                + "\n"
                + r#"{"uuid":"claude-1","timestamp":"2026-04-26T02:00:00Z","type":"user","cwd":"/Users/kc/demo","message":{"role":"user","content":"Ship it"}}"#
                + "\n"
                + r#"{"uuid":"claude-1","timestamp":"2026-04-26T02:01:00Z","type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Shipped"}]}}"#,
        )
        .unwrap();

        let sessions = scan_claude_sessions(temp.path()).unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].source, "claude");
        assert_eq!(sessions[0].workdir, "/Users/kc/demo");
        assert_eq!(sessions[0].created_at, "2026-04-26T02:00:00Z");
        assert_eq!(sessions[0].messages.len(), 2);
    }
}
