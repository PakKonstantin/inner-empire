//! The link resolution surface the application layer uses.
//!
//! A thin, typed face over `index::resolve` that adds the parts resolution
//! needs but the index does not know about: which heading or block inside the
//! target a link addresses, and what to offer when nothing matches.

use rusqlite::Connection;

use crate::error::Result;
use crate::index::{queries, resolve};
use crate::links::reference::{heading_matches, LinkTarget};
use crate::model::Link;
use crate::vault::path::VaultPath;

/// Where a link leads.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum ResolutionOutcome {
    /// The target exists.
    #[serde(rename_all = "camelCase")]
    Resolved {
        path: VaultPath,
        /// Line to scroll to, when the link addressed a heading or block that
        /// was found.
        line: Option<usize>,
        /// The link named a heading or block the target does not have. The note
        /// still opens; the UI says the anchor is missing rather than
        /// pretending it worked.
        anchor_missing: bool,
        ambiguous: bool,
    },
    /// Nothing answers to it. Carries the name a new note would get.
    #[serde(rename_all = "camelCase")]
    Unresolved { suggested_name: String },
    /// An `http(s)` destination, for the shell to open.
    #[serde(rename_all = "camelCase")]
    External { url: String },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkResolution {
    pub target: String,
    #[serde(flatten)]
    pub outcome: ResolutionOutcome,
}

pub struct LinkResolver;

impl LinkResolver {
    /// Resolve a link as written, from the note that contains it.
    pub fn resolve(conn: &Connection, from: &VaultPath, link: &Link) -> Result<LinkResolution> {
        if link.kind == crate::model::LinkKind::External {
            return Ok(LinkResolution {
                target: link.target.clone(),
                outcome: ResolutionOutcome::External {
                    url: link.target.clone(),
                },
            });
        }

        // `[[#Heading]]` addresses the note it is written in.
        let path = if link.target.is_empty() {
            Some(from.clone())
        } else {
            match resolve::resolve(conn, &link.target)? {
                resolve::Resolution::Resolved { path, .. } => Some(path),
                resolve::Resolution::Unresolved => None,
            }
        };

        let Some(path) = path else {
            return Ok(LinkResolution {
                target: link.target.clone(),
                outcome: ResolutionOutcome::Unresolved {
                    suggested_name: crate::vault::path::sanitize_segment(&link.target),
                },
            });
        };

        let ambiguous = if link.target.is_empty() {
            false
        } else {
            resolve::resolve(conn, &link.target)?.is_ambiguous()
        };

        let (line, anchor_missing) = Self::locate_anchor(conn, &path, link)?;
        Ok(LinkResolution {
            target: link.target.clone(),
            outcome: ResolutionOutcome::Resolved {
                path,
                line,
                anchor_missing,
                ambiguous,
            },
        })
    }

    /// Resolve a raw target string, for the UI's "follow link" action.
    pub fn resolve_target(
        conn: &Connection,
        from: &VaultPath,
        raw: &str,
    ) -> Result<LinkResolution> {
        let parsed = LinkTarget::parse(raw);
        let link = Link {
            kind: crate::model::LinkKind::WikiLink,
            raw: format!("[[{raw}]]"),
            target: parsed.path.clone(),
            heading: parsed.heading.clone(),
            block_id: parsed.block_id.clone(),
            alias: parsed.alias.clone(),
            line: 0,
            byte_start: 0,
            byte_end: 0,
        };
        Self::resolve(conn, from, &link)
    }

    /// Which line a heading or block reference points at.
    fn locate_anchor(
        conn: &Connection,
        path: &VaultPath,
        link: &Link,
    ) -> Result<(Option<usize>, bool)> {
        if let Some(block_id) = &link.block_id {
            let found = queries::blocks(conn, path)?
                .into_iter()
                .find(|b| b.id == *block_id);
            return Ok(match found {
                Some(block) => (Some(block.line), false),
                None => (None, true),
            });
        }

        if let Some(heading) = &link.heading {
            let found = queries::headings(conn, path)?
                .into_iter()
                .find(|h| heading_matches(&h.text, heading));
            return Ok(match found {
                Some(h) => (Some(h.line), false),
                None => (None, true),
            });
        }

        Ok((None, false))
    }
}
