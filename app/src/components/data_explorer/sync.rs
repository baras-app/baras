//! One-way syncs between the panel's local signals and the parent `UiSessionState`.

use dioxus::prelude::*;

use super::ctx::ExplorerCtx;

pub(super) fn use_state_sync(ctx: ExplorerCtx) {
    let ExplorerCtx {
        mut state,
        selected_encounter,
        mut auto_live,
        mut auto_follow,
        view_mode,
        selected_source,
        selected_source_id,
        breakdown_mode,
        show_players_only,
        mut show_only_bosses,
        sort_column,
        sort_direction,
        collapsed_sections,
        selected_rotation_anchor,
        usage_sort_column,
        usage_sort_direction,
        mut combat_log_state,
        time_range,
        ..
    } = ctx;

    // Sync local signals back to unified state when they change
    // Read all values first to create subscriptions, then write to parent state
    // NOTE: show_only_bosses is NOT synced here to avoid bidirectional sync loop.
    // It's synced in the onchange handler instead.
    use_effect(move || {
        let enc = *selected_encounter.read();
        let vm = *view_mode.read();
        let src = selected_source.read().clone();
        let src_id = *selected_source_id.read();
        let bm = *breakdown_mode.read();
        let players = *show_players_only.read();
        let col = *sort_column.read();
        let dir = *sort_direction.read();
        let sections = collapsed_sections.read().clone();
        let tr = *time_range.read();
        let anchor = *selected_rotation_anchor.read();
        let u_col = *usage_sort_column.read();
        let u_dir = *usage_sort_direction.read();
        let combat = combat_log_state.read().clone();

        if let Ok(mut state) = state.try_write() {
            state.data_explorer.selected_encounter = enc;
            state.data_explorer.view_mode = vm;
            state.data_explorer.selected_source = src;
            state.data_explorer.selected_source_id = src_id;
            state.data_explorer.breakdown_mode = bm;
            state.data_explorer.show_players_only = players;
            state.data_explorer.sort_column = col;
            state.data_explorer.sort_direction = dir;
            state.data_explorer.collapsed_sections = sections;
            state.data_explorer.time_range = tr;
            state.data_explorer.selected_rotation_anchor = anchor;
            state.data_explorer.usage_sort_column = u_col;
            state.data_explorer.usage_sort_direction = u_dir;
            state.combat_log = combat;
        }
    });

    // Sync show_only_bosses from parent state when it changes (e.g., config loaded at startup)
    // This handles the race condition where the component mounts before async config loading completes
    // NOTE: We only sync parent→local here. Local→parent sync happens in the onchange handler
    // to avoid a bidirectional sync loop that causes "sticky" toggle behavior.
    use_effect(move || {
        let parent_bosses = state.read().data_explorer.show_only_bosses;
        if *show_only_bosses.read() != parent_bosses {
            show_only_bosses.set(parent_bosses);
        }
    });

    // Sync auto_live preference from parent state when config loads asynchronously.
    // Also updates auto_follow to match: if user has auto_live off, auto_follow stays off.
    use_effect(move || {
        let parent_auto_live = state.read().data_explorer.auto_live;
        if *auto_live.read() != parent_auto_live {
            auto_live.set(parent_auto_live);
            // If auto_live just got enabled and no encounter is selected, enable auto_follow
            if parent_auto_live && selected_encounter.read().is_none() {
                auto_follow.set(true);
            }
            // If auto_live just got disabled, disable auto_follow too
            if !parent_auto_live {
                auto_follow.set(false);
            }
        }
    });

    // NOTE: selected_encounter is initialized from parent state on mount (use_explorer_ctx).
    // This handles navigation from external components that set selected_encounter before mounting.
    // We do NOT have a continuous parent→local sync effect here because it creates
    // a bidirectional sync loop with the local→parent sync above, causing
    // encounter clicks in the sidebar to be ignored/sticky.

    // Sync show_ids from parent state when it changes (e.g., config loaded at startup)
    use_effect(move || {
        let parent_show_ids = state.read().combat_log.show_ids;
        if combat_log_state.read().show_ids != parent_show_ids {
            combat_log_state.write().show_ids = parent_show_ids;
        }
    });
}
