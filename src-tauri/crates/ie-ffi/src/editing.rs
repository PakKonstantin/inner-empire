//! Markdown editing actions, for the keyboard accessory bar.
//!
//! These are pure: text plus a selection in, text plus a selection out. That
//! is what makes them testable without a text view, and it is why they live
//! here rather than in Swift — on a host with no Swift compiler, logic written
//! in Swift is logic nobody has run.
//!
//! **Offsets are UTF-16.** `UITextView` and `NSRange` count UTF-16 code units;
//! Rust counts UTF-8 bytes. A note containing an emoji or a single accented
//! character makes the two disagree, and an edit applied at the wrong offset
//! corrupts the note rather than failing visibly. So the boundary speaks
//! UTF-16 and the conversion happens here, once, with tests.
//!
//! The semantics deliberately match the desktop's `src/editor/commands.ts`:
//! pressing the same button twice undoes it, and selecting nothing puts the
//! cursor where typing should continue.

/// The result of an editing action.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EditResult {
    pub text: String,
    /// UTF-16 offsets, ready to hand back to a text view.
    pub selection_start: u32,
    pub selection_end: u32,
}

/// A selection, in the units the host speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Selection {
    pub start: u32,
    pub end: u32,
}

impl Selection {
    fn ordered(self) -> (usize, usize) {
        let (a, b) = (self.start as usize, self.end as usize);
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }
}

/// UTF-16 offset to a byte index, clamped to the text.
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

/// Byte index back to a UTF-16 offset.
fn utf16_offset(text: &str, byte: usize) -> u32 {
    let byte = byte.min(text.len());
    text[..byte].chars().map(char::len_utf16).sum::<usize>() as u32
}

fn result(text: String, start_byte: usize, end_byte: usize) -> EditResult {
    EditResult {
        selection_start: utf16_offset(&text, start_byte),
        selection_end: utf16_offset(&text, end_byte),
        text,
    }
}

/// Wrap or unwrap the selection with `marker` — bold, italic, code.
///
/// Three cases, in the order a person expects: markers just outside the
/// selection are removed, a selection that is itself wrapped is unwrapped, and
/// anything else is wrapped. With nothing selected the cursor lands between
/// the markers so typing continues inside them.
#[uniffi::export]
pub fn toggle_wrap(text: String, selection: Selection, marker: String) -> EditResult {
    let (from16, to16) = selection.ordered();
    let (from, to) = (byte_offset(&text, from16), byte_offset(&text, to16));
    let width = marker.len();

    // The user selected the text and the markers are around it.
    //
    // `from - width` can land inside a multi-byte character — an emoji before
    // the selection is four bytes, and a two-byte marker would slice it in
    // half — so the boundary is checked rather than assumed. Slicing there
    // would panic, which on a phone means the app disappears mid-sentence.
    let before_start = from.saturating_sub(width);
    let has_before =
        from >= width && text.is_char_boundary(before_start) && text[before_start..from] == marker;
    let has_after = to + width <= text.len()
        && text.is_char_boundary(to + width)
        && text[to..to + width] == marker;
    if has_before && has_after {
        let mut out = String::with_capacity(text.len() - width * 2);
        out.push_str(&text[..before_start]);
        out.push_str(&text[from..to]);
        out.push_str(&text[to + width..]);
        let new_from = before_start;
        return result(out, new_from, new_from + (to - from));
    }

    // The selection includes the markers.
    let selected = &text[from..to];
    if selected.len() > width * 2 && selected.starts_with(&marker) && selected.ends_with(&marker) {
        let inner = &selected[width..selected.len() - width];
        let mut out = String::with_capacity(text.len() - width * 2);
        out.push_str(&text[..from]);
        out.push_str(inner);
        out.push_str(&text[to..]);
        let inner_len = inner.len();
        return result(out, from, from + inner_len);
    }

    let mut out = String::with_capacity(text.len() + width * 2);
    out.push_str(&text[..from]);
    out.push_str(&marker);
    out.push_str(selected);
    out.push_str(&marker);
    out.push_str(&text[to..]);

    if from == to {
        let cursor = from + width;
        result(out, cursor, cursor)
    } else {
        result(out, from + width, to + width)
    }
}

/// The byte range of the lines the selection touches.
fn line_span(text: &str, from: usize, to: usize) -> (usize, usize) {
    let start = text[..from].rfind('\n').map_or(0, |i| i + 1);
    let end = text[to..].find('\n').map_or(text.len(), |i| to + i);
    (start, end)
}

/// Apply `f` to each line the selection touches, keeping the selection over
/// the same lines afterwards.
fn map_lines(text: &str, selection: Selection, f: impl Fn(&str) -> String) -> EditResult {
    let (from16, to16) = selection.ordered();
    let (from, to) = (byte_offset(text, from16), byte_offset(text, to16));
    let (span_start, span_end) = line_span(text, from, to);

    let replaced: Vec<String> = text[span_start..span_end].split('\n').map(&f).collect();
    let replacement = replaced.join("\n");

    let mut out = String::with_capacity(text.len() + replacement.len());
    out.push_str(&text[..span_start]);
    out.push_str(&replacement);
    out.push_str(&text[span_end..]);

    // The selection follows the lines rather than the offsets: adding a "- "
    // should not leave the cursor two characters to the left of where it was.
    result(out, span_start, span_start + replacement.len())
}

/// Set, change or remove the heading level of the lines the selection touches.
///
/// Choosing the level a line already has removes it, so the same button
/// toggles. Level 0 always removes.
#[uniffi::export]
pub fn set_heading_level(text: String, selection: Selection, level: u8) -> EditResult {
    map_lines(&text, selection, |line| {
        let stripped = strip_heading(line);
        let existing = heading_level(line);
        if level == 0 || existing == Some(level) {
            stripped.to_string()
        } else {
            format!("{} {stripped}", "#".repeat(level.clamp(1, 6) as usize))
        }
    })
}

fn heading_level(line: &str) -> Option<u8> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    // A heading needs whitespace after the hashes; `#tag` is a tag.
    line[hashes..].starts_with(' ').then_some(hashes as u8)
}

fn strip_heading(line: &str) -> &str {
    match heading_level(line) {
        Some(level) => line[level as usize..].trim_start_matches(' '),
        None => line,
    }
}

/// Add or remove a `> ` quote marker on the lines the selection touches.
#[uniffi::export]
pub fn toggle_quote(text: String, selection: Selection) -> EditResult {
    let (from16, to16) = selection.ordered();
    let (from, to) = (byte_offset(&text, from16), byte_offset(&text, to16));
    let (span_start, span_end) = line_span(&text, from, to);
    // Mixed lines become all-quoted rather than toggling each one, which is
    // what makes the button predictable on a multi-line selection.
    let all_quoted = text[span_start..span_end]
        .split('\n')
        .all(|line| line.trim_start().starts_with('>'));

    map_lines(&text, selection, |line| {
        if all_quoted {
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            format!(
                "{indent}{}",
                trimmed.trim_start_matches('>').trim_start_matches(' ')
            )
        } else if line.trim().is_empty() {
            line.to_string()
        } else {
            format!("> {line}")
        }
    })
}

/// Add or remove a `- ` bullet on the lines the selection touches.
#[uniffi::export]
pub fn toggle_bullet(text: String, selection: Selection) -> EditResult {
    let (from16, to16) = selection.ordered();
    let (from, to) = (byte_offset(&text, from16), byte_offset(&text, to16));
    let (span_start, span_end) = line_span(&text, from, to);
    let all_bulleted = text[span_start..span_end].split('\n').all(is_bullet);

    map_lines(&text, selection, |line| {
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        if all_bulleted {
            format!("{indent}{}", strip_bullet(trimmed))
        } else if trimmed.is_empty() {
            line.to_string()
        } else {
            format!("{indent}- {}", strip_bullet(trimmed))
        }
    })
}

fn is_bullet(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ")
}

fn strip_bullet(trimmed: &str) -> &str {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return rest;
        }
    }
    trimmed
}

/// Add, tick or remove a `- [ ] ` task marker.
///
/// Three states rather than two, because that is what a checklist is: not a
/// task, an unticked task, a ticked one. Pressing the button walks them.
#[uniffi::export]
pub fn toggle_task(text: String, selection: Selection) -> EditResult {
    let (from16, to16) = selection.ordered();
    let (from, to) = (byte_offset(&text, from16), byte_offset(&text, to16));
    let (span_start, span_end) = line_span(&text, from, to);
    let lines: Vec<&str> = text[span_start..span_end].split('\n').collect();

    let all_unticked = lines.iter().all(|l| task_state(l) == Some(false));
    let all_ticked = lines.iter().all(|l| task_state(l) == Some(true));

    map_lines(&text, selection, move |line| {
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        let body = strip_task(strip_bullet(trimmed));
        if trimmed.is_empty() {
            line.to_string()
        } else if all_ticked {
            format!("{indent}{body}")
        } else if all_unticked {
            format!("{indent}- [x] {body}")
        } else {
            format!("{indent}- [ ] {body}")
        }
    })
}

fn task_state(line: &str) -> Option<bool> {
    let trimmed = strip_bullet(line.trim_start());
    if trimmed.starts_with("[ ] ") || trimmed == "[ ]" {
        Some(false)
    } else if trimmed.starts_with("[x] ") || trimmed.starts_with("[X] ") || trimmed == "[x]" {
        Some(true)
    } else {
        None
    }
}

fn strip_task(trimmed: &str) -> &str {
    for marker in ["[ ] ", "[x] ", "[X] "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return rest;
        }
    }
    trimmed.trim_start_matches("[ ]").trim_start_matches("[x]")
}

/// Insert `[[]]` and put the cursor between the brackets, or wrap a selection.
#[uniffi::export]
pub fn insert_wikilink(text: String, selection: Selection) -> EditResult {
    let (from16, to16) = selection.ordered();
    let (from, to) = (byte_offset(&text, from16), byte_offset(&text, to16));
    let selected = &text[from..to];

    let mut out = String::with_capacity(text.len() + 4);
    out.push_str(&text[..from]);
    out.push_str("[[");
    out.push_str(selected);
    out.push_str("]]");
    out.push_str(&text[to..]);

    // With nothing selected the cursor goes inside, where the note name is
    // about to be typed and where autocomplete should fire.
    if from == to {
        let cursor = from + 2;
        result(out, cursor, cursor)
    } else {
        result(out, from + 2, to + 2)
    }
}

/// Insert a `#` at the cursor, ready for a tag name.
#[uniffi::export]
pub fn insert_tag(text: String, selection: Selection) -> EditResult {
    let (from16, to16) = selection.ordered();
    let (from, to) = (byte_offset(&text, from16), byte_offset(&text, to16));

    // A tag needs whitespace before it, or it is part of the previous word.
    let needs_space = from > 0 && !text[..from].ends_with(|c: char| c.is_whitespace());
    let prefix = if needs_space { " #" } else { "#" };

    let mut out = String::with_capacity(text.len() + 2);
    out.push_str(&text[..from]);
    out.push_str(prefix);
    out.push_str(&text[from..to]);
    out.push_str(&text[to..]);

    let cursor = from + prefix.len() + (to - from);
    result(out, cursor, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(start: u32, end: u32) -> Selection {
        Selection { start, end }
    }

    #[test]
    fn wrapping_a_selection_puts_it_between_markers() {
        let out = toggle_wrap("make this bold".into(), sel(5, 9), "**".into());
        assert_eq!(out.text, "make **this** bold");
        assert_eq!((out.selection_start, out.selection_end), (7, 11));
    }

    #[test]
    fn wrapping_nothing_leaves_the_cursor_inside() {
        let out = toggle_wrap("ab".into(), sel(1, 1), "**".into());
        assert_eq!(out.text, "a****b");
        // Between the two pairs, so typing continues in bold.
        assert_eq!((out.selection_start, out.selection_end), (3, 3));
    }

    #[test]
    fn pressing_bold_twice_undoes_it() {
        let once = toggle_wrap("make this bold".into(), sel(5, 9), "**".into());
        let twice = toggle_wrap(
            once.text.clone(),
            sel(once.selection_start, once.selection_end),
            "**".into(),
        );
        assert_eq!(twice.text, "make this bold");
        assert_eq!((twice.selection_start, twice.selection_end), (5, 9));
    }

    #[test]
    fn unwrapping_works_when_the_markers_are_selected_too() {
        let out = toggle_wrap("make **this** bold".into(), sel(5, 13), "**".into());
        assert_eq!(out.text, "make this bold");
        assert_eq!((out.selection_start, out.selection_end), (5, 9));
    }

    #[test]
    fn offsets_are_utf16_so_emoji_do_not_shift_the_edit() {
        // "🎉" is one char, two UTF-16 units, four UTF-8 bytes. A text view
        // says the selection starts at 2; getting this wrong writes the
        // markers into the middle of the emoji's bytes.
        let out = toggle_wrap("🎉 party".into(), sel(3, 8), "**".into());
        assert_eq!(out.text, "🎉 **party**");
        assert_eq!((out.selection_start, out.selection_end), (5, 10));
    }

    #[test]
    fn accented_characters_are_counted_the_way_a_text_view_counts_them() {
        // "é" is one UTF-16 unit but two UTF-8 bytes.
        let out = toggle_wrap("café time".into(), sel(5, 9), "*".into());
        assert_eq!(out.text, "café *time*");
        assert_eq!((out.selection_start, out.selection_end), (6, 10));
    }

    #[test]
    fn a_heading_is_added_and_the_same_level_removes_it() {
        let added = set_heading_level("Title".into(), sel(0, 0), 2);
        assert_eq!(added.text, "## Title");

        let removed = set_heading_level(added.text.clone(), sel(0, 0), 2);
        assert_eq!(removed.text, "Title");
    }

    #[test]
    fn a_different_level_replaces_rather_than_stacks() {
        let out = set_heading_level("### Deep".into(), sel(0, 0), 1);
        assert_eq!(out.text, "# Deep");
    }

    #[test]
    fn a_tag_at_the_start_of_a_line_is_not_a_heading() {
        // `#tag` has no space, so it is a tag and must survive untouched.
        let out = set_heading_level("#project notes".into(), sel(0, 0), 1);
        assert_eq!(out.text, "# #project notes");
    }

    #[test]
    fn headings_apply_to_every_line_the_selection_touches() {
        let text = "one\ntwo\nthree".to_string();
        let out = set_heading_level(text, sel(1, 6), 2);
        assert_eq!(out.text, "## one\n## two\nthree");
    }

    #[test]
    fn quoting_adds_and_removes() {
        let added = toggle_quote("a line".into(), sel(0, 0));
        assert_eq!(added.text, "> a line");
        assert_eq!(toggle_quote(added.text, sel(0, 0)).text, "a line");
    }

    #[test]
    fn a_partly_quoted_selection_becomes_fully_quoted() {
        // Toggling each line separately would leave it just as mixed.
        let out = toggle_quote("> one\ntwo".into(), sel(0, 9));
        assert_eq!(out.text, "> > one\n> two");
    }

    #[test]
    fn bullets_add_and_remove_and_keep_indentation() {
        let added = toggle_bullet("  item".into(), sel(0, 0));
        assert_eq!(added.text, "  - item");
        assert_eq!(toggle_bullet(added.text, sel(0, 0)).text, "  item");
    }

    #[test]
    fn a_bullet_of_any_style_counts_as_one() {
        assert_eq!(toggle_bullet("* item".into(), sel(0, 0)).text, "item");
        assert_eq!(toggle_bullet("+ item".into(), sel(0, 0)).text, "item");
    }

    #[test]
    fn a_task_walks_three_states() {
        let plain = "buy milk".to_string();
        let unticked = toggle_task(plain, sel(0, 0));
        assert_eq!(unticked.text, "- [ ] buy milk");

        let ticked = toggle_task(unticked.text, sel(0, 0));
        assert_eq!(ticked.text, "- [x] buy milk");

        let back = toggle_task(ticked.text, sel(0, 0));
        assert_eq!(back.text, "buy milk");
    }

    #[test]
    fn a_bullet_becomes_a_task_without_doubling_the_marker() {
        let out = toggle_task("- buy milk".into(), sel(0, 0));
        assert_eq!(out.text, "- [ ] buy milk");
    }

    #[test]
    fn a_wikilink_leaves_the_cursor_where_the_name_goes() {
        let out = insert_wikilink("see ".into(), sel(4, 4));
        assert_eq!(out.text, "see [[]]");
        // Between the brackets, which is also where autocomplete fires.
        assert_eq!((out.selection_start, out.selection_end), (6, 6));
    }

    #[test]
    fn a_wikilink_wraps_a_selection() {
        let out = insert_wikilink("see Other Note here".into(), sel(4, 14));
        assert_eq!(out.text, "see [[Other Note]] here");
    }

    #[test]
    fn a_tag_gets_a_space_when_it_would_otherwise_join_a_word() {
        let joined = insert_tag("word".into(), sel(4, 4));
        assert_eq!(joined.text, "word #");
        assert_eq!(joined.selection_start, 6);

        let after_space = insert_tag("word ".into(), sel(5, 5));
        assert_eq!(after_space.text, "word #");

        let at_start = insert_tag(String::new(), sel(0, 0));
        assert_eq!(at_start.text, "#");
    }

    #[test]
    fn an_empty_line_is_left_alone_by_list_markers() {
        // Bulleting a blank line produces "- " with nothing after it, which is
        // litter rather than a list.
        let out = toggle_bullet("one\n\ntwo".into(), sel(0, 8));
        assert_eq!(out.text, "- one\n\n- two");
    }

    #[test]
    fn no_offset_in_a_mixed_width_note_can_crash_an_action() {
        // The emoji bug this caught was a panic, not a wrong answer, and on a
        // phone a panic is the app vanishing mid-sentence. So every action is
        // driven at every offset of a string that mixes one-, two-, three- and
        // four-byte characters, with the selection running both ways.
        let text = "a é 漢 🎉 **b** - [ ] c\n## d\n> e".to_string();
        let length = text.chars().map(char::len_utf16).sum::<usize>() as u32;

        for start in 0..=length {
            for end in 0..=length {
                let selection = sel(start, end);
                // Past-the-end offsets too: a text view can report one after a
                // change it has not applied yet.
                let wide = sel(start, end + 5);

                for selection in [selection, wide] {
                    let _ = toggle_wrap(text.clone(), selection, "**".into());
                    let _ = toggle_wrap(text.clone(), selection, "*".into());
                    let _ = toggle_wrap(text.clone(), selection, "`".into());
                    let _ = set_heading_level(text.clone(), selection, 2);
                    let _ = set_heading_level(text.clone(), selection, 0);
                    let _ = toggle_quote(text.clone(), selection);
                    let _ = toggle_bullet(text.clone(), selection);
                    let _ = toggle_task(text.clone(), selection);
                    let _ = insert_wikilink(text.clone(), selection);
                    let _ = insert_tag(text.clone(), selection);
                }
            }
        }
    }

    #[test]
    fn an_action_never_reports_a_selection_past_the_end() {
        let text = "é🎉 word".to_string();
        let length = text.chars().map(char::len_utf16).sum::<usize>() as u32;
        for start in 0..=length {
            let out = insert_wikilink(text.clone(), sel(start, length));
            let new_length = out.text.chars().map(char::len_utf16).sum::<usize>() as u32;
            assert!(out.selection_end <= new_length, "{out:?}");
        }
    }

    #[test]
    fn every_action_leaves_the_selection_inside_the_text() {
        // A selection past the end is what makes a text view throw.
        let text = "some text\nwith lines".to_string();
        let cases: Vec<EditResult> = vec![
            toggle_wrap(text.clone(), sel(0, 4), "**".into()),
            set_heading_level(text.clone(), sel(0, 4), 3),
            toggle_quote(text.clone(), sel(0, 4)),
            toggle_bullet(text.clone(), sel(0, 4)),
            toggle_task(text.clone(), sel(0, 4)),
            insert_wikilink(text.clone(), sel(0, 4)),
            insert_tag(text.clone(), sel(0, 4)),
        ];
        for out in cases {
            let length = out.text.chars().map(char::len_utf16).sum::<usize>() as u32;
            assert!(out.selection_start <= length, "{out:?}");
            assert!(out.selection_end <= length, "{out:?}");
            assert!(out.selection_start <= out.selection_end, "{out:?}");
        }
    }
}
