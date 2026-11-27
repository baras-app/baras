use std::fmt::Write;

/// Values for the placeholders in user-authored display/alert text:
/// `{source}`, `{target}`, `{ability}`, `{effect}` and `{stack}`.
/// `{ability}`/`{effect}` are the names the combat log recorded
/// (`[Ability] [ApplyEffect: Effect]`), so one definition covering both
/// factions shows each player's own name. A `None` value leaves its
/// placeholder untouched (timers have no stack count or log names).
#[derive(Debug, Default, Clone, Copy)]
pub struct Placeholders<'a> {
    pub source: Option<&'a str>,
    pub target: Option<&'a str>,
    pub ability: Option<&'a str>,
    pub effect: Option<&'a str>,
    pub stacks: Option<u8>,
}

impl Placeholders<'_> {
    /// Substitute every known placeholder in one pass, then trim
    pub fn format(&self, text: &str) -> String {
        if !text.contains('{') {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len() + 16);
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            rest = &rest[open..];
            let replaced = rest.find('}').and_then(|close| {
                let value = match &rest[1..close] {
                    "source" => self.source.map(|s| out.push_str(s)),
                    "target" => self.target.map(|s| out.push_str(s)),
                    "ability" => self.ability.map(|s| out.push_str(s)),
                    "effect" => self.effect.map(|s| out.push_str(s)),
                    "stack" => self.stacks.map(|n| {
                        let _ = write!(out, "{n}");
                    }),
                    _ => None,
                };
                value.map(|()| close + 1)
            });
            let skip = replaced.unwrap_or_else(|| {
                out.push('{');
                1
            });
            rest = &rest[skip..];
        }
        out.push_str(rest);
        let trimmed = out.trim();
        if trimmed.len() == out.len() {
            out
        } else {
            trimmed.to_string()
        }
    }
}

