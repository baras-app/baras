//! Characters tab: roster and character store edits

use tauri::State;

use baras_types::characters::{CharacterRecord, CharacterRoster, ManualLock};

use crate::service::ServiceHandle;

#[tauri::command]
pub async fn get_character_roster(handle: State<'_, ServiceHandle>) -> Result<CharacterRoster, String> {
    Ok(handle.character_roster().await)
}

/// Create or update; `previous_key` is the entry being renamed, if any.
#[tauri::command]
pub async fn save_character(
    record: CharacterRecord,
    previous_key: Option<String>,
    handle: State<'_, ServiceHandle>,
) -> Result<String, String> {
    handle
        .save_character(record, previous_key)
        .ok_or_else(|| "Character needs a name".to_string())
}

#[tauri::command]
pub async fn delete_character(key: String, handle: State<'_, ServiceHandle>) -> Result<bool, String> {
    Ok(handle.delete_character(&key))
}

#[tauri::command]
pub async fn restore_character(key: String, handle: State<'_, ServiceHandle>) -> Result<bool, String> {
    Ok(handle.restore_character(&key))
}

#[tauri::command]
pub async fn get_lockout_reset_hour(handle: State<'_, ServiceHandle>) -> Result<u8, String> {
    Ok(handle.lockout_reset_hour())
}

#[tauri::command]
pub async fn set_lockout_reset_hour(hour: u8, handle: State<'_, ServiceHandle>) -> Result<bool, String> {
    Ok(handle.set_lockout_reset_hour(hour))
}

#[tauri::command]
pub async fn add_manual_lock(key: String, lock: ManualLock, handle: State<'_, ServiceHandle>) -> Result<bool, String> {
    Ok(handle.add_manual_lock(&key, lock))
}

#[tauri::command]
pub async fn clear_manual_lock(
    key: String,
    operation: String,
    tier: String,
    handle: State<'_, ServiceHandle>,
) -> Result<bool, String> {
    Ok(handle.clear_manual_lock(&key, &operation, &tier))
}
