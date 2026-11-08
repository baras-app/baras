//! Log Management rules on the Settings page: the automatic cleanup rules,
//! the content that is never deleted, and whether new logs are scanned at
//! startup. Every change saves immediately. The actions that apply them
//! (run cleanup, scan, rebuild) live on the Log Management page itself.

use dioxus::prelude::*;

use baras_types::history::{ContentTag, LogCleanup};

use crate::api;
use crate::components::{ToastSeverity, use_toast};

const MB: f64 = 1_048_576.0;

async fn save_rules(rules: LogCleanup) -> Result<(), String> {
    let mut cfg = api::get_config().await.ok_or("Settings unavailable")?;
    cfg.log_cleanup = rules;
    api::update_config(&cfg).await
}

async fn save_auto_scan(on: bool) -> Result<(), String> {
    let mut cfg = api::get_config().await.ok_or("Settings unavailable")?;
    cfg.auto_scan_history = on;
    api::update_config(&cfg).await
}

#[component]
pub fn CleanupRules() -> Element {
    let mut toast = use_toast();
    let mut rules = use_signal(LogCleanup::default);
    let mut auto_scan = use_signal(|| true);
    let mut retain = use_signal(Vec::<ContentTag>::new);

    // Saved settings seed the controls once; edits save straight back
    let config = use_resource(|| async move { api::get_config().await });
    use_effect(move || {
        if let Some(Some(cfg)) = config.read().as_ref() {
            rules.set(cfg.log_cleanup.clone());
            auto_scan.set(cfg.auto_scan_history);
        }
    });
    let retain_loaded = use_resource(|| async move { api::get_retained_content().await.unwrap_or_default() });
    use_effect(move || {
        if let Some(r) = retain_loaded.read().as_ref() {
            retain.set(r.clone());
        }
    });

    let mut set_rules = move |r: LogCleanup| {
        rules.set(r.clone());
        spawn(async move {
            if let Err(err) = save_rules(r).await {
                toast.show(format!("Failed to save settings: {err}"), ToastSeverity::Normal);
            }
        });
    };
    let mut toggle_retain = move |tag: ContentTag, on: bool| {
        let mut list = retain.read().clone();
        list.retain(|t| *t != tag);
        if on {
            list.push(tag);
        }
        retain.set(list.clone());
        spawn(async move {
            if let Err(err) = api::set_retained_content(&list).await {
                toast.show(format!("Failed to save retention list: {err}"), ToastSeverity::Normal);
            }
        });
    };
    let r = rules.read().clone();

    rsx! {
        div { class: "cleanup-rules",
            label { class: "settings-check",
                input {
                    r#type: "checkbox",
                    checked: *auto_scan.read(),
                    onchange: move |e| {
                        let on = e.checked();
                        auto_scan.set(on);
                        spawn(async move {
                            if let Err(err) = save_auto_scan(on).await {
                                toast.show(format!("Failed to save settings: {err}"), ToastSeverity::Normal);
                            }
                        });
                    },
                }
                "Scan new logs at startup"
            }
            p { class: "hint", "The statistics pages are built from scanned logs, archived ones included. You can also scan from Log Management." }

            span { class: "library-retain-title", "Automatic cleanup" }
            div { class: "library-rules",
                RuleRow {
                    on: r.delete_smaller_than > 0,
                    text: "Delete logs smaller than",
                    unit: "MB",
                    step: "0.1",
                    min: "0.1",
                    value: format!("{:.1}", r.delete_smaller_than as f64 / MB),
                    on_toggle: move |on: bool| {
                        let mut r = rules.read().clone();
                        r.delete_smaller_than = if on { MB as u64 } else { 0 };
                        set_rules(r);
                    },
                    on_value: move |v: String| {
                        if let Ok(mb) = v.parse::<f64>() && mb > 0.0 {
                            let mut r = rules.read().clone();
                            r.delete_smaller_than = (mb * MB) as u64;
                            set_rules(r);
                        }
                    },
                }
                RuleRow {
                    on: r.archive_after_days.is_some(),
                    text: "Archive logs older than",
                    unit: "days",
                    step: "1",
                    min: "1",
                    value: r.archive_after_days.unwrap_or(30).to_string(),
                    on_toggle: move |on: bool| {
                        let mut r = rules.read().clone();
                        r.archive_after_days = on.then_some(30);
                        set_rules(r);
                    },
                    on_value: move |v: String| {
                        if let Ok(d) = v.parse::<u32>() && d > 0 {
                            let mut r = rules.read().clone();
                            r.archive_after_days = Some(d);
                            set_rules(r);
                        }
                    },
                }
                RuleRow {
                    on: r.delete_after_days.is_some(),
                    text: "Delete logs older than",
                    unit: "days",
                    step: "1",
                    min: "1",
                    value: r.delete_after_days.unwrap_or(90).to_string(),
                    on_toggle: move |on: bool| {
                        let mut r = rules.read().clone();
                        r.delete_after_days = on.then_some(90);
                        set_rules(r);
                    },
                    on_value: move |v: String| {
                        if let Ok(d) = v.parse::<u32>() && d > 0 {
                            let mut r = rules.read().clone();
                            r.delete_after_days = Some(d);
                            set_rules(r);
                        }
                    },
                }
            }
            div { class: "library-retain",
                span { class: "library-retain-title", "Never delete logs containing" }
                div { class: "library-retain-tags",
                    for tag in ContentTag::ALL {
                        {
                            let on = retain.read().contains(&tag);
                            rsx! {
                                label { class: if on { "lib-tag-toggle on" } else { "lib-tag-toggle" },
                                    input { r#type: "checkbox", checked: on, onchange: move |e| toggle_retain(tag, e.checked()) }
                                    {tag.label()}
                                }
                            }
                        }
                    }
                }
            }
            p { class: "hint",
                "Archived logs are compressed but stay in the statistics and can be restored from Log Management. Deleted logs are removed for good; their pulls stay in the statistics. Protected content can still be archived. Favorites, the live log and today's logs are never touched. Cleanup runs at startup or from Log Management."
            }
        }
    }
}

/// One rule: a checkbox, its wording, and the number it applies.
#[component]
fn RuleRow(
    on: bool,
    text: String,
    unit: String,
    step: String,
    min: String,
    value: String,
    on_toggle: EventHandler<bool>,
    on_value: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "library-rule",
            label { class: "settings-check",
                input { r#type: "checkbox", checked: on, onchange: move |e| on_toggle.call(e.checked()) }
                "{text}"
            }
            input {
                class: "library-num",
                r#type: "number",
                step: "{step}",
                min: "{min}",
                disabled: !on,
                value: "{value}",
                onchange: move |e| on_value.call(e.value()),
            }
            span { class: "hint", "{unit}" }
        }
    }
}
