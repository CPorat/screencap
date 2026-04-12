pub(crate) fn extract_json_payload(raw: &str) -> &str {
    let trimmed = raw.trim();

    if let Some(after_prefix) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    {
        let after_prefix = after_prefix.trim_start_matches(|ch: char| ch == '\n' || ch == '\r');
        if let Some(inner) = after_prefix.strip_suffix("```") {
            return inner.trim();
        }
        return after_prefix.trim();
    }

    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_json_fence() {
        assert_eq!(extract_json_payload("```json\n{\"a\":1}\n```"), r#"{"a":1}"#);
    }

    #[test]
    fn strips_fence_with_trailing_newline() {
        assert_eq!(
            extract_json_payload("```json\n{\"a\":1}\n```\n"),
            r#"{"a":1}"#
        );
    }

    #[test]
    fn strips_plain_fence() {
        assert_eq!(extract_json_payload("```\n{\"a\":1}\n```"), r#"{"a":1}"#);
    }

    #[test]
    fn returns_raw_json() {
        assert_eq!(extract_json_payload(r#"  {"a":1}  "#), r#"{"a":1}"#);
    }
}
