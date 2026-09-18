//! Deciding when the editor should offer completions, and for what.
//!
//! Typing `[[` should open a note picker; typing `#` should offer tags. Both
//! sound trivial and are not: the trigger has to survive a cursor moving
//! backwards through the text, a `[[` inside a code fence, an already-closed
//! link, and a `#` that is the start of a heading rather than a tag.
//!
//! Here rather than in Swift for the same reason as the editing actions — this
//! is logic, there is no Swift compiler on this machine, and logic nobody has
//! run is not logic. Offsets are UTF-16, matching what a text view reports.

/// What the editor should be offering, if anything.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CompletionTrigger {
    /// Nothing to offer at this cursor.
    None,
    /// Inside `[[…`. `query` is what has been typed so far.
    WikiLink {
        query: String,
        /// UTF-16 range of the text to replace, excluding the brackets.
        replace_start: u32,
        replace_end: u32,
        /// The link already has its closing `]]`, so accepting a suggestion
        /// must not add a second pair.
        closed: bool,
    },
    /// After a `#`. `query` is the partial tag, which may contain `/`.
    Tag {
        query: String,
        replace_start: u32,
        replace_end: u32,
    },
}

/// Is the cursor inside a fenced code block?
///
/// An odd number of fence lines before it means yes. Not a Markdown parser —
/// that is `ie-core`'s job and it runs over the whole document, which is too
/// much work for every keystroke — but enough to stop a note picker opening
/// inside a code sample.
fn inside_code_fence(before: &str) -> bool {
    before
        .split('\n')
        .filter(|line| line.trim_start().starts_with("```"))
        .count()
        % 2
        == 1
}

/// How far back to look for an opening `[[`.
///
/// A note name is not a paragraph. Scanning the whole document on every
/// keystroke would be work the battery pays for, and a `[[` a thousand
/// characters back is not one the user is still typing.
const MAX_LOOKBACK: usize = 256;

fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut counted = 0usize;
    for (index, ch) in text.char_indices() {
        if counted >= utf16 {
            return index;
        }
        counted += ch.len_utf16();
    }
    text.len()
}

fn utf16_offset(text: &str, byte: usize) -> u32 {
    let byte = byte.min(text.len());
    text[..byte].chars().map(char::len_utf16).sum::<usize>() as u32
}

/// What, if anything, to complete at `cursor`.
#[uniffi::export]
pub fn completion_at(text: String, cursor: u32) -> CompletionTrigger {
    let at = byte_offset(&text, cursor as usize);
    let before = &text[..at];

    // In code, `[[` is a literal and `#` is a comment, so neither completes.
    // Counting fence *lines* rather than occurrences of the characters: a
    // fence at the very start of the note has no newline before it, and
    // searching for "\n```" misses it.
    if inside_code_fence(before) {
        return CompletionTrigger::None;
    }

    let window_start = before
        .char_indices()
        .rev()
        .take(MAX_LOOKBACK)
        .last()
        .map_or(0, |(i, _)| i);
    let window = &before[window_start..];

    if let Some(trigger) = wikilink_at(&text, window, window_start, at) {
        return trigger;
    }
    tag_at(&text, window, window_start, at).unwrap_or(CompletionTrigger::None)
}

fn wikilink_at(
    text: &str,
    window: &str,
    window_start: usize,
    at: usize,
) -> Option<CompletionTrigger> {
    let open = window.rfind("[[")? + window_start;

    // A `]]` between the brackets and the cursor closes that link; the cursor
    // is past it, not inside it.
    if text[open + 2..at].contains("]]") {
        return None;
    }
    // A link target has no newline in it. Without this, an unclosed `[[` keeps
    // offering completions for the rest of the note.
    let query = &text[open + 2..at];
    if query.contains('\n') {
        return None;
    }

    // `[[Note|Alias` and `[[Note#Heading` — the target is what completes, so
    // once one of those appears the note picker has served its purpose.
    if query.contains('|') || query.contains('#') || query.contains('^') {
        return None;
    }

    // Accepting a suggestion must not add a second `]]`.
    let rest = &text[at..];
    let closed = rest
        .split('\n')
        .next()
        .is_some_and(|line| line.starts_with("]]"));

    Some(CompletionTrigger::WikiLink {
        query: query.to_string(),
        replace_start: utf16_offset(text, open + 2),
        replace_end: utf16_offset(text, at),
        closed,
    })
}

fn tag_at(text: &str, window: &str, window_start: usize, at: usize) -> Option<CompletionTrigger> {
    let hash = window.rfind('#')? + window_start;
    let query = &text[hash + 1..at];

    // A tag is one word. Whitespace ends it, and so does anything a tag
    // cannot contain.
    if query
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '[' | ']' | '(' | ')' | '#'))
    {
        return None;
    }

    // `# Heading` is a heading, not a tag: a `#` at the start of a line
    // followed by a space. By the time there is a non-space character it is
    // ambiguous, and the user typing `#t` on a fresh line means a tag.
    let line_start = text[..hash].rfind('\n').map_or(0, |i| i + 1);
    let indent = &text[line_start..hash];
    if indent.chars().all(char::is_whitespace) && text[hash + 1..].starts_with(' ') {
        return None;
    }

    // `word#tag` is not a tag either; a tag starts a word.
    if hash > 0 {
        let previous = text[..hash].chars().next_back()?;
        if !previous.is_whitespace() && !matches!(previous, '(' | '[' | '>' | '-') {
            return None;
        }
    }

    Some(CompletionTrigger::Tag {
        query: query.to_string(),
        replace_start: utf16_offset(text, hash + 1),
        replace_end: utf16_offset(text, at),
    })
}

/// Put a chosen completion into the text.
///
/// Returns the new text and where the cursor should land: after the closing
/// brackets for a link, after the tag for a tag, so typing continues in the
/// right place either way.
#[uniffi::export]
pub fn apply_completion(
    text: String,
    trigger: CompletionTrigger,
    choice: String,
) -> crate::editing::EditResult {
    let (start16, end16, insert, trailing) = match trigger {
        CompletionTrigger::None => {
            return crate::editing::EditResult {
                selection_start: utf16_offset(&text, text.len()),
                selection_end: utf16_offset(&text, text.len()),
                text,
            }
        }
        CompletionTrigger::WikiLink {
            replace_start,
            replace_end,
            closed,
            ..
        } => (
            replace_start,
            replace_end,
            choice,
            if closed { "" } else { "]]" },
        ),
        CompletionTrigger::Tag {
            replace_start,
            replace_end,
            ..
        } => (replace_start, replace_end, choice, ""),
    };

    let start = byte_offset(&text, start16 as usize);
    let end = byte_offset(&text, end16 as usize);

    let mut out = String::with_capacity(text.len() + insert.len() + trailing.len());
    out.push_str(&text[..start]);
    out.push_str(&insert);
    out.push_str(trailing);
    out.push_str(&text[end..]);

    let cursor = start + insert.len() + trailing.len();
    let cursor16 = utf16_offset(&out, cursor);
    crate::editing::EditResult {
        text: out,
        selection_start: cursor16,
        selection_end: cursor16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query_of(trigger: &CompletionTrigger) -> Option<&str> {
        match trigger {
            CompletionTrigger::WikiLink { query, .. } | CompletionTrigger::Tag { query, .. } => {
                Some(query)
            }
            CompletionTrigger::None => None,
        }
    }

    #[test]
    fn two_brackets_start_a_note_completion() {
        let trigger = completion_at("see [[".into(), 6);
        assert!(matches!(trigger, CompletionTrigger::WikiLink { .. }));
        assert_eq!(query_of(&trigger), Some(""));
    }

    #[test]
    fn the_partial_name_is_the_query() {
        let trigger = completion_at("see [[Other No".into(), 14);
        assert_eq!(query_of(&trigger), Some("Other No"));
        match trigger {
            CompletionTrigger::WikiLink {
                replace_start,
                replace_end,
                closed,
                ..
            } => {
                assert_eq!((replace_start, replace_end), (6, 14));
                assert!(!closed);
            }
            other => panic!("expected a wiki link, got {other:?}"),
        }
    }

    #[test]
    fn a_closed_link_is_noticed_so_brackets_are_not_doubled() {
        // Cursor between the brackets of `[[]]`, which is where the toolbar
        // button leaves it.
        let trigger = completion_at("see [[]]".into(), 6);
        match trigger {
            CompletionTrigger::WikiLink { closed, .. } => assert!(closed),
            other => panic!("expected a wiki link, got {other:?}"),
        }
    }

    #[test]
    fn a_finished_link_does_not_keep_completing() {
        // The cursor is past the `]]`, so this link is done.
        let trigger = completion_at("see [[Note]] and more".into(), 21);
        assert_eq!(trigger, CompletionTrigger::None);
    }

    #[test]
    fn a_link_does_not_run_past_the_end_of_a_line() {
        let trigger = completion_at("see [[unclosed\nnext line".into(), 24);
        assert_eq!(trigger, CompletionTrigger::None);
    }

    #[test]
    fn an_alias_or_heading_ends_the_note_picker() {
        // Past the `|` the user is naming the link, not choosing the note.
        assert_eq!(
            completion_at("see [[Note|Al".into(), 13),
            CompletionTrigger::None
        );
        assert_eq!(
            completion_at("see [[Note#Head".into(), 15),
            CompletionTrigger::None
        );
        assert_eq!(
            completion_at("see [[Note^blo".into(), 14),
            CompletionTrigger::None
        );
    }

    #[test]
    fn brackets_inside_a_code_fence_are_literal() {
        let text = "```\nlet x = [[1, 2]];\n".to_string();
        let cursor = text.chars().map(char::len_utf16).sum::<usize>() as u32;
        assert_eq!(completion_at(text, cursor), CompletionTrigger::None);
    }

    #[test]
    fn brackets_after_a_closed_fence_complete_again() {
        let text = "```\ncode\n```\n\nsee [[No".to_string();
        let cursor = text.chars().map(char::len_utf16).sum::<usize>() as u32;
        assert_eq!(query_of(&completion_at(text, cursor)), Some("No"));
    }

    #[test]
    fn a_hash_starts_a_tag_completion() {
        let trigger = completion_at("about #proj".into(), 11);
        assert_eq!(query_of(&trigger), Some("proj"));
        match trigger {
            CompletionTrigger::Tag {
                replace_start,
                replace_end,
                ..
            } => assert_eq!((replace_start, replace_end), (7, 11)),
            other => panic!("expected a tag, got {other:?}"),
        }
    }

    #[test]
    fn a_nested_tag_keeps_completing_past_the_slash() {
        assert_eq!(
            query_of(&completion_at("about #proj/al".into(), 14)),
            Some("proj/al")
        );
    }

    #[test]
    fn a_heading_is_not_a_tag() {
        // `# ` at the start of a line.
        assert_eq!(completion_at("# Title".into(), 7), CompletionTrigger::None);
        assert_eq!(
            completion_at("intro\n## Sub".into(), 12),
            CompletionTrigger::None
        );
    }

    #[test]
    fn a_hash_starting_a_line_with_no_space_is_a_tag() {
        // `#project` on its own line is a tag, not a heading.
        assert_eq!(query_of(&completion_at("#proj".into(), 5)), Some("proj"));
    }

    #[test]
    fn a_hash_in_the_middle_of_a_word_is_not_a_tag() {
        assert_eq!(completion_at("issue#42".into(), 8), CompletionTrigger::None);
    }

    #[test]
    fn a_space_ends_the_tag() {
        assert_eq!(
            completion_at("about #proj and".into(), 15),
            CompletionTrigger::None
        );
    }

    #[test]
    fn accepting_a_note_adds_the_closing_brackets_once() {
        let text = "see [[Oth".to_string();
        let trigger = completion_at(text.clone(), 9);
        let out = apply_completion(text, trigger, "Other Note".into());
        assert_eq!(out.text, "see [[Other Note]]");
        // Past the brackets, so typing continues after the link.
        assert_eq!(out.selection_start, 18);
    }

    #[test]
    fn accepting_into_an_already_closed_link_does_not_double_them() {
        let text = "see [[Oth]]".to_string();
        let trigger = completion_at(text.clone(), 9);
        let out = apply_completion(text, trigger, "Other Note".into());
        assert_eq!(out.text, "see [[Other Note]]");
    }

    #[test]
    fn accepting_a_tag_replaces_the_partial() {
        let text = "about #proj".to_string();
        let trigger = completion_at(text.clone(), 11);
        let out = apply_completion(text, trigger, "project/alpha".into());
        assert_eq!(out.text, "about #project/alpha");
        assert_eq!(out.selection_start, 20);
    }

    #[test]
    fn offsets_hold_when_the_note_contains_wider_characters() {
        // "🎉" is two UTF-16 units. A text view reports the cursor at 8; a
        // byte offset would be 10 and would cut the note in the wrong place.
        let text = "🎉 see [[Oth".to_string();
        let cursor = text.chars().map(char::len_utf16).sum::<usize>() as u32;
        let trigger = completion_at(text.clone(), cursor);
        assert_eq!(query_of(&trigger), Some("Oth"));

        let out = apply_completion(text, trigger, "Other".into());
        assert_eq!(out.text, "🎉 see [[Other]]");
    }

    #[test]
    fn no_cursor_position_in_a_mixed_width_note_can_crash() {
        let text = "🎉 [[é漢 #tag\n## head\n```\n[[x]]\n```\n[[".to_string();
        let length = text.chars().map(char::len_utf16).sum::<usize>() as u32;
        for cursor in 0..=length + 5 {
            let trigger = completion_at(text.clone(), cursor);
            let _ = apply_completion(text.clone(), trigger, "Choice".into());
        }
    }

    #[test]
    fn a_far_away_bracket_is_not_still_being_typed() {
        // Beyond the lookback window: the user is not completing a link they
        // opened three paragraphs ago.
        let text = format!("[[{}", "x".repeat(400));
        let cursor = text.chars().map(char::len_utf16).sum::<usize>() as u32;
        assert_eq!(completion_at(text, cursor), CompletionTrigger::None);
    }
}
