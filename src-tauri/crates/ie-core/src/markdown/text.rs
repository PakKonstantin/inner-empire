//! Byte-offset and line bookkeeping shared by the parser, the scanner and the
//! transformer.

use std::ops::Range;

/// Maps byte offsets to line numbers in one pass, so converting a scanner hit
/// into a line number is a binary search rather than a re-count.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset at which each line starts.
    starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut starts = vec![0usize];
        for (idx, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(idx + 1);
            }
        }
        Self {
            starts,
            len: source.len(),
        }
    }

    /// Zero-based line containing `offset`.
    pub fn line_of(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(next) => next.saturating_sub(1),
        }
    }

    /// Byte range of a line, excluding its terminator.
    pub fn line_range(&self, line: usize) -> Range<usize> {
        let start = self.starts.get(line).copied().unwrap_or(self.len);
        let end = self
            .starts
            .get(line + 1)
            .map(|next| next.saturating_sub(1))
            .unwrap_or(self.len);
        start..end
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// The text of a line with any trailing carriage return removed, so the
    /// same source behaves identically whether it was saved on Windows or
    /// Linux.
    pub fn line_text<'a>(&self, source: &'a str, line: usize) -> &'a str {
        let range = self.line_range(line);
        source
            .get(range)
            .unwrap_or("")
            .trim_end_matches('\r')
    }
}

/// Byte ranges the extension scanner must not look inside: code, HTML, and
/// already-recognised constructs.
///
/// Without this, a shell snippet containing `#!/bin/sh` would register a tag
/// named `!/bin/sh`, and a code sample containing `[[x]]` would become a link.
#[derive(Debug, Clone, Default)]
pub struct ExclusionZones {
    ranges: Vec<Range<usize>>,
}

impl ExclusionZones {
    pub fn new(mut ranges: Vec<Range<usize>>) -> Self {
        ranges.retain(|r| r.start < r.end);
        ranges.sort_by_key(|r| (r.start, r.end));

        // Merge overlaps so membership is a single binary search.
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
        for range in ranges {
            match merged.last_mut() {
                Some(last) if range.start <= last.end => {
                    last.end = last.end.max(range.end);
                }
                _ => merged.push(range),
            }
        }
        Self { ranges: merged }
    }

    pub fn covers(&self, offset: usize) -> bool {
        self.ranges
            .binary_search_by(|range| {
                if offset < range.start {
                    std::cmp::Ordering::Greater
                } else if offset >= range.end {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Add more ranges, returning a new set. Used to feed recognised wiki links
    /// into the tag scan so `[[Note#Heading]]` does not yield a `#Heading` tag.
    pub fn extended(&self, extra: impl IntoIterator<Item = Range<usize>>) -> Self {
        let mut ranges = self.ranges.clone();
        ranges.extend(extra);
        Self::new(ranges)
    }
}

/// Count words the way a status bar should: runs of non-whitespace, ignoring
/// pure punctuation runs such as a `---` rule.
pub fn count_words(text: &str) -> usize {
    text.split_whitespace()
        .filter(|token| token.chars().any(char::is_alphanumeric))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_lookup_finds_the_right_line_for_every_offset() {
        let source = "one\ntwo\nthree";
        let index = LineIndex::new(source);
        assert_eq!(index.line_count(), 3);
        assert_eq!(index.line_of(0), 0);
        assert_eq!(index.line_of(3), 0);
        assert_eq!(index.line_of(4), 1);
        assert_eq!(index.line_of(8), 2);
        assert_eq!(index.line_of(source.len()), 2);
    }

    #[test]
    fn line_ranges_exclude_the_terminator() {
        let source = "one\ntwo\n";
        let index = LineIndex::new(source);
        assert_eq!(&source[index.line_range(0)], "one");
        assert_eq!(&source[index.line_range(1)], "two");
    }

    #[test]
    fn carriage_returns_are_stripped_from_line_text() {
        let source = "one\r\ntwo\r\n";
        let index = LineIndex::new(source);
        assert_eq!(index.line_text(source, 0), "one");
        assert_eq!(index.line_text(source, 1), "two");
    }

    #[test]
    fn exclusion_zones_merge_and_answer_membership() {
        let zones = ExclusionZones::new(vec![0..5, 3..8, 20..25]);
        assert!(zones.covers(0));
        assert!(zones.covers(7));
        assert!(!zones.covers(8));
        assert!(!zones.covers(19));
        assert!(zones.covers(24));
        assert!(!zones.covers(25));
    }

    #[test]
    fn empty_ranges_are_discarded() {
        assert!(ExclusionZones::new(vec![5..5]).is_empty());
    }

    #[test]
    fn word_count_ignores_punctuation_only_runs() {
        assert_eq!(count_words("one two three"), 3);
        assert_eq!(count_words("one --- two"), 2);
        assert_eq!(count_words("   "), 0);
        assert_eq!(count_words("don't stop"), 2);
    }
}
