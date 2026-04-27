pub fn render_frontmatter_markdown(frontmatter: &[(&str, String)], body: &str) -> String {
    let mut output = String::from("---\n");
    for (key, value) in frontmatter {
        output.push_str(key);
        output.push_str(": ");
        output.push_str(&value.replace('\n', " ").replace('\r', " "));
        output.push('\n');
    }
    output.push_str("---\n\n");
    output.push_str(body.trim_end());
    output.push('\n');
    output
}

#[cfg(test)]
mod tests {
    use super::render_frontmatter_markdown;

    #[test]
    fn render_frontmatter_markdown_wraps_machine_fields_and_body() {
        let rendered = render_frontmatter_markdown(
            &[
                ("slug", "session-ingest".into()),
                ("status", "developing".into()),
            ],
            "# Session Ingest\n\nSummary.",
        );

        assert!(rendered.starts_with("---\n"));
        assert!(rendered.contains("slug: session-ingest\n"));
        assert!(rendered.contains("status: developing\n"));
        assert!(rendered.ends_with("# Session Ingest\n\nSummary.\n"));
    }
}
