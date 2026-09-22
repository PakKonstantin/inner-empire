//! Parsing and re-rendering the inside of a wiki link.
//!
//! One place owns this grammar, so the scanner, the rename rewriter, the
//! autocomplete backend and the embed resolver cannot drift apart.
//!
//! ```text
//! [[Note]]                     target
//! [[Note|Display]]             target + alias
//! [[Note#Heading]]             target + heading
//! [[Note#Heading|Display]]     target + heading + alias
//! [[Note#^block-id]]           target + block
//! [[#Heading]]                 heading in the current note
//! [[#^block-id]]               block in the current note
//! ```

/// The addressable parts of a link, independent of how it was written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkTarget {
    /// The note being addressed. Empty means "this note", which is how
    /// `[[#Heading]]` works.
    pub path: String,
    pub heading: Option<String>,
    pub block_id: Option<String>,
    pub alias: Option<String>,
}

impl LinkTarget {
    /// Parse the text between `[[` and `]]`.
    pub fn parse(inner: &str) -> Self {
        // The alias runs to the end, so the split is at the *first* pipe:
        // `[[Note|a|b]]` displays "a|b".
        let (locator, alias) = match inner.find('|') {
            Some(idx) => (
                &inner[..idx],
                Some(inner[idx + 1..].trim().to_string()).filter(|s| !s.is_empty()),
            ),
            None => (inner, None),
        };

        let locator = locator.trim();
        let (path, fragment) = match locator.find('#') {
            Some(idx) => (&locator[..idx], Some(&locator[idx + 1..])),
            None => (locator, None),
        };

        let mut target = LinkTarget {
            path: path.trim().to_string(),
            alias,
            ..Default::default()
        };

        match fragment {
            Some(rest) if rest.starts_with('^') => {
                let id = rest[1..].trim();
                if !id.is_empty() {
                    target.block_id = Some(id.to_string());
                }
            }
            // A heading may itself contain '#' for a nested path
            // (`Note#Chapter#Section`), so everything after the first '#' is
            // kept together rather than split again.
            Some(rest) => {
                let heading = rest.trim();
                if !heading.is_empty() {
                    target.heading = Some(heading.to_string());
                }
            }
            None => {}
        }

        target
    }

    /// Render back to the text that belongs between `[[` and `]]`.
    pub fn to_inner(&self) -> String {
        let mut out = self.path.clone();
        if let Some(block) = &self.block_id {
            out.push_str("#^");
            out.push_str(block);
        } else if let Some(heading) = &self.heading {
            out.push('#');
            out.push_str(heading);
        }
        if let Some(alias) = &self.alias {
            out.push('|');
            out.push_str(alias);
        }
        out
    }

    /// Render the whole link, as an embed or a plain wiki link.
    pub fn to_wikilink(&self, embed: bool) -> String {
        let prefix = if embed { "![[" } else { "[[" };
        format!("{prefix}{}]]", self.to_inner())
    }

    /// Does this address a location inside the note that contains it?
    pub fn is_same_note(&self) -> bool {
        self.path.is_empty()
    }

    /// What the reader sees when no alias is given: the most specific part.
    pub fn default_display(&self) -> String {
        if let Some(alias) = &self.alias {
            return alias.clone();
        }
        if self.path.is_empty() {
            if let Some(heading) = &self.heading {
                return heading.clone();
            }
            if let Some(block) = &self.block_id {
                return block.clone();
            }
        }
        self.path.clone()
    }
}

/// Turn a heading into a stable anchor.
///
/// Used for `[[Note#Heading]]` matching and for the `id` attribute in exported
/// HTML, so a link in an exported page lands where the same link lands in the
/// app.
pub fn slugify(heading: &str) -> String {
    let mut out = String::with_capacity(heading.len());
    let mut last_was_dash = false;
    for ch in heading.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
            last_was_dash = false;
        } else if !out.is_empty() && !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Compare heading text the way link resolution does: case-insensitively and
/// ignoring the punctuation and spacing differences that trip users up.
pub fn heading_matches(candidate: &str, wanted: &str) -> bool {
    candidate.eq_ignore_ascii_case(wanted) || slugify(candidate) == slugify(wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_target() {
        let t = LinkTarget::parse("Note");
        assert_eq!(t.path, "Note");
        assert_eq!(t.alias, None);
        assert_eq!(t.default_display(), "Note");
    }

    #[test]
    fn target_with_alias() {
        let t = LinkTarget::parse("Note|Display text");
        assert_eq!(t.path, "Note");
        assert_eq!(t.alias.as_deref(), Some("Display text"));
        assert_eq!(t.default_display(), "Display text");
    }

    #[test]
    fn target_with_heading() {
        let t = LinkTarget::parse("Note#Heading");
        assert_eq!(t.path, "Note");
        assert_eq!(t.heading.as_deref(), Some("Heading"));
        assert_eq!(t.block_id, None);
    }

    #[test]
    fn target_with_heading_and_alias() {
        let t = LinkTarget::parse("Note#Heading|Display");
        assert_eq!(t.path, "Note");
        assert_eq!(t.heading.as_deref(), Some("Heading"));
        assert_eq!(t.alias.as_deref(), Some("Display"));
    }

    #[test]
    fn target_with_block_reference() {
        let t = LinkTarget::parse("Note#^important-point");
        assert_eq!(t.path, "Note");
        assert_eq!(t.block_id.as_deref(), Some("important-point"));
        assert_eq!(t.heading, None);
    }

    #[test]
    fn a_nested_heading_path_keeps_its_inner_hashes() {
        let t = LinkTarget::parse("Note#Chapter#Section");
        assert_eq!(t.path, "Note");
        assert_eq!(t.heading.as_deref(), Some("Chapter#Section"));
    }

    #[test]
    fn an_alias_may_itself_contain_a_pipe() {
        let t = LinkTarget::parse("Note|a|b");
        assert_eq!(t.path, "Note");
        assert_eq!(t.alias.as_deref(), Some("a|b"));
    }

    #[test]
    fn same_note_references_have_an_empty_path() {
        let heading = LinkTarget::parse("#Heading");
        assert!(heading.is_same_note());
        assert_eq!(heading.heading.as_deref(), Some("Heading"));
        assert_eq!(heading.default_display(), "Heading");

        let block = LinkTarget::parse("#^abc");
        assert!(block.is_same_note());
        assert_eq!(block.block_id.as_deref(), Some("abc"));
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        let t = LinkTarget::parse("  Note  #  Heading  |  Display  ");
        assert_eq!(t.path, "Note");
        assert_eq!(t.heading.as_deref(), Some("Heading"));
        assert_eq!(t.alias.as_deref(), Some("Display"));
    }

    #[test]
    fn every_form_round_trips_through_rendering() {
        for inner in [
            "Note",
            "Note|Display",
            "Note#Heading",
            "Note#Heading|Display",
            "Note#^block-id",
            "Note#^block-id|Display",
            "#Heading",
            "Folder/Sub/Note#Heading|Display",
        ] {
            let parsed = LinkTarget::parse(inner);
            assert_eq!(parsed.to_inner(), inner, "round trip failed for {inner}");
            assert_eq!(parsed.to_wikilink(false), format!("[[{inner}]]"));
            assert_eq!(parsed.to_wikilink(true), format!("![[{inner}]]"));
        }
    }

    #[test]
    fn slugs_are_lowercase_and_hyphenated() {
        assert_eq!(slugify("Getting Started"), "getting-started");
        assert_eq!(slugify("What's *this*?"), "what-s-this");
        assert_eq!(slugify("  Trailing  "), "trailing");
        assert_eq!(slugify("Section 2.1"), "section-2-1");
    }

    #[test]
    fn heading_matching_tolerates_case_and_punctuation() {
        assert!(heading_matches("Getting Started", "getting started"));
        assert!(heading_matches("What's this?", "What-s-this"));
        assert!(!heading_matches("Getting Started", "Getting Finished"));
    }
}
