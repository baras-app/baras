use crate::CombatEvent;

#[derive(Default)]
pub struct AppState {
    pub events: Vec<CombatEvent>,
    pub current_byte: Option<u64>,
}
