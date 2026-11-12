//! PvP Statistics: warzones and arenas kept apart, each with its match
//! calendar, record (arenas only: two round wins take the match; warzone
//! outcomes are not in the log), and boards for maps, your disciplines, and
//! the people you play with and against. One fetch of compact match rows;
//! everything here is derived from them locally.

use crate::components::multi_select::{MultiSelect, discipline_entries, plain_entries, toggle_value};
use baras_types::history::keeps;
use std::collections::HashMap;

use dioxus::prelude::*;

use baras_types::formatting;
use baras_types::history::{BackfillEvent, PvpMatchRow};

use super::Backfill;
use super::heatmap::{DayRange, HeatDay, Heatmap, day_of, in_range, level};
use super::overview::short_ts;
use super::scroll::use_paged;
use crate::api;
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::use_file_opener;

const TABLE_ID: &str = "hist-pvp-table";
/// Rows shown on the people boards
const PEOPLE: usize = 12;

/// Local filters within the chosen mode. Empty = any.
#[derive(Clone, PartialEq, Default)]
struct PvpFilter {
    characters: Vec<String>,
    maps: Vec<String>,
    /// Discipline names to keep (mirror pairs together); empty = any
    disciplines: Vec<String>,
    servers: Vec<String>,
    /// Only matches this player was in, on either side
    person: String,
}

impl PvpFilter {
    fn matches(&self, r: &PvpMatchRow) -> bool {
        keeps(&self.characters, &r.character)
            && keeps(&self.maps, &r.map)
            && (self.disciplines.is_empty() || r.discipline.as_ref().is_some_and(|d| self.disciplines.contains(d)))
            && keeps(&self.servers, &r.server)
            && (self.person.is_empty() || r.players.iter().any(|p| p.name == self.person))
    }

    fn active(&self) -> usize {
        [&self.characters, &self.maps, &self.disciplines, &self.servers].iter().filter(|v| !v.is_empty()).count()
            + usize::from(!self.person.is_empty())
    }
}

fn distinct<'a>(it: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut v: Vec<String> = it.filter(|s| !s.is_empty()).map(str::to_string).collect();
    v.sort();
    v.dedup();
    v
}

/// One board line: how often, and (arenas) the record
#[derive(Clone, PartialEq)]
struct Tally {
    name: String,
    /// Discipline icon file, on the disciplines board
    icon: Option<String>,
    matches: u32,
    wins: u32,
    losses: u32,
}

/// Count matches per key; a match contributes to every key `keys` yields
/// (a map once, every teammate on the field).
fn tally<'a>(rows: &'a [PvpMatchRow], keys: impl Fn(&'a PvpMatchRow) -> Vec<(&'a str, Option<&'a str>)>) -> Vec<Tally> {
    let mut out: Vec<Tally> = Vec::new();
    for r in rows {
        for (k, icon) in keys(r) {
            let t = match out.iter_mut().find(|t| t.name == k) {
                Some(t) => t,
                None => {
                    out.push(Tally { name: k.to_string(), icon: icon.map(str::to_string), matches: 0, wins: 0, losses: 0 });
                    out.last_mut().unwrap()
                }
            };
            t.matches += 1;
            t.wins += u32::from(r.won == Some(true));
            t.losses += u32::from(r.won == Some(false));
        }
    }
    out.sort_by(|a, b| b.matches.cmp(&a.matches).then_with(|| a.name.cmp(&b.name)));
    out
}

fn win_rate(wins: u32, losses: u32) -> Option<u32> {
    (wins + losses > 0).then(|| wins * 100 / (wins + losses))
}

/// Calendar cells: shade by matches that day; arena days tint by net result
fn heat_days(rows: &[PvpMatchRow], arena: bool) -> Vec<HeatDay> {
    let mut days: HashMap<i64, (u32, u32, u32)> = HashMap::new(); // matches, wins, losses
    for r in rows {
        let Some(day) = day_of(&r.timestamp) else { continue };
        let d = days.entry(day).or_default();
        d.0 += 1;
        d.1 += u32::from(r.won == Some(true));
        d.2 += u32::from(r.won == Some(false));
    }
    let max = days.values().map(|d| d.0).max().unwrap_or(1) as f64;
    days.into_iter()
        .map(|(day, (n, w, l))| HeatDay {
            day,
            level: level(f64::from(n), max),
            tone: match (arena, w.cmp(&l)) {
                (false, _) => "pvp",
                (true, std::cmp::Ordering::Greater) => "win",
                (true, std::cmp::Ordering::Less) => "loss",
                (true, std::cmp::Ordering::Equal) => "even",
            },
            title: if arena {
                format!("{n} match{} · {w}–{l}", if n == 1 { "" } else { "es" })
            } else {
                format!("{n} match{}", if n == 1 { "" } else { "es" })
            },
        })
        .collect()
}

/// Result cell: arenas carry the round score; warzones have no outcome
fn result(r: &PvpMatchRow) -> (String, &'static str) {
    if !r.arena {
        return ("—".into(), "pvp-no-result");
    }
    let (w, l) = r.score();
    match r.won {
        Some(true) => (format!("Win {w}–{l}"), "pull-result kill"),
        Some(false) => (format!("Loss {w}–{l}"), "pull-result wipe"),
        None => (format!("Undecided {w}–{l}"), "pull-result reset"),
    }
}

fn names<'a>(r: &'a PvpMatchRow, friendly: bool) -> Vec<&'a str> {
    r.players.iter().filter(|p| p.friendly == friendly).map(|p| p.name.as_str()).collect()
}

fn keyed<'a>(names: Vec<&'a str>) -> Vec<(&'a str, Option<&'a str>)> {
    names.into_iter().map(|n| (n, None)).collect()
}

#[component]
pub fn PvpTab(european: bool, backfill: Backfill) -> Element {
    let opener = use_file_opener();
    let mut arena = use_signal(|| true);
    let mut filter = use_signal(PvpFilter::default);
    let day_range: Signal<Option<DayRange>> = use_signal(|| None);
    let paged = use_paged("pvp");

    let mut matches = use_resource(|| async move { api::get_pvp_matches().await.unwrap_or_default() });
    // A live match just ended, or a re-scan finished
    use_effect(move || {
        if matches!(*backfill.event.read(), Some(BackfillEvent::Done { .. })) || *backfill.history.read() > 0 {
            matches.restart();
        }
    });

    let all = matches.read().clone().unwrap_or_default();
    let loaded = matches.read().is_some();
    let is_arena = *arena.read();
    let f = filter.read().clone();
    let range = day_range.read().clone();
    // The mode scopes everything, filters narrow within it
    let pool: Vec<&PvpMatchRow> = all.iter().filter(|r| r.arena == is_arena).collect();
    let rows: Vec<PvpMatchRow> = pool
        .iter()
        .filter(|r| f.matches(r) && range.as_ref().is_none_or(|rg| in_range(&r.timestamp, rg)))
        .map(|r| (*r).clone())
        .collect();

    let characters = distinct(pool.iter().map(|r| r.character.as_str()));
    let maps = distinct(pool.iter().map(|r| r.map.as_str()));
    let disciplines = distinct(pool.iter().filter_map(|r| r.discipline.as_deref()));
    let servers = distinct(pool.iter().map(|r| r.server.as_str()));
    let (wz_count, arena_count) = all.iter().fold((0, 0), |(w, a), r| if r.arena { (w, a + 1) } else { (w + 1, a) });

    // Value boxes
    let total = rows.len();
    let wins = rows.iter().filter(|r| r.won == Some(true)).count() as u32;
    let losses = rows.iter().filter(|r| r.won == Some(false)).count() as u32;

    let map_board = tally(&rows, |r| vec![(r.map.as_str(), None)]);
    let disc_board = tally(&rows, |r| r.discipline.as_deref().map(|d| (d, r.discipline_icon.as_deref())).into_iter().collect());
    let mut mates = tally(&rows, |r| keyed(names(r, true)));
    let mut foes = tally(&rows, |r| keyed(names(r, false)));
    mates.truncate(PEOPLE);
    foes.truncate(PEOPLE);
    let record_rate = win_rate(wins, losses).map(|p| format!("{p} %")).unwrap_or_else(|| "—".into());

    let heat = heat_days(&rows, is_arena);
    // Literal branches inside rsx props become formatted Strings; these are &'static str
    let (heat_title, heat_legend) = if is_arena { ("Arena matches", "win") } else { ("Warzone matches", "pvp") };
    let limit = paged.limit("pvp");

    let mut set = move |apply: fn(&mut PvpFilter, String), v: String| {
        apply(&mut filter.write(), v);
        paged.show_all("");
    };
    // Multi-select change (toolbar list or board click); starts the table over
    let mut pick = move |field: fn(&mut PvpFilter) -> &mut Vec<String>, v: Vec<String>| {
        *field(&mut filter.write()) = v;
        paged.show_all("");
    };
    // Board clicks toggle one value; a discipline brings its mirror, as in the list
    // Compute in its own statement: a `filter.read()` temporary inside the call
    // arguments would outlive into `pick`'s `filter.write()` and panic
    let pick_map = move |name: String| {
        let v = toggle_value(&filter.read().maps, &name, false);
        pick(|f| &mut f.maps, v);
    };
    let pick_disc = move |name: String| {
        let v = toggle_value(&filter.read().disciplines, &name, true);
        pick(|f| &mut f.disciplines, v);
    };
    let pick_person = move |name: String| {
        let v = if filter.read().person == name { String::new() } else { name };
        set(|f, v| f.person = v, v);
    };

    rsx! {
        div { class: "pull-history-main pvp-main",
            div { class: "pvp-head",
                div { class: "data-tab-selector pvp-views",
                    button { class: if !is_arena { "data-tab active" } else { "data-tab" }, title: "8-player warzone matches",
                        onclick: move |_| { arena.set(false); filter.set(PvpFilter::default()); },
                        i { class: "fa-solid fa-flag" } " Warzones " span { class: "section-count", "{wz_count}" }
                    }
                    button { class: if is_arena { "data-tab active" } else { "data-tab" }, title: "4-player arena matches, best of three rounds",
                        onclick: move |_| { arena.set(true); filter.set(PvpFilter::default()); },
                        i { class: "fa-solid fa-khanda" } " Arenas " span { class: "section-count", "{arena_count}" }
                    }
                }
                div { class: "history-toolbar",
                    MultiSelect {
                        any_label: "All characters",
                        noun: "characters",
                        entries: plain_entries(&characters),
                        picked: f.characters.clone(),
                        on_change: move |v| pick(|f| &mut f.characters, v),
                    }
                    MultiSelect {
                        any_label: "Any map",
                        noun: "maps",
                        entries: plain_entries(&maps),
                        picked: f.maps.clone(),
                        on_change: move |v| pick(|f| &mut f.maps, v),
                    }
                    MultiSelect {
                        any_label: "Any discipline",
                        noun: "disciplines",
                        title: "Mirror disciplines count as one",
                        entries: discipline_entries(&disciplines),
                        picked: f.disciplines.clone(),
                        on_change: move |v| pick(|f| &mut f.disciplines, v),
                    }
                    if servers.len() > 1 || !f.servers.is_empty() {
                        MultiSelect {
                            any_label: "Any server",
                            noun: "servers",
                            entries: plain_entries(&servers),
                            picked: f.servers.clone(),
                            on_change: move |v| pick(|f| &mut f.servers, v),
                        }
                    }
                    if let Some((from, to)) = range.clone() {
                        span { class: "library-day-pick",
                            i { class: "fa-solid fa-calendar-day" }
                            if from == to { " {from}" } else { " {from} – {to}" }
                            button { class: "btn btn-xs btn-ghost", title: "Show every day again", onclick: move |_| { let mut d = day_range; d.set(None) }, i { class: "fa-solid fa-xmark" } }
                        }
                    }
                    if !f.person.is_empty() {
                        span { class: "library-day-pick", title: "Only matches with this player on either side",
                            i { class: "fa-solid fa-user" } " {f.person}"
                            button { class: "btn btn-xs btn-ghost", title: "Every player again", onclick: move |_| set(|f, v| f.person = v, String::new()), i { class: "fa-solid fa-xmark" } }
                        }
                    }
                    button {
                        class: if f.active() > 0 { "btn btn-xs history-clear active" } else { "btn btn-xs history-clear" },
                        disabled: f.active() == 0,
                        onclick: move |_| filter.set(PvpFilter::default()),
                        i { class: "fa-solid fa-filter-circle-xmark" } " Clear filters"
                    }
                    span { class: "pull-history-count", "{total} of {pool.len()} matches" }
                }
            }

            if loaded && all.is_empty() {
                div { class: "pvp-empty",
                    i { class: "fa-solid fa-crosshairs" }
                    p { "No PvP matches recorded yet." }
                    p { class: "hint", "Warzones and arenas are picked up from your logs the same way boss pulls are. Logs scanned before this view existed need one re-scan." }
                    button { class: "btn btn-sm", disabled: backfill.running(), onclick: move |_| backfill.start(true),
                        i { class: "fa-solid fa-rotate" } " Scan logs"
                    }
                }
            } else {
                div { class: "pvp-top",
                    div { class: "history-cards pvp-stats",
                        div { class: "history-card", div { class: "k", "Matches" } div { class: "v", "{total}" } }
                        if is_arena {
                            div { class: "history-card", title: "Two round wins take the match; matches the rounds did not decide count for neither side",
                                div { class: "k", "Record" }
                                div { class: "v", "{wins}–{losses}" }
                            }
                            div { class: "history-card", title: "Wins over decided matches",
                                div { class: "k", "Win rate" }
                                div { class: "v", "{record_rate}" }
                            }
                        } else {
                            div { class: "history-card", title: "Distinct maps played",
                                div { class: "k", "Maps" }
                                div { class: "v", "{map_board.len()}" }
                            }
                        }
                    }
                    Heatmap { days: heat, selected: day_range, title: heat_title, hint: "", legend: heat_legend }
                }

                div { class: "pvp-boards",
                    Board { title: "Maps", icon: "fa-solid fa-map", rows: map_board.clone(), record: is_arena, active: f.maps.clone(), on_pick: pick_map }
                    Board { title: "Your disciplines", icon: "fa-solid fa-user-shield", rows: disc_board.clone(), record: is_arena, active: f.disciplines.clone(), on_pick: pick_disc }
                    Board { title: "Played with most", icon: "fa-solid fa-people-group", rows: mates, record: is_arena, active: vec![f.person.clone()], on_pick: pick_person }
                    Board { title: "Played against most", icon: "fa-solid fa-skull-crossbones", rows: foes, record: is_arena, active: vec![f.person.clone()], on_pick: pick_person }
                }

                div { id: TABLE_ID, class: "pull-history-table-wrap", onscroll: move |_| paged.on_scroll(TABLE_ID, "pvp", total),
                    table { class: "pull-history-table",
                        thead {
                            tr {
                                th { "Date" }
                                th { "Map" }
                                if is_arena { th { "Result" } }
                                th { "Duration" }
                                th { "Character" }
                                th { "Discipline" }
                                th { "With" }
                                th { "Against" }
                                th {}
                            }
                        }
                        tbody {
                            for r in rows.iter().take(limit) {
                                {
                                    let (label, class) = result(r);
                                    let path = r.path.clone();
                                    let encounter_id = r.encounter_id;
                                    let with = names(r, true).join(", ");
                                    let against = names(r, false).join(", ");
                                    rsx! {
                                        tr { key: "{r.filename}:{r.encounter_id}",
                                            td { {short_ts(&r.timestamp)} }
                                            td { "{r.map}" }
                                            if is_arena { td { span { class: "{class}", "{label}" } } }
                                            td { {formatting::format_duration(r.duration_seconds)} }
                                            td { "{r.character}" }
                                            td {
                                                span { class: "pull-discipline",
                                                    if let Some(icon) = r.role_icon.as_deref().and_then(get_role_icon) {
                                                        img { class: "role-icon", src: *icon, alt: "" }
                                                    }
                                                    if let Some(icon) = r.discipline_icon.as_deref().and_then(get_class_icon) {
                                                        img { class: "discipline-icon", src: *icon, alt: "" }
                                                    }
                                                    {r.discipline.clone().unwrap_or_else(|| "-".into())}
                                                }
                                            }
                                            td { class: "people", title: "{with}", "{with}" }
                                            td { class: "people", title: "{against}", "{against}" }
                                            td {
                                                if let Some(path) = path {
                                                    button {
                                                        class: "btn btn-xs btn-ghost",
                                                        title: "Open in Analysis",
                                                        disabled: opener.busy(),
                                                        onclick: move |_| opener.open(path.clone(), Some(encounter_id)),
                                                        i { class: "fa-solid fa-arrow-up-right-from-square" }
                                                    }
                                                } else {
                                                    span { class: "pull-log-missing", title: "Log file no longer on disk", i { class: "fa-solid fa-file-circle-xmark" } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if limit < total {
                        div { class: "history-more",
                            span { class: "history-more-count", "Showing {limit} of {total}" }
                            button { class: "btn btn-xs", onclick: move |_| paged.show_all("pvp"), "Show all" }
                        }
                    }
                }
            }
        }
    }
}

/// Name, matches, and (arenas) W–L with the rate. With `on_pick`, a row
/// toggles that name as a filter.
#[component]
fn Board(
    title: &'static str,
    icon: &'static str,
    rows: Vec<Tally>,
    record: bool,
    /// Names shown as picked
    #[props(default)] active: Vec<String>,
    #[props(default)] on_pick: Option<EventHandler<String>>,
) -> Element {
    rsx! {
        div { class: "pvp-board",
            h4 { i { class: "{icon}" } "{title}" }
            if rows.is_empty() {
                div { class: "empty", "Nothing here yet" }
            } else {
                table {
                    tbody {
                        for t in rows.iter() {
                            {
                                let name = t.name.clone();
                                let pickable = on_pick.is_some();
                                let is_active = pickable && active.contains(&t.name);
                                let rate = win_rate(t.wins, t.losses);
                                rsx! {
                                    tr {
                                        key: "{t.name}",
                                        class: format!("{}{}", if pickable { "pick" } else { "" }, if is_active { " active" } else { "" }),
                                        onclick: move |_| if let Some(h) = &on_pick { h.call(name.clone()) },
                                        td { class: "name", title: "{t.name}",
                                            if let Some(icon) = t.icon.as_deref().and_then(get_class_icon) {
                                                img { class: "discipline-icon", src: *icon, alt: "" }
                                            }
                                            "{t.name}"
                                        }
                                        td { class: "num", "{t.matches}" }
                                        if record {
                                            td { class: "num", "{t.wins}–{t.losses}" }
                                            td { class: "num",
                                                if let Some(p) = rate {
                                                    span { class: if p >= 50 { "pvp-rate up" } else { "pvp-rate down" }, "{p} %" }
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
