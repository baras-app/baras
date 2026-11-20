//! Damage Taken summary: two small tables (damage types, mitigation) in one
//! column layout: name, share, amount ("—" when there is none).

use dioxus::prelude::*;

use baras_types::formatting;

/// (name, share %, amount); no share while the summary is still loading
pub type DtRow = (&'static str, Option<f64>, Option<f64>);

#[component]
pub fn DtSummaryTable(title: &'static str, rows: Vec<DtRow>, european: bool) -> Element {
    rsx! {
        div { class: "dt-summary-table",
            div { class: "dt-summary-title", "{title}" }
            for (name, pct, amount) in rows {
                span { class: "dt-summary-label", "{name}" }
                span { class: "dt-summary-pct",
                    match pct {
                        Some(p) => formatting::format_pct(p, european),
                        None => "—".to_string(),
                    }
                }
                span { class: "dt-summary-val",
                    match amount.filter(|a| *a > 0.0) {
                        Some(a) => formatting::format_compact_f64(a, european),
                        None => "—".to_string(),
                    }
                }
            }
        }
    }
}
