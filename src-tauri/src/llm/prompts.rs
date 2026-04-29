pub(crate) fn session_transcript(session: &crate::models::StoredSession) -> String {
    session
        .messages
        .iter()
        .filter(|message| matches!(message.role.as_str(), "user" | "assistant"))
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn session_user_transcript(session: &crate::models::StoredSession) -> String {
    session
        .messages
        .iter()
        .filter(|message| message.role == "user")
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn strip_llm_think_blocks(markdown: &str) -> String {
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
