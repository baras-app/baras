//! Parse text recognized from a raid-frame band.

/// Anything outside this is two fields that ran together, not a reading.
const MIN_VALUE_DIGITS: usize = 3;
const MAX_VALUE_DIGITS: usize = 7;

/// Parse the health text SWTOR prints in an ops frame.
///
/// Reads `271,245 (55%)` into `(Some(271245), Some(55))`. Either half may be
/// missing: the percentage is frequently covered by ops target markers, and the
/// numeric display is an option some users leave off.
///
/// Recognition routinely substitutes letters for digits at raid-frame sizes, so
/// the most common confusions are folded back before parsing.
pub fn parse_health_text(text: &str) -> (Option<u32>, Option<u8>) {
    // Split before folding, not after. The left edge of the crop picks up a
    // frame border that recognition reports as '|' or '[', and folding first
    // would turn it into a digit ('|' => '1') that then looks like part of the
    // value — `|333,269` read as 1,333,269.
    let (value_part, percent_part) = split_percent(text);

    let percent = parse_percent(&fold_digit_lookalikes(percent_part));

    // Nothing that cannot begin a number may lead the value. Splitting the
    // parenthesised part off first means this cannot eat a '(' and mistake a
    // percentage for a value.
    let trimmed = value_part.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
    let value = parse_grouped_value(&fold_digit_lookalikes(trimmed));

    (value, percent)
}

/// Split the value from the parenthesised percentage.
///
/// A real '(' always opens it. A bracket only does so after digits: the same
/// shape at the start of the line is the frame border the crop's left edge
/// picked up, and splitting there would throw the value away.
fn split_percent(text: &str) -> (&str, &str) {
    let mut seen_digit = false;
    for (idx, c) in text.char_indices() {
        match c {
            '(' => return (&text[..idx], &text[idx..]),
            '[' | '{' if seen_digit => return (&text[..idx], &text[idx..]),
            c if c.is_ascii_digit() => seen_digit = true,
            _ => {}
        }
    }
    (text, "")
}

/// Parse the percentage, with or without a surviving '%'.
///
/// Only digits and '%' are rendered between the parentheses, so a bare number
/// that could be a percentage is one. Recognition drops the '%' more often than
/// it reads it, and turns it into '9' or '99' when it does; not looking for the
/// glyph at all sidesteps both.
fn parse_percent(folded: &str) -> Option<u8> {
    folded
        .split(['(', ')', '[', ']', '{', '}'])
        .filter_map(|chunk| {
            let chunk = chunk.trim();
            let digits = chunk.strip_suffix('%').unwrap_or(chunk).trim();
            digits.parse::<u32>().ok()
        })
        .find(|percent| *percent <= 100)
        .map(|percent| percent as u8)
}

/// Fold the letters recognition substitutes for digits at raid-frame sizes.
fn fold_digit_lookalikes(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'O' | 'o' | 'D' | 'Q' => '0',
            'l' | 'I' | 'i' | '|' => '1',
            'S' => '5',
            'B' => '8',
            other => other,
        })
        .collect()
}

/// Parse the absolute health value, using thousands separators as a structural
/// check.
///
/// SWTOR renders `271,245`. When a separator survives recognition, every group
/// after the first must be exactly three digits and the first must be one to
/// three. An artifact that folded into a digit — `I333,269` becoming
/// `1333,269` — shows up as an over-long first group and is trimmed from the
/// left, which is where the stray glyph came from.
///
/// Without a separator there is no structure to check, so the digit run is
/// taken as-is.
fn parse_grouped_value(folded: &str) -> Option<u32> {
    let groups: Vec<String> = folded
        .split([',', '.'])
        .map(|group| group.chars().filter(char::is_ascii_digit).collect())
        .collect();

    let (first, rest) = groups.split_first()?;

    let digits: String = if rest.is_empty() {
        // No separator survived, so there is no structure left to check.
        first.clone()
    } else if rest.iter().any(|group| group.len() != 3) {
        // A shape SWTOR never renders: a delimiter was misread and the groups
        // ran together. Concatenating anyway is how `462,769 (93%)` became
        // 462759193, a number with no sign of being wrong on it.
        return None;
    } else {
        let first = if first.len() > 3 {
            &first[first.len() - 3..]
        } else {
            first.as_str()
        };
        std::iter::once(first).chain(rest.iter().map(String::as_str)).collect()
    };

    if !(MIN_VALUE_DIGITS..=MAX_VALUE_DIGITS).contains(&digits.len()) {
        return None;
    }
    digits.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_health_with_value_and_percent() {
        assert_eq!(parse_health_text("271,245 (55%)"), (Some(271_245), Some(55)));
        assert_eq!(parse_health_text("95,575 (22%)"), (Some(95_575), Some(22)));
        assert_eq!(parse_health_text("485,275 (100%)"), (Some(485_275), Some(100)));
    }

    #[test]
    fn parses_health_without_separators() {
        assert_eq!(parse_health_text("443604 (100%)"), (Some(443_604), Some(100)));
    }

    #[test]
    fn parses_value_when_percent_is_covered_by_a_marker() {
        assert_eq!(parse_health_text("498,994"), (Some(498_994), None));
        assert_eq!(parse_health_text("443,782 ("), (Some(443_782), None));
    }

    #[test]
    fn parses_percent_when_value_is_unreadable() {
        assert_eq!(parse_health_text("(76%)"), (None, Some(76)));
    }

    #[test]
    fn recovers_from_letter_for_digit_confusion() {
        assert_eq!(parse_health_text("44O,917 (1OO%)"), (Some(440_917), Some(100)));
        assert_eq!(parse_health_text("l15,319 (26%)"), (Some(115_319), Some(26)));
    }

    #[test]
    fn drops_a_leading_edge_artifact_instead_of_folding_it_into_a_digit() {
        // Observed live: the crop's left edge produced a '|' that '|' => '1'
        // turned into a leading digit, reading 333,269 as 1,333,269.
        assert_eq!(parse_health_text("|333,269"), (Some(333_269), None));
        assert_eq!(parse_health_text("| 333,269 (55%)"), (Some(333_269), Some(55)));
        assert_eq!(parse_health_text("[ 147.569 (40%)"), (Some(147_569), Some(40)));
        assert_eq!(parse_health_text("[VENNDRICK 274,567"), (Some(274_567), None));
    }

    #[test]
    fn separator_groups_catch_an_artifact_that_folded_into_a_digit() {
        // 'I' survives the non-alphanumeric trim and folds to '1', so only the
        // group structure reveals it: the first group cannot hold four digits.
        assert_eq!(parse_health_text("I333,269"), (Some(333_269), None));
        assert_eq!(parse_health_text("l1333,269 (81%)"), (Some(333_269), Some(81)));
    }

    #[test]
    fn a_short_leading_group_is_legitimate() {
        // 95,575 and 443,604 must survive untouched — only over-long first
        // groups indicate an artifact.
        assert_eq!(parse_health_text("95,575 (22%)"), (Some(95_575), Some(22)));
        assert_eq!(parse_health_text("5,575"), (Some(5_575), None));
    }

    #[test]
    fn trimming_cannot_turn_a_percentage_into_a_value() {
        // Stripping leading punctuation must not eat the '(' and leave "100%)"
        // looking like a three-digit value.
        assert_eq!(parse_health_text("(100%)"), (None, Some(100)));
        assert_eq!(parse_health_text(" (76%)"), (None, Some(76)));
    }

    #[test]
    fn rejects_nonsense_rather_than_guessing() {
        assert_eq!(parse_health_text(""), (None, None));
        assert_eq!(parse_health_text("---"), (None, None));
        // Too few digits to be a health value.
        assert_eq!(parse_health_text("12"), (None, None));
        // Percentages above 100 are a misread, not a reading.
        assert_eq!(parse_health_text("(255%)"), (None, None));
    }

    /// A misread '(' merges value and percentage into one digit run. All of
    /// these read as confident nine-digit health values before.
    #[test]
    fn fields_that_ran_together_are_rejected_not_concatenated() {
        // 462,769 (93%) — the '(' read as '1'.
        assert_eq!(parse_health_text("[ 462,759193)").0, None);
        // 440,917 (100%) — a digit lost, leaving a two-digit second group.
        assert_eq!(parse_health_text("[440.97(100%)").0, None);
        // 442,565 (100%) — leading digit lost to a '?'.
        assert_eq!(parse_health_text("[?42,55 [10%)").0, None);
    }

    /// The bracket opening the percentage and the one the crop's left edge
    /// invents are the same glyph; only the digits before it tell them apart.
    #[test]
    fn a_bracket_opens_the_percentage_only_after_digits() {
        // 391,830 (100%): leading '[' is the frame border, the second opens the
        // percentage. Reading both as noise concatenated them into 391830100.
        assert_eq!(parse_health_text("[391.830[100)"), (Some(391_830), Some(100)));
        assert_eq!(parse_health_text("! 204111(46%)]"), (Some(204_111), Some(46)));
        // A real '(' still opens it with no value in front.
        assert_eq!(parse_health_text("(76%)"), (None, Some(76)));
    }

    /// The '%' is dropped more often than read, so the digits carry it.
    #[test]
    fn the_percentage_survives_a_lost_percent_sign() {
        assert_eq!(parse_health_text("417.073 (85)").1, Some(85));
        // Truncated before the closing bracket.
        assert_eq!(parse_health_text("373590 (85").1, Some(85));
        // '%' read as '9' pushes the group over 100, which is still a misread.
        assert_eq!(parse_health_text("[ 325.152 (8199)"), (Some(325_152), None));
    }
}
