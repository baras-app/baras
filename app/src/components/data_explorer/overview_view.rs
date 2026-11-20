//! Overview tab: raid table(s), group chart, NPC health and challenges.

use dioxus::prelude::*;
use std::collections::HashMap;

use super::ctx::{ExplorerCtx, repaint_with};
use crate::api::{NpcHealthRow, PlayerDeath, PvpFaction, RaidOverviewRow, TimeRange};
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::encounter_types::ChallengeSummary;
use crate::components::overview_chart::OverviewGroupChart;
use crate::components::overview_table::OverviewTable;
use baras_types::formatting;

pub(super) fn overview_view(ctx: ExplorerCtx, european: bool) -> Element {
    let ExplorerCtx {
        encounters,
        shown_encounter,
        overview_sort_col,
        overview_layouts,
        state,
        overview_sort_asc,
        overview_alive_view,
        overview_stacked,
        overview_show_deaths,
        overview_hovered,
        mut expanded_npc_groups,
        mut time_range,
        player_deaths,
        npc_health,
        mut recap_death,
        overview_table_data,
        overview_data,
        ..
    } = ctx;
    repaint_with(overview_data);
    let eu = european;
    let format_number = |n: f64| formatting::format_compact_f64(n, eu);

    rsx! {
           // Raid Overview - Donut Charts + Table
           // Uses memoized overview_table_data - charts initialized via use_effect above
           div { class: "overview-section",
               // Overview table - uses memoized data
               {
                   let table_data = overview_table_data.read();
                   let deaths = player_deaths.read().clone();
                   // Same death again closes the recap; another one switches to it
                   let open_death = move |d: PlayerDeath| {
                       let open = recap_death.peek().as_ref() == Some(&d);
                       recap_death.set((!open).then_some(d));
                   };
                   let columns = overview_layouts.read().columns(table_data.pvp);
                   rsx! {
                       // PvP encounters split into friendly/enemy tables; PvE gets one
                       {
                           if table_data.pvp {
                               let side_rows = |f: Option<PvpFaction>| -> Vec<RaidOverviewRow> {
                                   table_data.rows.iter().filter(|r| r.faction == f).cloned().collect()
                               };
                               let unknown = side_rows(None);
                               rsx! {
                                   OverviewTable {
                                       side_label: Some(("friendly", "Friendly")),
                                       rows: side_rows(Some(PvpFaction::Friendly)),
                                       total_label: "Friendly Total",
                                       columns: columns.clone(),
                                       sort_col: overview_sort_col,
                                       sort_asc: overview_sort_asc,
                                       european: eu,
                                       deaths: deaths.clone(),
                                       on_death_click: open_death,
                                       hovered: Some(overview_hovered),
    layout_state: Some(state),
    pvp: table_data.pvp,
                                   }
                                   OverviewTable {
                                       side_label: Some(("enemy", "Enemy")),
                                       rows: side_rows(Some(PvpFaction::Enemy)),
                                       total_label: "Enemy Total",
                                       columns: columns.clone(),
                                       sort_col: overview_sort_col,
                                       sort_asc: overview_sort_asc,
                                       european: eu,
                                       deaths: deaths.clone(),
                                       on_death_click: open_death,
                                   }
                                   if !unknown.is_empty() {
                                       OverviewTable {
                                           side_label: Some(("unknown", "Unclassified")),
                                           rows: unknown,
                                           columns: columns.clone(),
                                           sort_col: overview_sort_col,
                                           sort_asc: overview_sort_asc,
                                           european: eu,
                                           deaths: deaths.clone(),
                                           on_death_click: open_death,
                                       }
                                   }
                               }
                           } else {
                               rsx! {
                                   OverviewTable {
                                       rows: table_data.rows.clone(),
                                       total_label: "Group Total",
                                       columns: columns.clone(),
                                       sort_col: overview_sort_col,
                                       sort_asc: overview_sort_asc,
                                       european: eu,
                                       deaths: deaths.clone(),
                                       on_death_click: open_death,
                                       hovered: Some(overview_hovered),
    layout_state: Some(state),
    pvp: table_data.pvp,
                                   }
                               }
                           }
                       }

                       // Group chart: PvP plots the friendly side only (shares across sides are meaningless)
                       OverviewGroupChart {
                           encounter_idx: shown_encounter(),
                           time_range: time_range(),
                           rows: table_data.rows.iter().filter(|r| r.faction != Some(PvpFaction::Enemy)).cloned().collect::<Vec<_>>(),
                           sort_col: overview_sort_col,
                           sort_asc: overview_sort_asc,
                           hovered: overview_hovered,
                           alive_view: overview_alive_view,
                           stacked: overview_stacked,
                           show_deaths: overview_show_deaths,
                           european: eu,
                           deaths: player_deaths.read().clone(),
                           on_time_range_change: move |r: TimeRange| time_range.set(r),
                       }

                       // Side-by-side: Deaths + NPC Health + Challenges
                       {
                           let npcs = npc_health.read();
                           // The encounter's challenges and each player's (role, class, discipline) icons
                           let (challenges, player_icons): (Vec<ChallengeSummary>, HashMap<String, (Option<String>, Option<String>, String)>) =
                               shown_encounter()
                                   .and_then(|enc_idx| {
                                       encounters().iter().find(|e| e.encounter_id as u32 == enc_idx).map(|e| (
                                           e.challenges.clone(),
                                           e.player_metrics.iter()
                                               .map(|p| (p.name.clone(), (p.role_icon.clone(), p.class_icon.clone(), p.discipline_name.clone().unwrap_or_default())))
                                               .collect(),
                                       ))
                                   })
                                   .unwrap_or_default();
                           let has_npcs = !npcs.is_empty();
                           let has_challenges = !challenges.is_empty();

                           // Group NPCs by name
                           let mut groups: Vec<(String, Vec<&NpcHealthRow>)> = {
                               let mut map: std::collections::BTreeMap<String, Vec<&NpcHealthRow>> = std::collections::BTreeMap::new();
                               for npc in npcs.iter() {
                                   map.entry(npc.name.clone()).or_default().push(npc);
                               }
                               map.into_iter()
                                   .map(|(name, mut members)| {
                                       members.sort_by(|a, b| a.first_seen_secs.total_cmp(&b.first_seen_secs));
                                       (name, members)
                                   })
                                   .collect()
                           };

                           // Sort: unique NPCs first (by max_hp desc), then grouped NPCs (by max_hp desc)
                           groups.sort_by(|a, b| {
                               let a_unique = a.1.len() == 1;
                               let b_unique = b.1.len() == 1;
                               match (a_unique, b_unique) {
                                   (true, false) => std::cmp::Ordering::Less,
                                   (false, true) => std::cmp::Ordering::Greater,
                                   _ => {
                                       let a_max = a.1.iter().map(|n| n.max_hp).max().unwrap_or(0);
                                       let b_max = b.1.iter().map(|n| n.max_hp).max().unwrap_or(0);
                                       b_max.cmp(&a_max)
                                   }
                               }
                           });

                           let npc_count: usize = groups.iter().map(|(_, v)| v.len()).sum();
                           let expanded = expanded_npc_groups.read();
                           let format_compact = |n: i64| formatting::format_compact(n, eu);

                           rsx! {
                               if has_npcs || has_challenges {
                                   div { class: "overview-columns",
                                       // ─── NPC Health column ───────────────────
                                       if has_npcs {
                                           div { class: "overview-column",
                                               div { class: "overview-column-header",
                                                   i { class: "fa-solid fa-heart-pulse" }
                                                   " NPC Health"
                                                   span { class: "text-muted", " ({npc_count})" }
                                               }
                                               div { class: "npc-health-box",
                                                   table { class: "npc-health-table",
                                                       thead {
                                                           tr {
                                                               th { "Name" }
                                                               th { class: "num", "HP" }
                                                               th { class: "num", "Max" }
                                                               th { class: "num", "%" }
                                                           }
                                                       }
                                                       tbody {
                                                           for (name, members) in groups.iter() {
                                                               if members.len() == 1 {
                                                                   {
                                                                       let npc = members[0];
                                                                       let is_dead = npc.final_hp == 0;
                                                                       let hp_class = if is_dead { "dead" } else { "alive" };
                                                                       let seen_str = formatting::format_duration(npc.first_seen_secs as i64);
                                                                       let death_str = npc.death_time_secs.map(|t| formatting::format_duration(t as i64));
                                                                       rsx! {
                                                                           tr { class: "npc-row {hp_class}",
                                                                               td {
                                                                                   span { class: "npc-name", "{npc.name}" }
                                                                                   span { class: "npc-seen-time", " @{seen_str}" }
                                                                                   if let Some(ref dt) = death_str {
                                                                                       span { class: "npc-death-time",
                                                                                           i { class: "fa-solid fa-skull" }
                                                                                           " {dt}"
                                                                                       }
                                                                                   }
                                                                               }
                                                                               td { class: "num",
                                                                                   if is_dead {
                                                                                       span { class: "npc-hp-dead",
                                                                                           i { class: "fa-solid fa-skull" }
                                                                                           " Dead"
                                                                                       }
                                                                                   } else {
                                                                                       "{format_number(npc.final_hp as f64)}"
                                                                                   }
                                                                               }
                                                                               td { class: "num", "{format_number(npc.max_hp as f64)}" }
                                                                               td { class: "num",
                                                                                   if is_dead {
                                                                                       span { class: "npc-hp-dead", "0.0%" }
                                                                                   } else {
                                                                                       "{formatting::format_pct_f32(npc.final_hp_pct, eu)}"
                                                                                   }
                                                                               }
                                                                           }
                                                                       }
                                                                   }
                                                               } else {
                                                                   {
                                                                       let is_expanded = expanded.contains(name);
                                                                       let dead_count = members.iter().filter(|n| n.final_hp == 0).count();
                                                                       let alive_count = members.len() - dead_count;
                                                                       let max_hp = members.iter().map(|n| n.max_hp).max().unwrap_or(0);
                                                                       let all_dead = alive_count == 0;
                                                                       let group_hp_class = if all_dead { "dead" } else { "alive" };
                                                                       let chevron_class = if is_expanded { "fa-solid fa-chevron-down" } else { "fa-solid fa-chevron-right" };
                                                                       let toggle_name = name.clone();
                                                                       rsx! {
                                                                           tr {
                                                                               class: "npc-group-header {group_hp_class}",
                                                                               onclick: move |_| {
                                                                                   let mut set = expanded_npc_groups.write();
                                                                                   if set.contains(&toggle_name) {
                                                                                       set.remove(&toggle_name);
                                                                                   } else {
                                                                                       set.insert(toggle_name.clone());
                                                                                   }
                                                                               },
                                                                               td { colspan: "4",
                                                                                   i { class: "npc-group-chevron {chevron_class}" }
                                                                                   span { class: "npc-name", "{name}" }
                                                                                   span { class: "npc-group-count",
                                                                                       if all_dead {
                                                                                           " ({dead_count} dead)"
                                                                                       } else {
                                                                                           " ({alive_count} alive, {dead_count} dead)"
                                                                                       }
                                                                                   }
                                                                                   span { class: "npc-group-max", " \u{2014} {format_number(max_hp as f64)}" }
                                                                               }
                                                                           }
                                                                           if is_expanded {
                                                                               for npc in members.iter() {
                                                                                   {
                                                                                       let is_dead = npc.final_hp == 0;
                                                                                       let hp_class = if is_dead { "dead" } else { "alive" };
                                                                                       let seen_str = formatting::format_duration(npc.first_seen_secs as i64);
                                                                                       let death_str = npc.death_time_secs.map(|t| formatting::format_duration(t as i64));
                                                                                       rsx! {
                                                                                           tr { class: "npc-row npc-sub-row {hp_class}",
                                                                                               td {
                                                                                                   span { class: "npc-seen-time", "@{seen_str}" }
                                                                                                   if let Some(ref dt) = death_str {
                                                                                                       span { class: "npc-death-time",
                                                                                                           i { class: "fa-solid fa-skull" }
                                                                                                           " {dt}"
                                                                                                       }
                                                                                                   }
                                                                                               }
                                                                                               td { class: "num",
                                                                                                   if is_dead {
                                                                                                       span { class: "npc-hp-dead",
                                                                                                           i { class: "fa-solid fa-skull" }
                                                                                                           " Dead"
                                                                                                       }
                                                                                                   } else {
                                                                                                       "{format_number(npc.final_hp as f64)}"
                                                                                                   }
                                                                                               }
                                                                                               td { class: "num", "{format_number(npc.max_hp as f64)}" }
                                                                                               td { class: "num",
                                                                                                   if is_dead {
                                                                                                       span { class: "npc-hp-dead", "0.0%" }
                                                                                                   } else {
                                                                                                       "{formatting::format_pct_f32(npc.final_hp_pct, eu)}"
                                                                                                   }
                                                                                               }
                                                                                           }
                                                                                       }
                                                                                   }
                                                                               }
                                                                           }
                                                                       }
                                                                   }
                                                               }
                                                           }
                                                       }
                                                   }
                                               }
                                           }
                                       }

                                       // ─── Challenges column ────────────────────
                                       if has_challenges {
                                           div { class: "overview-column",
                                               div { class: "overview-column-header",
                                                   i { class: "fa-solid fa-trophy" }
                                                   " Challenges"
                                               }
                                               div { class: "challenge-cards",
                                                   for challenge in challenges.iter() {
                                                       {
                                                           let duration_str = formatting::format_duration(challenge.duration_secs as i64);
                                                           let per_sec_str = challenge.per_second
                                                               .map(|ps| format_compact(ps as i64))
                                                               .unwrap_or_default();
                                                           let columns = challenge.columns.as_str();
                                                           let show_total = matches!(columns, "TotalPercent" | "TotalPerSecond" | "TotalOnly");
                                                           let show_ps = matches!(columns, "PerSecondPercent" | "TotalPerSecond" | "PerSecondOnly");
                                                           let show_pct = matches!(columns, "TotalPercent" | "PerSecondPercent" | "PercentOnly");
                                                           let bar_color = challenge.color
                                                               .map(|c| format!("rgba({}, {}, {}, 0.15)", c[0], c[1], c[2]))
                                                               .unwrap_or_else(|| "rgba(74, 144, 217, 0.15)".to_string());

                                                           rsx! {
                                                               div { class: "challenge-card",
                                                                   div { class: "challenge-card-header",
                                                                       span { class: "challenge-name", "{challenge.name}" }
                                                                       span { class: "challenge-total",
                                                                           "{format_compact(challenge.total_value)}"
                                                                           if !per_sec_str.is_empty() {
                                                                               span { class: "text-muted", " ({per_sec_str}/s)" }
                                                                           }
                                                                       }
                                                                       span { class: "challenge-duration text-muted", "{duration_str}" }
                                                                   }
                                                                   if !challenge.by_player.is_empty() {
                                                                       div { class: "challenge-players",
                                                                           // Column headers
                                                                           div { class: "challenge-player-header",
                                                                               span { class: "challenge-player-name", "Player" }
                                                                               if show_ps {
                                                                                   span { class: "challenge-player-ps", "/s" }
                                                                               }
                                                                               if show_total {
                                                                                   span { class: "challenge-player-value", "Total" }
                                                                               }
                                                                               if show_pct {
                                                                                   span { class: "challenge-player-pct", "%" }
                                                                               }
                                                                           }
                                                                           for player in challenge.by_player.iter() {
                                                                               {
                                                                                   let ps_str = player.per_second
                                                                                       .map(|ps| format_compact(ps as i64))
                                                                                       .unwrap_or_default();
                                                                                   rsx! {
                                                                                       div { class: "challenge-player-row",
                                                                                           div {
                                                                                               class: "challenge-bar-fill",
                                                                                               style: "width: {player.percent:.1}%; background: {bar_color}",
                                                                                           }
                                                                                           span { class: "challenge-player-name",
                                                                                               if let Some((role, class, discipline)) = player_icons.get(&player.name) {
                                                                                                   if let Some(asset) = role.as_deref().and_then(get_role_icon) {
                                                                                                       img { class: "role-icon", src: *asset, alt: "" }
                                                                                                   }
                                                                                                   if let Some(asset) = class.as_deref().and_then(get_class_icon) {
                                                                                                       img { class: "class-icon", src: *asset, title: "{discipline}", alt: "" }
                                                                                                   }
                                                                                               }
                                                                                               "{player.name}"
                                                                                           }
                                                                                           if show_ps && !ps_str.is_empty() {
                                                                                               span { class: "challenge-player-ps", "{ps_str}/s" }
                                                                                           }
                                                                                           if show_total {
                                                                                               span { class: "challenge-player-value", "{format_compact(player.value)}" }
                                                                                           }
                                                                                           if show_pct {
                                                                                               span { class: "challenge-player-pct", "{formatting::format_pct_f32(player.percent, eu)}" }
                                                                                           }
                                                                                       }
                                                                                   }
                                                                               }
                                                                           }
                                                                       }
                                                                   }
                                                               }
                                                           }
                                                       }
                                                   }
                                               }
                                           }
                                       }
                                   }
                               }
                           }
                       }

                   }
               }
           }
       }
}
