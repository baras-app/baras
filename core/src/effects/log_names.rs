//! Names from the triggering log line that fill an effect's text placeholders.

use crate::context::{IStr, resolve};
use crate::dsl::Placeholders;

/// Source, target, ability and effect names for `{source}`, `{target}`,
/// `{ability}` and `{effect}` (the ability's name when no effect was logged)
#[derive(Debug, Clone, Copy)]
pub(super) struct LogNames {
    pub source: IStr,
    pub target: IStr,
    pub ability: IStr,
    pub effect: IStr,
}

impl LogNames {
    /// Resolve the placeholders in display or alert `text`
    pub fn format(self, text: &str, stacks: u8) -> String {
        Placeholders {
            source: Some(resolve(self.source)),
            target: Some(resolve(self.target)),
            ability: Some(resolve(self.ability)),
            effect: Some(resolve(self.effect)),
            stacks: Some(stacks),
        }
        .format(text)
    }
}
