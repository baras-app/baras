use crate::combat_event::*;
use crate::log_ids::{effect_id, effect_type_id};
use memchr::memchr;
use memchr::memchr_iter;
use time::Time;

#[cfg(test)]
mod tests;

macro_rules! parse_i64 {
    ($s:expr) => {
        $s.parse::<i64>().unwrap_or_default()
    };
}
macro_rules! parse_i32 {
    ($s:expr) => {
        $s.parse::<i32>().unwrap_or_default()
    };
}

pub fn parse_line(line_number: u64, _line: &str) -> Option<CombatEvent> {
    let b = _line.as_bytes();
    let brackets: Vec<usize> = memchr_iter(b'[', b).collect();
    let end_brackets: Vec<usize> = memchr_iter(b']', b).collect();

    let time_segment = &_line[brackets[0] + 1..end_brackets[0]];
    let source_entity_segment = &_line[brackets[1] + 1..end_brackets[1]];
    let target_entity_segment = &_line[brackets[2] + 1..end_brackets[2]];
    let action_segment = &_line[brackets[3] + 1..end_brackets[3]];
    let effect_segment = &_line[brackets[4] + 1..end_brackets[4]];
    let details_segment = &_line[end_brackets[4] + 1..];

    let timestamp = parse_timestamp(time_segment)?;
    let source_entity = parse_entity(source_entity_segment)?;
    let target_entity = parse_entity(target_entity_segment)?;
    let action = parse_action(action_segment)?;

    let target_entity = if target_entity.entity_type == EntityType::SelfReference {
        source_entity.clone()
    } else {
        target_entity
    };

    let effect = parse_effect(effect_segment)?;
    let details = parse_details(details_segment, effect.effect_id, effect.type_id)?;

    let event = CombatEvent {
        line_number,
        timestamp,
        source_entity,
        target_entity,
        action,
        effect,
        details,
    };

    Some(event)
}

// parse HH:MM:SS.mmm
fn parse_timestamp(segment: &str) -> Option<Time> {
    let b = segment.as_bytes();
    if b.len() != 12 || b[2] != b':' || b[5] != b':' || b[8] != b'.' {
        return None;
    }

    let hour = (b[0] - b'0') * 10 + (b[1] - b'0');
    let minute = (b[3] - b'0') * 10 + (b[4] - b'0');
    let second = (b[6] - b'0') * 10 + (b[7] - b'0');
    let millis = (b[9] - b'0') as u16 * 100 + (b[10] - b'0') as u16 * 10 + (b[11] - b'0') as u16;

    Time::from_hms_milli(hour, minute, second, millis).ok()
}

fn parse_entity(segment: &str) -> Option<Entity> {
    let bytes = segment.as_bytes();
    let self_target_pos = memchr(b'=', bytes);

    // handle [=]
    if self_target_pos.is_some() {
        return Some(Entity {
            entity_type: EntityType::SelfReference,
            ..Default::default()
        });
    }
    //handle []
    if segment.is_empty() {
        return Some(Entity {
            ..Default::default()
        });
    }

    let pipes: Vec<usize> = memchr_iter(b'|', bytes).collect();
    let name_segment = &segment[..pipes[0]];
    // let _ = &segment[pipes[0] + 1..pipes[1]]; // coordinates ignore for now not used
    let health_segment = &segment[pipes[1]..];

    let (name, class_id, log_id, entity_type) = parse_entity_name_id(name_segment)?;
    let health = parse_entity_health(health_segment)?;

    Some(Entity {
        name: name.to_string(),
        class_id,
        log_id,
        entity_type,
        health,
    })
}

fn parse_entity_health(segment: &str) -> Option<(i32, i32)> {
    let bytes = segment.as_bytes();
    let paren = memchr(b'(', bytes);
    let slash = memchr(b'/', bytes);
    let paren_end = memchr(b')', bytes);

    let current_health = parse_i32!(&segment[paren? + 1..slash?]);
    let health_end_pos = parse_i32!(&segment[slash? + 1..paren_end?]);

    Some((current_health, health_end_pos))
}

fn parse_entity_name_id(segment: &str) -> Option<(&str, i64, i64, EntityType)> {
    let bytes = segment.as_bytes();

    let brace = memchr(b'{', bytes);
    let end_brace = memchr(b'}', bytes);
    let hashtag = memchr(b'#', bytes);
    let slash = memchr(b'/', bytes);

    // Parse Player and Player Companion
    if hashtag.is_some() {
        let player_name = &segment[1..hashtag?];

        if slash.is_none() {
            let player_id = parse_i64!(&segment[hashtag? + 1..]);

            return Some((player_name, 0, player_id, EntityType::Player));
        } else {
            let companion_name = &segment[slash? + 1..brace? - 1];
            let companion_char_id = parse_i64!(&segment[brace? + 1..end_brace?]);
            let companion_log_id = parse_i64!(&&segment[end_brace? + 2..]);

            return Some((
                companion_name,
                companion_char_id,
                companion_log_id,
                EntityType::Companion,
            ));
        }
    }

    // if no '#' detected parse NPC
    let npc_name = segment[..brace?].trim();
    let npc_char_id = parse_i64!(&segment[brace? + 1..end_brace?]);
    let npc_log_id = parse_i64!(&segment[end_brace? + 2..]);

    Some((npc_name, npc_char_id, npc_log_id, EntityType::Npc))
}

fn parse_action(segment: &str) -> Option<Action> {
    let bytes = segment.as_bytes();

    let brace = memchr(b'{', bytes);
    let end_brace = memchr(b'}', bytes);

    if segment.is_empty() {
        return Some(Action {
            ..Default::default()
        });
    }

    let action_name = segment[..end_brace?].trim().to_string();
    let action_id = parse_i64!(segment[brace? + 1..end_brace?]);

    Some(Action {
        name: action_name,
        action_id,
    })
}

fn parse_effect(segment: &str) -> Option<Effect> {
    let bytes = segment.as_bytes();
    let braces: Vec<usize> = memchr_iter(b'{', bytes).collect();
    let end_braces: Vec<usize> = memchr_iter(b'}', bytes).collect();
    let slash = memchr(b'/', bytes);
    if braces.len() < 2 || end_braces.len() < 2 {
        return Some(Effect {
            ..Default::default()
        });
    }

    let type_name = segment[..braces[0]].trim().to_string();
    let type_id = parse_i64!(&segment[braces[0] + 1..end_braces[0]]);
    let effect_name = segment[end_braces[0] + 2..braces[1] - 1].trim().to_string();
    let effect_id = parse_i64!(&segment[braces[1] + 1..end_braces[1]]);

    let (difficulty_name, difficulty_id) =
        if type_id == effect_type_id::AREAENTERED && braces.len() == 3 {
            (
                segment[end_braces[1] + 1..braces[2]].trim().to_string(),
                parse_i64!(segment[braces[2] + 1..end_braces[2]]),
            )
        } else {
            (String::new(), 0)
        };

    let (discipline_name, discipline_id) = if type_id == effect_type_id::DISCIPLINECHANGED {
        (
            segment[slash? + 1..braces[2]].trim().to_string(),
            parse_i64!(segment[braces[2] + 1..end_braces[2]]),
        )
    } else {
        (String::new(), 0)
    };

    Some(Effect {
        type_name,
        type_id,
        effect_name,
        effect_id,
        difficulty_name,
        difficulty_id,
        discipline_name,
        discipline_id,
    })
}

fn parse_details(segment: &str, effect_id: i64, effect_type_id: i64) -> Option<Details> {
    match effect_id {
        effect_id::DAMAGE => parse_dmg_details(segment),
        effect_id::HEAL => parse_heal_details(segment),
        _ => {
            if (effect_type_id == effect_type_id::APPLYEFFECT
                || effect_type_id == effect_type_id::MODIFYCHARGES)
                && memchr(b'(', segment.as_bytes()).is_some()
            {
                parse_charges(segment)
            } else {
                Some(Details {
                    ..Default::default()
                })
            }
        }
    }
}

fn parse_dmg_details(segment: &str) -> Option<Details> {
    let bytes = segment.as_bytes();

    // Find main delimiters
    let paren = memchr(b'(', bytes)?;
    let paren_end = rfind_matching_paren(bytes, paren)?;
    let angle = memchr(b'<', bytes);
    let angle_end = memchr(b'>', bytes);

    let inner = &segment[paren + 1..paren_end];
    let inner_bytes = inner.as_bytes();

    // Parse threat from <value>
    let threat = angle
        .zip(angle_end)
        .and_then(|(s, e)| segment[s + 1..e].parse::<f32>().ok())
        .unwrap_or_default();

    // Handle edge case: (0 -) - nullified damage from reflect
    if inner.trim() == "0 -" {
        return Some(Details {
            dmg_amount: 0,
            avoid_type: String::from("reflected"),
            is_reflect: true,
            threat,
            ..Default::default()
        });
    }

    // Check for crit marker
    let is_crit = memchr(b'*', inner_bytes).is_some();

    // Check for reflected marker
    let is_reflect = inner.contains("reflected");

    // Check for avoidance (-miss, -dodge, -parry, -immune, -resist, -deflect, -shield, -)
    let dash = memchr(b'-', inner_bytes);
    let avoid_type = if dash.is_some() {
        dash.map(|pos| {
            let start = pos + 1;
            let end = inner[start..]
                .find(|c: char| c.is_whitespace())
                .map(|e| start + e)
                .unwrap_or_default();
            if end != 0 {
                inner[start..end].trim().to_string()
            } else {
                String::new()
            }
        })
        .unwrap_or_default()
    } else {
        String::new()
    };

    // Parse amount (first number)
    let amount_end = inner
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(inner.len());
    let dmg_amount = parse_i32!(&inner[..amount_end]);

    // Parse effective damage after ~
    let tilde = memchr(b'~', inner_bytes);
    let dmg_effective = tilde
        .map(|pos| {
            let start = pos + 1;
            let end = inner[start..]
                .find(|c: char| !c.is_ascii_digit())
                .map(|e| start + e)
                .unwrap_or(inner.len());
            parse_i32!(&inner[start..end])
        })
        .unwrap_or(dmg_amount);
    // Find damage type and ID (first { } pair in inner, but not "reflected" or "absorbed")
    let brace = memchr(b'{', inner_bytes);
    let brace_end = memchr(b'}', inner_bytes);

    let (dmg_type, dmg_type_id) = if let (Some(bs), Some(be)) = (brace, brace_end) {
        // Find type name before the brace - scan backwards for a word
        let type_start = inner[..bs]
            .trim_end()
            .rfind(|c: char| c.is_whitespace())
            .map(|p| p + 1)
            .unwrap_or(0);
        let dmg_type = inner[type_start..bs].trim().to_string();
        let dmg_type_id = parse_i64!(&inner[bs + 1..be]);
        if dmg_type.contains('-') {
            (String::new(), 0)
        } else {
            (dmg_type, dmg_type_id)
        }
    } else {
        (String::new(), 0)
    };

    // Parse absorbed amount from nested (X absorbed {id})
    let dmg_absorbed = if let Some(absorbed_pos) = inner.find("absorbed") {
        let before_absorbed = &inner[..absorbed_pos];
        if let Some(nested_paren) = before_absorbed.rfind('(') {
            let absorbed_str = &inner[nested_paren + 1..absorbed_pos].trim();
            Some(parse_i32!(absorbed_str))
        } else {
            Some(0)
        }
    } else {
        None
    }
    .unwrap_or(0);

    Some(Details {
        dmg_amount,
        is_crit,
        is_reflect,
        dmg_effective,
        dmg_type,
        dmg_type_id,
        avoid_type,
        dmg_absorbed,
        threat,
        ..Default::default()
    })
}
/// Find matching closing paren, handling nested parens
fn rfind_matching_paren(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, &b) in bytes[start..].iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_heal_details(segment: &str) -> Option<Details> {
    let bytes = segment.as_bytes();

    // Find main delimiters
    let paren = memchr(b'(', bytes)?;
    let paren_end = memchr(b')', bytes)?;
    let angle = memchr(b'<', bytes);
    let angle_end = memchr(b'>', bytes);

    let inner = &segment[paren + 1..paren_end];
    let inner_bytes = inner.as_bytes();

    // Parse threat from <value> - only present if effective heal occurred
    let threat = angle
        .zip(angle_end)
        .and_then(|(s, e)| segment[s + 1..e].parse::<f32>().ok())
        .unwrap_or_default();

    // Check for crit marker
    let is_crit = memchr(b'*', inner_bytes).is_some();

    // Parse heal amount (first number)
    let amount_end = inner
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(inner.len());
    let heal_amount = parse_i32!(&inner[..amount_end]);

    // Parse effective heal after ~, default to heal_amount if not present
    let tilde = memchr(b'~', inner_bytes);
    let heal_effective = tilde
        .map(|pos| {
            let start = pos + 1;
            let end = inner[start..]
                .find(|c: char| !c.is_ascii_digit())
                .map(|e| start + e)
                .unwrap_or(inner.len());
            parse_i32!(&inner[start..end])
        })
        .unwrap_or(heal_amount);

    Some(Details {
        heal_amount,
        heal_effective,
        is_crit,
        threat,
        ..Default::default()
    })
}

fn parse_charges(segment: &str) -> Option<Details> {
    let bytes = segment.as_bytes();

    let paren = memchr(b'(', bytes)?;
    let paren_end = memchr(b')', bytes)?;
    let brace = memchr(b'{', bytes)?;
    let brace_end = memchr(b'}', bytes)?;

    // Parse count: number before "charges"
    let inner = &segment[paren + 1..paren_end];
    let count_end = inner
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(inner.len());
    let charges = parse_i32!(&inner[..count_end]);

    // Parse ability ID
    let ability_id = parse_i64!(&segment[brace + 1..brace_end]);

    Some(Details {
        charges,
        ability_id,
        ..Default::default()
    })
}
