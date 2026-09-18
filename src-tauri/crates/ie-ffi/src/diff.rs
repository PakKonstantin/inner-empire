//! A line diff, for showing what a conflict actually changed.
//!
//! When a note changed on disk while it was open, the app must ask rather than
//! pick a winner — and "ask" is worthless if the only options are two opaque
//! blobs. This produces the hunks a Compare view needs, so the choice is
//! between visible alternatives.
//!
//! Lines rather than characters: Markdown is edited a paragraph at a time, and
//! a character diff of two prose paragraphs produces noise no one can read.
//!
//! It lives here and not in `ie-core` deliberately. A diff is a sync concern,
//! not part of what a note *is*, and the core's job is the note format.

/// What happened to one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LineChange {
    /// Present in both, unchanged.
    Same,
    /// Only in the local version.
    Added,
    /// Only in the version on disk.
    Removed,
}

/// One line of the comparison.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DiffLine {
    pub change: LineChange,
    pub text: String,
    /// 1-based line number in the local text, when the line is in it.
    pub local_line: Option<u32>,
    /// 1-based line number in the text on disk, when the line is in it.
    pub remote_line: Option<u32>,
}

/// A run of changed lines, with the unchanged context either side.
///
/// Hunks rather than a flat list because that is the unit a person resolves:
/// "keep mine here, theirs there" is a decision per hunk, and offering it per
/// line would be unusable on a phone.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DiffHunk {
    pub lines: Vec<DiffLine>,
    /// False for a run of context between changes.
    pub has_changes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TextDiff {
    pub hunks: Vec<DiffHunk>,
    pub added: u32,
    pub removed: u32,
    /// The two texts are the same. The UI says so rather than showing an empty
    /// comparison, which looks like a failure.
    pub identical: bool,
}

/// Compare two versions of a note.
///
/// `local` is what the user has typed; `remote` is what is on disk now.
#[uniffi::export]
pub fn diff_text(local: String, remote: String, context_lines: u32) -> TextDiff {
    let local_lines: Vec<&str> = local.split('\n').collect();
    let remote_lines: Vec<&str> = remote.split('\n').collect();

    let ops = lcs_diff(&remote_lines, &local_lines);

    let mut added = 0u32;
    let mut removed = 0u32;
    for op in &ops {
        match op.change {
            LineChange::Added => added += 1,
            LineChange::Removed => removed += 1,
            LineChange::Same => {}
        }
    }

    if added == 0 && removed == 0 {
        return TextDiff {
            hunks: Vec::new(),
            added: 0,
            removed: 0,
            identical: true,
        };
    }

    TextDiff {
        hunks: group(ops, context_lines as usize),
        added,
        removed,
        identical: false,
    }
}

/// The longest common subsequence, as a sequence of per-line operations.
///
/// The classic dynamic-programming LCS rather than Myers: a note is thousands
/// of lines at most, the table is trivially fast at that size, and the code is
/// short enough to be obviously correct. Myers is the right answer for a
/// repository's worth of source, which this is not.
///
/// Long common prefixes and suffixes are stripped first, so the usual case —
/// one paragraph edited in a long note — never builds a table at all.
fn lcs_diff(remote: &[&str], local: &[&str]) -> Vec<DiffLine> {
    let prefix = remote
        .iter()
        .zip(local.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let max_suffix = remote.len().min(local.len()) - prefix;
    let suffix = remote
        .iter()
        .rev()
        .zip(local.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();

    let mut out = Vec::with_capacity(remote.len().max(local.len()));
    for (i, text) in remote.iter().take(prefix).enumerate() {
        out.push(DiffLine {
            change: LineChange::Same,
            text: (*text).to_string(),
            local_line: Some(i as u32 + 1),
            remote_line: Some(i as u32 + 1),
        });
    }

    let remote_mid = &remote[prefix..remote.len() - suffix];
    let local_mid = &local[prefix..local.len() - suffix];

    // table[i][j] = LCS length of remote_mid[i..] and local_mid[j..]
    let (n, m) = (remote_mid.len(), local_mid.len());
    let mut table = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if remote_mid[i] == local_mid[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if remote_mid[i] == local_mid[j] {
            out.push(DiffLine {
                change: LineChange::Same,
                text: remote_mid[i].to_string(),
                local_line: Some((prefix + j) as u32 + 1),
                remote_line: Some((prefix + i) as u32 + 1),
            });
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            out.push(DiffLine {
                change: LineChange::Removed,
                text: remote_mid[i].to_string(),
                local_line: None,
                remote_line: Some((prefix + i) as u32 + 1),
            });
            i += 1;
        } else {
            out.push(DiffLine {
                change: LineChange::Added,
                text: local_mid[j].to_string(),
                local_line: Some((prefix + j) as u32 + 1),
                remote_line: None,
            });
            j += 1;
        }
    }
    while i < n {
        out.push(DiffLine {
            change: LineChange::Removed,
            text: remote_mid[i].to_string(),
            local_line: None,
            remote_line: Some((prefix + i) as u32 + 1),
        });
        i += 1;
    }
    while j < m {
        out.push(DiffLine {
            change: LineChange::Added,
            text: local_mid[j].to_string(),
            local_line: Some((prefix + j) as u32 + 1),
            remote_line: None,
        });
        j += 1;
    }

    for k in 0..suffix {
        let text = remote[remote.len() - suffix + k];
        out.push(DiffLine {
            change: LineChange::Same,
            text: text.to_string(),
            local_line: Some((local.len() - suffix + k) as u32 + 1),
            remote_line: Some((remote.len() - suffix + k) as u32 + 1),
        });
    }
    out
}

/// Collapse long runs of unchanged lines, keeping `context` either side.
fn group(lines: Vec<DiffLine>, context: usize) -> Vec<DiffHunk> {
    let changed: Vec<bool> = lines.iter().map(|l| l.change != LineChange::Same).collect();

    // A line is kept when it is changed or within `context` of one.
    let keep: Vec<bool> = (0..lines.len())
        .map(|i| {
            let lo = i.saturating_sub(context);
            let hi = (i + context + 1).min(lines.len());
            changed[lo..hi].iter().any(|c| *c)
        })
        .collect();

    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut current: Vec<DiffLine> = Vec::new();
    let mut current_has_changes = false;

    for (line, kept) in lines.into_iter().zip(keep) {
        if !kept {
            if !current.is_empty() {
                hunks.push(DiffHunk {
                    lines: std::mem::take(&mut current),
                    has_changes: current_has_changes,
                });
                current_has_changes = false;
            }
            continue;
        }
        if line.change != LineChange::Same {
            current_has_changes = true;
        }
        current.push(line);
    }
    if !current.is_empty() {
        hunks.push(DiffHunk {
            lines: current,
            has_changes: current_has_changes,
        });
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changes(diff: &TextDiff) -> Vec<(LineChange, String)> {
        diff.hunks
            .iter()
            .flat_map(|h| h.lines.iter())
            .map(|l| (l.change, l.text.clone()))
            .collect()
    }

    #[test]
    fn identical_text_is_reported_as_identical() {
        let diff = diff_text("same\ntext\n".into(), "same\ntext\n".into(), 3);
        assert!(diff.identical);
        assert!(diff.hunks.is_empty());
        assert_eq!((diff.added, diff.removed), (0, 0));
    }

    #[test]
    fn an_edited_line_shows_both_versions() {
        let diff = diff_text("a\nMINE\nc".into(), "a\nTHEIRS\nc".into(), 3);
        assert!(!diff.identical);
        assert_eq!(
            changes(&diff),
            vec![
                (LineChange::Same, "a".into()),
                (LineChange::Removed, "THEIRS".into()),
                (LineChange::Added, "MINE".into()),
                (LineChange::Same, "c".into()),
            ]
        );
        assert_eq!((diff.added, diff.removed), (1, 1));
    }

    #[test]
    fn an_added_paragraph_is_only_an_addition() {
        let diff = diff_text("a\nb\nnew\n".into(), "a\nb\n".into(), 3);
        assert_eq!(diff.added, 1);
        assert_eq!(diff.removed, 0);
    }

    #[test]
    fn line_numbers_point_at_the_right_version() {
        let diff = diff_text("a\nMINE\nc".into(), "a\nTHEIRS\nc".into(), 3);
        let lines: Vec<_> = diff.hunks.iter().flat_map(|h| h.lines.iter()).collect();

        let mine = lines.iter().find(|l| l.text == "MINE").unwrap();
        assert_eq!(mine.local_line, Some(2));
        // Not on disk, so it has no line there.
        assert_eq!(mine.remote_line, None);

        let theirs = lines.iter().find(|l| l.text == "THEIRS").unwrap();
        assert_eq!(theirs.local_line, None);
        assert_eq!(theirs.remote_line, Some(2));
    }

    #[test]
    fn unchanged_text_far_from_a_change_is_collapsed() {
        let local: String = (0..100)
            .map(|n| {
                if n == 50 {
                    "changed".to_string()
                } else {
                    format!("line {n}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let remote: String = (0..100)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");

        let diff = diff_text(local, remote, 2);
        let shown: usize = diff.hunks.iter().map(|h| h.lines.len()).sum();
        // The change, both versions of it, and two lines of context each side.
        assert!(shown <= 8, "showed {shown} lines of a 100-line note");
        assert!(diff.hunks.iter().any(|h| h.has_changes));
    }

    #[test]
    fn several_separate_changes_become_several_hunks() {
        let remote: String = (0..60)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let local: String = (0..60)
            .map(|n| match n {
                5 => "first change".to_string(),
                50 => "second change".to_string(),
                _ => format!("line {n}"),
            })
            .collect::<Vec<_>>()
            .join("\n");

        let diff = diff_text(local, remote, 2);
        let with_changes = diff.hunks.iter().filter(|h| h.has_changes).count();
        assert_eq!(with_changes, 2, "two distant edits should not merge");
    }

    #[test]
    fn an_empty_local_version_removes_everything() {
        let diff = diff_text(String::new(), "a\nb\nc".into(), 3);
        assert_eq!(diff.removed, 3);
        // `"".split('\n')` yields one empty line, which is a real line in a
        // file that is empty — not nothing.
        assert_eq!(diff.added, 1);
    }

    #[test]
    fn both_empty_is_identical_rather_than_a_change() {
        assert!(diff_text(String::new(), String::new(), 3).identical);
    }

    #[test]
    fn a_long_note_with_one_edit_does_not_build_a_large_table() {
        // The prefix and suffix are stripped first, so this is fast rather
        // than quadratic in the note's length. Ten thousand lines would take
        // noticeable time if it were not.
        let remote: String = (0..10_000)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let local = remote.replacen("line 5000", "edited", 1);

        let started = std::time::Instant::now();
        let diff = diff_text(local, remote, 3);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(500),
            "took {:?}",
            started.elapsed()
        );
        assert_eq!((diff.added, diff.removed), (1, 1));
    }

    #[test]
    fn a_reordered_note_is_described_rather_than_rewritten_wholesale() {
        // Moving a paragraph is a delete and an insert, not a rewrite of
        // everything between: an LCS that gave up here would show the whole
        // note as changed and be useless to resolve.
        let remote = "intro\nalpha\nbeta\ngamma\nend".to_string();
        let local = "intro\nbeta\ngamma\nalpha\nend".to_string();
        let diff = diff_text(local, remote, 3);
        assert_eq!(diff.added, 1);
        assert_eq!(diff.removed, 1);
    }
}
