/// Substitute `{source}`, `{target}` and `{stack}` in user-authored display/alert text.
/// `stacks: None` leaves `{stack}` untouched (timers have no stack count).
pub fn format_placeholders(text: &str, source: &str, target: &str, stacks: Option<u8>) -> String {
    if !text.contains('{') {
        return text.to_string();
    }
    let mut out = text.replace("{source}", source).replace("{target}", target);
    if let Some(stacks) = stacks
        && out.contains("{stack}")
    {
        out = out.replace("{stack}", &stacks.to_string());
    }
    out.trim().to_string()
}
