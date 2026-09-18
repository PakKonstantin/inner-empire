//! Fuzzy filename matching for the quick switcher.
//!
//! Typing `prjpl` should find `Projects/Project Plan.md`. This is a subsequence
//! match with a score that rewards the things that make a match feel right:
//! consecutive characters, matches at the start of a word, and a short
//! candidate.

/// A scored match, with the positions that matched so the UI can highlight them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyMatch {
    pub score: i32,
    /// Character indices in the candidate that matched, in order.
    pub positions: Vec<usize>,
}

const BONUS_CONSECUTIVE: i32 = 12;
const BONUS_WORD_START: i32 = 10;
const BONUS_EXACT_CASE: i32 = 2;
const PENALTY_GAP: i32 = -1;
const PENALTY_LEADING: i32 = -2;
const MAX_LEADING_PENALTY: i32 = -12;

/// Score `needle` against `candidate`, or `None` if it is not a subsequence.
///
/// Matching is case-insensitive; matching the exact case is only a tiebreak.
pub fn score(needle: &str, candidate: &str) -> Option<FuzzyMatch> {
    if needle.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            positions: Vec::new(),
        });
    }

    let candidate_chars: Vec<char> = candidate.chars().collect();
    let needle_chars: Vec<char> = needle.chars().collect();
    if needle_chars.len() > candidate_chars.len() {
        return None;
    }

    let mut positions = Vec::with_capacity(needle_chars.len());
    let mut total = 0i32;
    let mut candidate_index = 0usize;
    let mut previous_match: Option<usize> = None;

    for needle_char in &needle_chars {
        let lowered = lower(*needle_char);
        let found = (candidate_index..candidate_chars.len())
            .find(|i| lower(candidate_chars[*i]) == lowered)?;

        let mut points = 1;
        if candidate_chars[found] == *needle_char {
            points += BONUS_EXACT_CASE;
        }
        let word_start = is_word_start(&candidate_chars, found);
        if word_start {
            points += BONUS_WORD_START;
        }
        match previous_match {
            Some(previous) if previous + 1 == found => points += BONUS_CONSECUTIVE,
            // Skipping ahead to the start of the next word is the whole point of
            // an initialism, so `pp` finding "Project Plan" must not be charged
            // for the distance between the two words.
            Some(_) if word_start => {}
            Some(previous) => points += PENALTY_GAP * (found - previous - 1).min(10) as i32,
            None if word_start => {}
            None => {
                points += (PENALTY_LEADING * found.min(10) as i32).max(MAX_LEADING_PENALTY);
            }
        }

        total += points;
        positions.push(found);
        previous_match = Some(found);
        candidate_index = found + 1;
    }

    // A shorter candidate containing the same match is the better answer.
    total -= (candidate_chars.len() / 8) as i32;

    Some(FuzzyMatch {
        score: total,
        positions,
    })
}

/// Score against a path, weighting the filename above the folders it sits in.
///
/// Typing `plan` should rank `Plan.md` above `Plans/notes.md`, and both above a
/// note that merely lives in a folder called `Planning`.
pub fn score_path(needle: &str, path: &str) -> Option<FuzzyMatch> {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    let offset = path.chars().count() - file_name.chars().count();

    match score(needle, file_name) {
        Some(mut hit) => {
            hit.score += 20;
            hit.positions = hit.positions.into_iter().map(|p| p + offset).collect();
            Some(hit)
        }
        None => score(needle, path),
    }
}

fn lower(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

fn is_word_start(chars: &[char], index: usize) -> bool {
    if index == 0 {
        return true;
    }
    let previous = chars[index - 1];
    let current = chars[index];
    !previous.is_alphanumeric() || (previous.is_lowercase() && current.is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranked<'a>(needle: &str, candidates: &[&'a str]) -> Vec<&'a str> {
        let mut scored: Vec<(i32, &str)> = candidates
            .iter()
            .filter_map(|c| score_path(needle, c).map(|m| (m.score, *c)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        scored.into_iter().map(|(_, c)| c).collect()
    }

    #[test]
    fn an_initialism_matches_a_multi_word_name() {
        assert!(score("prjpl", "Project Plan").is_some());
        assert!(score("pp", "Project Plan").is_some());
    }

    #[test]
    fn a_non_subsequence_does_not_match() {
        assert!(score("xyz", "Project Plan").is_none());
        assert!(score("planp", "Plan").is_none());
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(score("PLAN", "plan.md").is_some());
        assert!(score("plan", "PLAN.md").is_some());
    }

    #[test]
    fn word_starts_outrank_matches_buried_mid_word() {
        let word_start = score("pp", "Project Plan").unwrap().score;
        let buried = score("pp", "apple pp").unwrap().score;
        assert!(word_start > buried, "{word_start} should beat {buried}");
    }

    #[test]
    fn consecutive_characters_outrank_scattered_ones() {
        let consecutive = score("plan", "plan").unwrap().score;
        let scattered = score("plan", "p-l-a-n").unwrap().score;
        assert!(consecutive > scattered);
    }

    #[test]
    fn the_filename_outranks_a_folder_of_the_same_name() {
        assert_eq!(
            ranked("plan", &["Planning/notes.md", "Plan.md", "a/b/Plan.md"]),
            vec!["Plan.md", "a/b/Plan.md", "Planning/notes.md"]
        );
    }

    #[test]
    fn a_shorter_candidate_wins_when_the_match_is_otherwise_equal() {
        let short = score("ab", "ab").unwrap().score;
        let long = score("ab", "ab with a great deal of other text")
            .unwrap()
            .score;
        assert!(short > long);
    }

    #[test]
    fn positions_point_at_the_characters_that_matched() {
        let hit = score("pl", "Plan").unwrap();
        assert_eq!(hit.positions, vec![0, 1]);

        let in_path = score_path("pl", "Notes/Plan.md").unwrap();
        assert_eq!(in_path.positions, vec![6, 7]);
    }

    #[test]
    fn an_empty_needle_matches_everything_with_no_positions() {
        let hit = score("", "anything").unwrap();
        assert_eq!(hit.score, 0);
        assert!(hit.positions.is_empty());
    }

    #[test]
    fn a_needle_longer_than_the_candidate_cannot_match() {
        assert!(score("abcdef", "abc").is_none());
    }

    #[test]
    fn unicode_candidates_index_by_character_not_byte() {
        let hit = score("é", "café").unwrap();
        assert_eq!(hit.positions, vec![3]);
    }
}
