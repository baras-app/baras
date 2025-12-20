use crate::overlay::{create_all_entries, OverlayCommand, OverlayType, PersonalOverlayCommand};
use crate::service::OverlayUpdate;
use crate::SharedOverlayState;
use tokio::sync::mpsc;

/// Bridge between service overlay updates and the overlay threads
pub fn spawn_overlay_bridge(
    mut rx: mpsc::Receiver<OverlayUpdate>,
    overlay_state: SharedOverlayState,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(update) = rx.recv().await {
            match update {
                OverlayUpdate::MetricsUpdated(metrics) => {
                    // Create entries for all overlay types
                    let all_entries = create_all_entries(&metrics);

                    // Get running overlays and their channels
                    let overlay_txs: Vec<_> = {
                        let state = match overlay_state.lock() {
                            Ok(s) => s,
                            Err(_) => continue,
                        };

                        OverlayType::all()
                            .iter()
                            .filter_map(|&overlay_type| {
                                state.get_tx(overlay_type).cloned().map(|tx| (overlay_type, tx))
                            })
                            .collect()
                    };

                    // Send entries to each running overlay
                    for (overlay_type, tx) in overlay_txs {
                        if let Some(entries) = all_entries.get(&overlay_type) {
                            let _ = tx.send(OverlayCommand::UpdateEntries(entries.clone())).await;
                        }
                    }
                }
                OverlayUpdate::PersonalStatsUpdated(stats) => {
                    // Get personal overlay channel
                    let personal_tx = {
                        let state = match overlay_state.lock() {
                            Ok(s) => s,
                            Err(_) => continue,
                        };
                        state.get_personal_tx().cloned()
                    };

                    // Send stats to personal overlay
                    if let Some(tx) = personal_tx {
                        let _ = tx.send(PersonalOverlayCommand::UpdateStats(stats)).await;
                    }
                }
                OverlayUpdate::CombatStarted => {
                    // Could show overlay or clear entries
                }
                OverlayUpdate::CombatEnded => {
                    // Could hide overlay or freeze display
                }
            }
        }
    });
}
