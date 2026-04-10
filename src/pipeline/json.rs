pub(crate) fn extract_json_payload(raw: &str) -> &str {
    let trimmed = raw.trim();

    if let Some(inner) = trimmed
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
    {
        inner.trim()
    } else if let Some(inner) = trimmed
        .strip_prefix("```")
        .and_then(|value| value.strip_suffix("```"))
    {
        inner.trim()
    } else {
        trimmed
    }
}
