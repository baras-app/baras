//! Score names and health readings, then assign at most one player per row.
//!
//! Ambiguous rows stay empty. A missed row is cheaper than a wrong one.

use super::alignment::align_score;
use super::candidates::PlayerCandidate;
use super::normalize::{identified_len, normalize_ocr, similarity};
use super::{MIN_NAME_CONFIDENCE, MIN_OCR_NAME_CHARS};

/// What OCR found in one raid-frame row.
#[derive(Debug, Clone, Default)]
pub struct RowObservation {
    /// Slot index this row corresponds to, as laid out by the raid overlay grid.
    pub row: usize,
    /// Raw recognized name text, in whatever form OCR produced it.
    pub name_text: Option<String>,
    /// Absolute health, e.g. `271245` read from `271,245`.
    pub hp_value: Option<u32>,
    /// Health percentage, e.g. `55` read from `(55%)`. Diagnostics only: it is
    /// 100 for nearly every healthy player, so it separates almost nobody, and
    /// a dropped digit turns `(100%)` into a confident `10`.
    pub hp_percent: Option<u8>,
}

/// A row confidently matched to a player.
#[derive(Debug, Clone)]
pub struct RowAssignment {
    pub row: usize,
    pub entity_id: i64,
    /// Exact log spelling, never the OCR output.
    pub name: String,
    pub confidence: f32,
}

/// Thresholds and signal weights.
#[derive(Debug, Clone)]
pub struct MatchConfig {
    /// Minimum name score for a row to be considered at all.
    pub min_confidence: f32,
    /// Minimum lead over the next-best candidate.
    pub min_margin: f32,
    pub name_weight: f32,
    pub hp_value_weight: f32,
    /// Weakest name health may still be counted for. `None` bars it entirely.
    pub health_rescue_floor: Option<f32>,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            min_confidence: MIN_NAME_CONFIDENCE,
            min_margin: 0.10,
            // Names and absolute health identify a player; see `hp_value_score`.
            name_weight: 1.0,
            hp_value_weight: 0.9,
            health_rescue_floor: None,
        }
    }
}

impl MatchConfig {
    /// For the retry. The blend still has to clear `min_confidence`, so this
    /// widens what health is considered for, not what gets assigned.
    pub fn with_health_rescue(self) -> Self {
        Self {
            health_rescue_floor: Some(HEALTH_RESCUE_FLOOR),
            ..self
        }
    }
}

/// Weakest name a second look at health may act on.
pub const HEALTH_RESCUE_FLOOR: f32 = 0.45;

/// Health below this score is too weak to be useful.
const STRONG_HEALTH: f32 = 0.75;

/// Score recognized name text against a normalized log name.
///
/// The clipping, junk, wildcard and misread cases all live in one alignment —
/// see [`align_score`]. `TESTCHARL` matches `TESTCHARLIELONG` on its first
/// nine characters, whether the tail was clipped by the frame or hidden
/// behind another UI element.
pub fn name_similarity(observed: &str, target: &str) -> Option<f32> {
    if target.is_empty() || identified_len(observed) < MIN_OCR_NAME_CHARS {
        return None;
    }
    if observed == target {
        return Some(1.0);
    }

    // The crop's left edge sometimes yields a stray glyph that normalization
    // keeps because it reads as a letter — `ISOA` for `SOA`. An exact match on
    // the remainder is certainty; the alignment only discounts it.
    if observed.len() > MIN_OCR_NAME_CHARS && &observed[1..] == target {
        return Some(1.0);
    }

    Some(align_score(observed, target))
}

fn name_score(observed: &str, candidate: &PlayerCandidate) -> Option<f32> {
    name_similarity(observed, &candidate.normalized)
}

/// Score a health reading against a candidate.
///
/// Compared as digit strings rather than numerically: a single misread digit
/// should cost a little, not push the value into a different order of magnitude.
/// Max health is checked as well as current, because out of combat the frame
/// shows exactly the max and because the log reading can lag the screen by a
/// fraction of a second.
fn hp_value_score(observed: u32, candidate: &PlayerCandidate) -> f32 {
    let observed = observed.to_string();

    let mut best: f32 = 0.0;
    if candidate.current_hp > 0 {
        best = best.max(similarity(&observed, &candidate.current_hp.to_string()));
    }
    if candidate.max_hp > 0 {
        // Slight discount: matching current health is the stronger claim.
        best = best.max(similarity(&observed, &candidate.max_hp.to_string()) * 0.95);
    }
    best
}

/// Why a row was left unassigned.
///
/// These want different things from the user: reading again may fix a weak
/// name, but never separates two players who look alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// Nothing legible in the frame.
    NoName,
    /// Read, but nobody on the roster is close enough.
    NoCandidate,
    /// Two or more players fit equally well. Needs a human.
    Ambiguous,
    /// Matched, but another row fit the same player better.
    TakenByBetterRow,
    /// No log roster to match against yet.
    NoRoster,
}

impl Rejection {
    pub fn reason(self) -> &'static str {
        match self {
            Rejection::NoName => "no readable name",
            Rejection::NoCandidate => "no candidate above min_confidence",
            Rejection::Ambiguous => "too close to the next-best candidate",
            Rejection::TakenByBetterRow => "every matching player went to a better row",
            Rejection::NoRoster => "no roster to match against",
        }
    }
}

/// One health signal's contribution to a score.
#[derive(Debug, Clone, Copy)]
pub struct Contribution {
    /// What the reading scored against the candidate.
    pub score: f32,
    /// Whether it was strong enough to be folded into the total.
    pub counted: bool,
}

/// How one row scored against one candidate.
#[derive(Debug, Clone)]
pub struct CandidateScore {
    pub entity_id: i64,
    pub name: String,
    pub total: f32,
    pub name_score: f32,
    /// Present when the row carried an absolute health reading and the
    /// candidate had health to compare it against.
    pub hp_value: Option<Contribution>,
}

/// Combined score for one row against one candidate, in `0.0..=1.0`,
/// keeping the pieces so they can be logged.
fn score_parts(
    observation: &RowObservation,
    normalized_name: Option<&str>,
    candidate: &PlayerCandidate,
    config: &MatchConfig,
) -> CandidateScore {
    let mut parts = CandidateScore {
        entity_id: candidate.entity_id,
        name: candidate.name.clone(),
        total: 0.0,
        name_score: 0.0,
        hp_value: None,
    };

    let Some(name) = normalized_name else {
        return parts;
    };
    let Some(name_score) = name_score(name, candidate) else {
        return parts;
    };
    parts.name_score = name_score;

    // Health is supporting evidence, never identity. It cannot rescue a name
    // that is too weak to stand on its own, unless a retry lowered the floor.
    if name_score < config.health_rescue_floor.unwrap_or(config.min_confidence) {
        return parts;
    }

    if config.name_weight <= 0.0 {
        parts.total = name_score;
        return parts;
    }
    let mut weighted = name_score * config.name_weight;
    let mut total_weight = config.name_weight;

    // Log health is an event-driven snapshot and may lag behind the frame. Only
    // agreement helps; disagreement and missing health say nothing.
    if let Some(hp) = observation.hp_value
        && (candidate.current_hp > 0 || candidate.max_hp > 0)
    {
        let s = hp_value_score(hp, candidate);
        let counted = s >= STRONG_HEALTH && config.hp_value_weight > 0.0;
        if counted {
            weighted += s * config.hp_value_weight;
            total_weight += config.hp_value_weight;
        }
        parts.hp_value = Some(Contribution { score: s, counted });
    }

    // Supporting evidence may improve a name score, but must never reduce it.
    parts.total = name_score.max(weighted / total_weight).clamp(0.0, 1.0);
    parts
}

/// What happened to one row. Diagnostics only — nothing reads this to decide.
#[derive(Debug, Clone)]
pub struct RowDecision {
    pub row: usize,
    /// The reading as OCR produced it.
    pub observed: Option<String>,
    /// The reading after normalization, when anything survived.
    pub normalized: Option<String>,
    /// Candidate the row was given.
    pub assigned: Option<CandidateScore>,
    /// Best candidate for this row alone, which is not always the one it was
    /// given — assignment is global.
    pub best: Option<CandidateScore>,
    /// Score of the next-best candidate behind `best`.
    pub runner_up: f32,
    /// Who that was. A lookalike rejection is about the pair.
    pub runner_up_name: Option<String>,
    /// Why the row went unassigned.
    pub rejected: Option<Rejection>,
}

/// Match raid-frame rows to log players.
///
/// Returns confident assignments in row order. Missing rows are left alone.
pub fn assign_rows(
    observations: &[RowObservation],
    candidates: &[PlayerCandidate],
    config: &MatchConfig,
) -> Vec<RowAssignment> {
    assign_rows_explained(observations, candidates, config).0
}

/// [`assign_rows`], with the reasoning behind every row.
pub fn assign_rows_explained(
    observations: &[RowObservation],
    candidates: &[PlayerCandidate],
    config: &MatchConfig,
) -> (Vec<RowAssignment>, Vec<RowDecision>) {
    if observations.is_empty() {
        return (Vec::new(), Vec::new());
    }

    // Normalize each reading once rather than per candidate. A read that is
    // nothing but wildcards carries no name at all.
    let normalized: Vec<Option<String>> = observations
        .iter()
        .map(|o| {
            o.name_text
                .as_deref()
                .map(normalize_ocr)
                .filter(|n| identified_len(n) > 0)
        })
        .collect();

    if candidates.is_empty() {
        let decisions = observations
            .iter()
            .zip(&normalized)
            .map(|(obs, norm)| RowDecision {
                row: obs.row,
                observed: obs.name_text.clone(),
                normalized: norm.clone(),
                assigned: None,
                best: None,
                runner_up: 0.0,
                runner_up_name: None,
                rejected: Some(Rejection::NoRoster),
            })
            .collect();
        return (Vec::new(), decisions);
    }

    // parts[row][candidate], with scores[row][candidate] derived from it.
    let parts: Vec<Vec<CandidateScore>> = observations
        .iter()
        .zip(&normalized)
        .map(|(obs, norm)| {
            candidates
                .iter()
                .map(|c| score_parts(obs, norm.as_deref(), c, config))
                .collect()
        })
        .collect();
    let scores: Vec<Vec<f32>> = parts.iter().map(|row| row.iter().map(|p| p.total).collect()).collect();

    let assigned = solve_assignment(&scores, config.min_confidence);

    // Do not let assignment by elimination turn two near-ties into two guesses.
    let mut out = Vec::new();
    let mut decisions = Vec::with_capacity(observations.len());

    for (row_idx, &candidate_idx) in assigned.iter().enumerate() {
        // Report the row's own best even when the solver gave it to someone
        // else, and fall back to the closest name when nothing scored at all —
        // "closest was X at 0.58" says more than "no match".
        let best_idx = best_by(&parts[row_idx], |p| p.total)
            .filter(|&i| parts[row_idx][i].total > 0.0)
            .or_else(|| best_by(&parts[row_idx], |p| p.name_score).filter(|&i| parts[row_idx][i].name_score > 0.0));
        let best = best_idx.map(|i| parts[row_idx][i].clone());
        // Named, not just scored: a tie needs both sides to be diagnosable.
        let runner_up_idx = best_idx.and_then(|best_idx| {
            scores[row_idx]
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != best_idx)
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(i, _)| i)
        });
        let runner_up = runner_up_idx.map_or(0.0, |i| scores[row_idx][i]);
        let runner_up_name = runner_up_idx.map(|i| parts[row_idx][i].name.clone());

        let mut decision = RowDecision {
            row: observations[row_idx].row,
            observed: observations[row_idx].name_text.clone(),
            normalized: normalized[row_idx].clone(),
            assigned: None,
            best,
            runner_up,
            runner_up_name,
            rejected: None,
        };

        let Some(candidate_idx) = candidate_idx else {
            decision.rejected = Some(if normalized[row_idx].is_none() {
                Rejection::NoName
            } else if decision.best.as_ref().is_some_and(|b| b.total > 0.0) {
                Rejection::TakenByBetterRow
            } else {
                Rejection::NoCandidate
            });
            decisions.push(decision);
            continue;
        };

        let score = scores[row_idx][candidate_idx];
        let margin_runner_up = scores[row_idx]
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != candidate_idx)
            .map(|(_, &s)| s)
            .fold(0.0f32, f32::max);

        if score - margin_runner_up < config.min_margin {
            decision.rejected = Some(Rejection::Ambiguous);
            decisions.push(decision);
            continue;
        }

        decision.assigned = Some(parts[row_idx][candidate_idx].clone());
        decisions.push(decision);

        out.push(RowAssignment {
            row: observations[row_idx].row,
            entity_id: candidates[candidate_idx].entity_id,
            name: candidates[candidate_idx].name.clone(),
            confidence: score,
        });
    }

    (out, decisions)
}

/// Index of the highest-scoring entry, by the given measure.
fn best_by(parts: &[CandidateScore], measure: impl Fn(&CandidateScore) -> f32) -> Option<usize> {
    parts
        .iter()
        .enumerate()
        .max_by(|a, b| {
            measure(a.1)
                .partial_cmp(&measure(b.1))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
}

/// Greedy assignment with pairwise score improvements.
fn solve_assignment(scores: &[Vec<f32>], floor: f32) -> Vec<Option<usize>> {
    let rows = scores.len();
    let cols = scores.first().map_or(0, |r| r.len());

    let mut pairs: Vec<(usize, usize, f32)> = Vec::with_capacity(rows * cols);
    for (r, row) in scores.iter().enumerate() {
        for (c, &s) in row.iter().enumerate() {
            if s >= floor {
                pairs.push((r, c, s));
            }
        }
    }
    pairs.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });

    let mut assigned: Vec<Option<usize>> = vec![None; rows];
    let mut taken = vec![false; cols];
    for (r, c, _) in pairs {
        if assigned[r].is_none() && !taken[c] {
            assigned[r] = Some(c);
            taken[c] = true;
        }
    }

    // Swap when the pair's total score improves.
    let mut improved = true;
    while improved {
        improved = false;
        for i in 0..rows {
            for j in (i + 1)..rows {
                let (Some(ci), Some(cj)) = (assigned[i], assigned[j]) else {
                    continue;
                };
                let current = scores[i][ci] + scores[j][cj];
                let swapped = scores[i][cj] + scores[j][ci];
                if swapped > current && scores[i][cj] >= floor && scores[j][ci] >= floor {
                    assigned[i] = Some(cj);
                    assigned[j] = Some(ci);
                    improved = true;
                }
            }
        }
    }

    assigned
}
