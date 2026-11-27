//! Effect Editor Panel
//!
//! The effect definitions as one filterable list; the picked effect's form
//! opens in the editor drawer.

use dioxus::prelude::*;

use super::def_list::{ListHead, sort_by_column};
use super::discipline_selector::class_of;
use super::effect_bulk::{BulkAction, BulkMenu};
use super::effect_import_modal::EffectImportPreviewModal;
use super::effect_row::{self, EffectRow};
use super::encounter_editor::InlineNameCreator;
use crate::api;
use crate::components::editor_common::Origin;
use crate::components::editor_drawer::{DiscardPrompt, layout_style, provide_drawer_guard};
use crate::components::editor_sort::{apply_rank, use_stable_rank};
use crate::components::effect_filters::{EffectFilterBar, matches_filters, resolved};
use crate::components::toast::{ToastManager, ToastSeverity, use_toast};
use crate::types::{
    AlertTrigger, AudioConfig, EffectImportPreview, EffectListItem, EntityFilter, Trigger, UiSessionState,
};

/// Create a default effect with sensible defaults
fn default_effect(name: String) -> EffectListItem {
    EffectListItem {
        id: String::new(),
        name,
        display_text: None,
        is_user_override: false,
        is_bundled: false,
        enabled: true,
        trigger: Trigger::EffectApplied {
            effects: vec![],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::Any,
            position: vec![],
        },
        ignore_effect_removed: false,
        refresh_abilities: vec![],
        duration_secs: Some(15.0),
        is_aoe_refresh: false,
        aoe_refresh_immediate: false,
        is_refreshed_on_modify: false,
        single_instance_per_target: false,
        default_charges: None,
        color: Some([80, 200, 80, 255]),
        show_at_secs: 0.0,
        display_targets: vec![],
        icon_ability_id: None,
        show_icon: true,
        display_source: false,
        show_target: false,
        track_uptime: false,
        track_uptime_after_first: false,
        stack_priority: false,
        show_countdown: true,
        show_single_stack: false,
        is_affected_by_alacrity: false,
        cooldown_ready_secs: 0.0,
        disciplines: vec![],
        source_disciplines: vec![],
        target_disciplines: vec![],
        ignore_refreshes: false,
        refresh_scope: Default::default(),
        persist_past_death: false,
        track_outside_combat: true,
        on_apply_trigger_timer: None,
        on_expire_trigger_timer: None,
        is_alert: false,
        alert_text: None,
        alert_on: AlertTrigger::None,
        alert_countdown_secs: None,
        audio: AudioConfig::default(),
        modifiers: vec![],
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Main Panel
// ─────────────────────────────────────────────────────────────────────────────

/// Marker ID for a draft effect that hasn't been saved yet
const DRAFT_EFFECT_ID: &str = "__new_draft__";

/// Bundled + user override = edited built-in
pub(crate) fn effect_origin(is_draft: bool, is_bundled: bool, is_user_override: bool) -> Origin {
    if is_draft {
        Origin::New
    } else {
        Origin::of(is_bundled && !is_user_override, is_bundled && is_user_override)
    }
}

/// The value, or the error as a toast
fn or_toast<T>(mut toast: ToastManager, result: Result<T, String>) -> Option<T> {
    result.map_err(|e| toast.show(e, ToastSeverity::Normal)).ok()
}

/// A mutation's outcome as a toast: `done` on success
fn notify<T>(mut toast: ToastManager, done: &str, result: Result<T, String>) -> Option<T> {
    let value = or_toast(toast, result)?;
    toast.show(done, ToastSeverity::Success);
    Some(value)
}

/// Export the given effects (none = every user effect) to a file the user picks
async fn export_to_file(toast: ToastManager, effect_ids: Option<&[String]>, default_name: &str) {
    let Some(toml) = or_toast(toast, api::export_effects_toml(effect_ids).await) else {
        return;
    };
    if let Some(path) = api::save_file_dialog(default_name).await {
        notify(toast, "Exported", api::save_export_file(&path, &toml).await);
    }
}

/// An effect ID from free text: lowercase snake_case ("Force Shroud (Copy)" → force_shroud_copy)
pub(crate) fn slug_id(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[derive(Props, Clone, PartialEq)]
pub struct EffectEditorProps {
    /// Unified UI session state (includes persisted state for this panel)
    pub state: Signal<UiSessionState>,
    /// Discipline of the logged-in character: its class starts the class filter
    #[props(default)]
    pub player_discipline: Option<String>,
}

#[component]
pub fn EffectEditorPanel(mut props: EffectEditorProps) -> Element {
    let guard = provide_drawer_guard();
    let toast = use_toast();
    let mut effects = use_signal(Vec::<EffectListItem>::new);
    let mut loading = use_signal(|| true);
    // Draft for new effects - not yet saved to backend
    let mut draft_effect = use_signal(|| None::<EffectListItem>);
    let mut import_preview = use_signal(|| None::<EffectImportPreview>);
    let mut import_toml_content = use_signal(|| None::<String>);

    // Persisted state: local copies, synced back one way
    let search_query = use_signal(|| props.state.read().effects_editor.search_query.clone());
    let mut expanded_effect = use_signal(|| props.state.read().effects_editor.expanded_effect.clone());
    let hide_disabled_effects = use_signal(|| props.state.read().effects_editor.hide_disabled_effects);
    let filters = use_signal(|| props.state.read().effects_editor.filters.clone());
    use_effect(move || {
        let mut state = props.state.write();
        state.effects_editor.search_query = search_query.read().clone();
        state.effects_editor.expanded_effect = expanded_effect.read().clone();
        state.effects_editor.hide_disabled_effects = *hide_disabled_effects.read();
        state.effects_editor.filters = filters.read().clone();
    });

    let reload = move || async move {
        if let Some(e) = api::get_effect_definitions().await {
            effects.set(e);
        }
    };
    use_future(move || async move {
        reload().await;
        loading.set(false);
    });

    // Bring the restored open effect into view once, when the list first
    // loads; picking a row later must not move the list (peek: no subscription)
    use_effect(move || {
        if !loading()
            && let Some(effect_id) = expanded_effect.peek().clone()
        {
            // Small delay to ensure DOM is rendered
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(100).await;
                if let Some(element) = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id(&format!("effect-{effect_id}")))
                {
                    element.scroll_into_view();
                }
            });
        }
    });

    let player_class = props.player_discipline.as_deref().and_then(class_of);

    // Search + hide-disabled; the filter counts run against this list
    let searched_effects = use_memo(move || {
        let query = search_query().to_lowercase();
        let hide_disabled = hide_disabled_effects();
        let hit = |s: &str| s.to_lowercase().contains(&query);
        effects()
            .into_iter()
            .filter(|e| {
                (!hide_disabled || e.enabled)
                    && (query.is_empty() || hit(&e.name) || hit(&e.id) || e.display_text.as_deref().is_some_and(hit))
            })
            .collect::<Vec<_>>()
    });
    let filtered_effects = use_memo(use_reactive!(|player_class| {
        let f = resolved(&filters.read(), player_class);
        searched_effects()
            .into_iter()
            .filter(|e| matches_filters(&f, e))
            .collect::<Vec<_>>()
    }));
    // Always A–Z (the backend order is arbitrary); a rename keeps its row in place
    let rank = use_stable_rank(effects.read().iter().map(|e| (e.id.as_str(), e.name.as_str())), true);
    let mut shown_effects = filtered_effects();
    apply_rank(&mut shown_effects, &rank.read(), |e| &e.id);
    let sort = use_signal(|| None);
    sort_by_column(&mut shown_effects, sort(), effect_row::sort_text);
    let disabled_count = effects.read().iter().filter(|e| !e.enabled).count();

    // Handlers
    let on_save = move |updated: EffectListItem| {
        spawn(async move {
            if notify(toast, "Saved", api::update_effect_definition(&updated).await).is_some() {
                // The saved values reach the list first, exactly as the form
                // holds them, so its draft reads as saved; only then the
                // refetch, whose changed origin flags the draft follows
                if let Some(e) = effects.write().iter_mut().find(|e| e.id == updated.id) {
                    *e = updated;
                }
                reload().await;
            }
        });
    };

    // Built-in items get "reset", not "delete"
    let mut on_delete = move |effect: EffectListItem| {
        expanded_effect.set(None);
        spawn(async move {
            let done = if effect.is_bundled {
                "Reset to built-in"
            } else {
                "Deleted"
            };
            notify(toast, done, api::delete_effect_definition(&effect.id).await);
            reload().await;
        });
    };

    // A copy opens as an unsaved draft, so its ID can be set before it is created
    let on_duplicate = move |effect: EffectListItem| {
        guard.confirm(move |_| {
            draft_effect.set(Some(EffectListItem {
                id: String::new(),
                name: format!("{} (Copy)", effect.name),
                is_bundled: false,
                is_user_override: false,
                ..effect.clone()
            }));
            expanded_effect.set(Some(DRAFT_EFFECT_ID.to_string()));
        });
    };

    let on_export_effect = move |effect: EffectListItem| {
        spawn(async move {
            export_to_file(
                toast,
                Some(std::slice::from_ref(&effect.id)),
                &format!("{}.toml", effect.id),
            )
            .await;
        });
    };

    // Bulk actions take the list as shown
    let shown_ids: Vec<String> = shown_effects.iter().map(|e| e.id.clone()).collect();
    let shown_count = shown_ids.len();
    let on_bulk = move |action: BulkAction| {
        let ids = shown_ids.clone();
        spawn(async move {
            let n = ids.len();
            let done = match action {
                BulkAction::Export => return export_to_file(toast, Some(&ids), "effects.toml").await,
                BulkAction::ExportOwn => return export_to_file(toast, None, "effects_custom.toml").await,
                BulkAction::Enabled(v) => {
                    let verb = if v { "Enabled" } else { "Disabled" };
                    notify(
                        toast,
                        &format!("{verb} {n} effects"),
                        api::set_effect_toggles(&ids, Some(v), None).await,
                    )
                }
                BulkAction::Audio(v) => {
                    let state = if v { "on" } else { "off" };
                    notify(
                        toast,
                        &format!("Audio {state} for {n} effects"),
                        api::set_effect_toggles(&ids, None, Some(v)).await,
                    )
                }
            };
            if done.is_some() {
                reload().await;
            }
        });
    };

    // A new effect is a local draft until its first save
    let on_create = move |name: String| {
        draft_effect.set(Some(default_effect(name)));
        expanded_effect.set(Some(DRAFT_EFFECT_ID.to_string()));
    };

    let on_save_draft = move |mut effect: EffectListItem| {
        // The ID typed in the form, else one made from the name
        effect.id = slug_id(if effect.id.trim().is_empty() {
            &effect.name
        } else {
            &effect.id
        });
        spawn(async move {
            if let Some(created) = notify(toast, "Created", api::create_effect_definition(&effect).await) {
                let created_id = created.id.clone();
                effects.write().push(created);
                draft_effect.set(None);
                expanded_effect.set(Some(created_id));
            }
        });
    };

    let on_cancel_draft = move |_: ()| {
        draft_effect.set(None);
        expanded_effect.set(None);
    };

    rsx! {
        div { class: "editor-layout", style: layout_style(),
            DiscardPrompt {}
            div { class: "editor-main effect-editor-panel",
                div { class: "effect-editor-header",
                    h2 { "Effect Definitions" }
                    div { class: "header-right",
                        BulkMenu { count: shown_count, total: effects.read().len(), on_pick: on_bulk }
                        button {
                            class: "btn btn-sm",
                            onclick: move |_| {
                                spawn(async move {
                                    let Some(path) = api::open_toml_file_dialog().await else { return };
                                    let Some(content) = or_toast(toast, api::read_import_file(&path).await) else { return };
                                    if let Some(preview) = or_toast(toast, api::preview_import_effects(&content).await) {
                                        import_toml_content.set(Some(content));
                                        import_preview.set(Some(preview));
                                    }
                                });
                            },
                            "Import"
                        }
                        InlineNameCreator {
                            button_label: "+ New Effect",
                            placeholder: "Effect name...",
                            on_create: on_create,
                        }
                    }
                }

                EffectFilterBar {
                    effects: searched_effects,
                    filters,
                    search: search_query,
                    hide_disabled: hide_disabled_effects,
                    disabled_count,
                    player_class,
                }

                if loading() {
                    div { class: "empty-state loading-late", "Loading effects..." }
                } else if filtered_effects().is_empty() && draft_effect().is_none() {
                    if effects().is_empty() {
                        div { class: "empty-state-guidance",
                            div { class: "empty-state-icon",
                                i { class: "fa-solid fa-sparkles" }
                            }
                            p { "No effects defined yet" }
                            p { class: "hint", "Click \"+ New Effect\" above to create your first effect" }
                        }
                    } else {
                        div { class: "empty-state", "No effects match the search and filters" }
                    }
                } else {
                    // The column names sit outside the scrolling rows
                    div { class: "def-table",
                    ListHead { cols: "effect-cols", columns: &effect_row::COLUMNS, sort }
                    div { class: "def-list",
                        if let Some(draft) = draft_effect() {
                            {
                                let is_draft_expanded = expanded_effect().as_deref() == Some(DRAFT_EFFECT_ID);
                                rsx! {
                                    EffectRow {
                                        key: "{DRAFT_EFFECT_ID}",
                                        effect: draft,
                                        expanded: is_draft_expanded,
                                        is_draft: true,
                                        on_toggle: move |_| {
                                            guard.switch(expanded_effect, (!is_draft_expanded).then(|| DRAFT_EFFECT_ID.to_string()));
                                        },
                                        on_save: on_save_draft,
                                        on_delete: on_cancel_draft,
                                        on_duplicate: move |_| {},
                                    }
                                }
                            }
                        }

                        for effect in shown_effects {
                            {
                                let effect_key = effect.id.clone();
                                let is_effect_expanded = expanded_effect().as_deref() == Some(effect_key.as_str());
                                let (for_delete, for_duplicate, for_export) = (effect.clone(), effect.clone(), effect.clone());

                                rsx! {
                                    EffectRow {
                                        key: "{effect_key}",
                                        effect,
                                        expanded: is_effect_expanded,
                                        on_toggle: move |_| {
                                            guard.switch(expanded_effect, (!is_effect_expanded).then(|| effect_key.clone()));
                                        },
                                        on_save: on_save,
                                        on_delete: move |_| on_delete(for_delete.clone()),
                                        on_duplicate: move |_| on_duplicate(for_duplicate.clone()),
                                        on_export: move |_| on_export_effect(for_export.clone()),
                                    }
                                }
                            }
                        }
                    }
                    }
                }

                if let Some(preview) = import_preview() {
                    EffectImportPreviewModal {
                        preview,
                        on_confirm: move |_| {
                            let content = import_toml_content().unwrap_or_default();
                            import_preview.set(None);
                            import_toml_content.set(None);
                            spawn(async move {
                                if notify(toast, "Imported", api::import_effects_toml(&content).await).is_some() {
                                    reload().await;
                                }
                            });
                        },
                        on_cancel: move |_| {
                            import_preview.set(None);
                            import_toml_content.set(None);
                        },
                    }
                }
            }
        }
    }
}
