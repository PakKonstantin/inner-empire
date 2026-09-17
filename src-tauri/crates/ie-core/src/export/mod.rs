//! Export and import.
//!
//! Export turns notes into something a person who does not have the app can
//! read: HTML with working links, or Markdown with links rewritten to relative
//! paths. Import copies files in and normalises their names so a folder
//! authored elsewhere becomes a usable part of the vault.

use std::collections::HashMap;

use crate::error::Result;
use crate::index::{queries, resolve};
use crate::markdown::render::{MarkdownRenderer, RenderOptions};
use crate::markdown::MarkdownParser;
use crate::model::LinkKind;
use crate::vault::path::VaultPath;
use crate::vault::{Collision, FileOps};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Markdown,
    Html,
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub format: ExportFormat,
    /// Follow links and export what they point at too.
    pub include_linked_notes: bool,
    /// Copy referenced images and other attachments alongside the output.
    pub include_attachments: bool,
    pub include_properties: bool,
    /// How many hops of linked notes to follow.
    pub depth: usize,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            format: ExportFormat::Html,
            include_linked_notes: false,
            include_attachments: true,
            include_properties: true,
            depth: 1,
        }
    }
}

/// One file the export produced.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedFile {
    /// Path relative to the export root.
    pub relative_path: String,
    pub source: Option<VaultPath>,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub files: Vec<ExportedFile>,
    /// Links that pointed at nothing, so the caller can report them instead of
    /// producing a page full of dead anchors without saying so.
    pub unresolved_links: Vec<String>,
}

/// Export one note and, optionally, what it links to.
///
/// Writing is delegated: this returns the rendered content keyed by output
/// path, so the caller decides where it lands and the core stays free of any
/// notion of a destination outside the vault.
pub fn export_notes(
    conn: &rusqlite::Connection,
    ops: &FileOps,
    roots: &[VaultPath],
    options: &ExportOptions,
) -> Result<(ExportResult, HashMap<String, Vec<u8>>)> {
    let mut selected: Vec<VaultPath> = Vec::new();
    let mut frontier: Vec<VaultPath> = roots.to_vec();
    let mut attachments: Vec<VaultPath> = Vec::new();

    let hops = if options.include_linked_notes {
        options.depth
    } else {
        0
    };

    for hop in 0..=hops {
        let mut next: Vec<VaultPath> = Vec::new();
        for path in std::mem::take(&mut frontier) {
            if selected.contains(&path) {
                continue;
            }
            selected.push(path.clone());

            for link in queries::outgoing_links(conn, &path)? {
                let Some(target) = link.target_path else {
                    continue;
                };
                let is_note = matches!(target.extension().as_deref(), Some("md" | "markdown"));
                if is_note {
                    if hop < hops && !selected.contains(&target) {
                        next.push(target);
                    }
                } else if options.include_attachments && !attachments.contains(&target) {
                    attachments.push(target);
                }
            }
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }

    // Every exported note, keyed by target text, so links between them point at
    // the files actually being written.
    let mut resolutions: HashMap<String, VaultPath> = HashMap::new();
    let parser = MarkdownParser::new();
    let mut unresolved: Vec<String> = Vec::new();

    for path in selected.iter().chain(attachments.iter()) {
        resolutions.insert(path.as_str().to_string(), path.clone());
        resolutions.insert(path.stem().to_string(), path.clone());
        resolutions.insert(path.file_name().to_string(), path.clone());
    }

    let mut output: HashMap<String, Vec<u8>> = HashMap::new();
    let mut files: Vec<ExportedFile> = Vec::new();

    for path in &selected {
        let source = ops.read(path)?;

        for link in parser.parse(&source).metadata.links {
            if link.kind == LinkKind::External || link.target.is_empty() {
                continue;
            }
            let known = resolutions.contains_key(&link.target)
                || resolve::resolve(conn, &link.target)?.file_id().is_some();
            if !known && !unresolved.contains(&link.target) {
                unresolved.push(link.target.clone());
            }
        }

        let (relative_path, bytes) = match options.format {
            ExportFormat::Markdown => (path.as_str().to_string(), source.into_bytes()),
            ExportFormat::Html => {
                let renderer = MarkdownRenderer::new(RenderOptions {
                    include_properties: options.include_properties,
                    standalone: true,
                    document_title: title_for(conn, path),
                    internal_link_extension: Some("html".into()),
                    ..RenderOptions::default()
                })
                .with_resolutions(resolutions.clone());
                let html = renderer.render(&source);
                let out_path = path
                    .with_extension(Some("html"))
                    .map(|p| p.as_str().to_string())
                    .unwrap_or_else(|_| format!("{}.html", path.as_str()));
                (out_path, html.into_bytes())
            }
        };

        output.insert(relative_path.clone(), bytes);
        files.push(ExportedFile {
            relative_path,
            source: Some(path.clone()),
        });
    }

    for path in &attachments {
        let bytes = ops.read_bytes(path)?;
        output.insert(path.as_str().to_string(), bytes);
        files.push(ExportedFile {
            relative_path: path.as_str().to_string(),
            source: Some(path.clone()),
        });
    }

    Ok((
        ExportResult {
            files,
            unresolved_links: unresolved,
        },
        output,
    ))
}

fn title_for(conn: &rusqlite::Connection, path: &VaultPath) -> String {
    queries::file(conn, path)
        .ok()
        .flatten()
        .map(|f| f.title)
        .unwrap_or_else(|| path.stem().to_string())
}

/// A file offered for import.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    /// Where it will land in the vault.
    pub target: VaultPath,
    /// The name it had, when sanitising changed it.
    pub original_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: Vec<ImportItem>,
    pub skipped: Vec<String>,
}

/// Copy external files into the vault under `folder`.
///
/// Names are sanitised so a folder authored on Linux with characters Windows
/// rejects becomes a vault that still opens on Windows.
pub fn import_files(
    ops: &FileOps,
    folder: &VaultPath,
    sources: &[(String, Vec<u8>)],
) -> Result<ImportResult> {
    let mut result = ImportResult::default();

    for (name, bytes) in sources {
        let safe = crate::vault::path::sanitize_segment(name);
        let original_name = if safe == *name {
            None
        } else {
            Some(name.clone())
        };

        let target = match folder.join(&safe) {
            Ok(path) => path,
            Err(_) => {
                result.skipped.push(name.clone());
                continue;
            }
        };

        match ops.create_note(&target, "", Collision::Rename) {
            Ok(final_path) => {
                ops.write_bytes(&final_path, bytes)?;
                result.imported.push(ImportItem {
                    target: final_path,
                    original_name,
                });
            }
            Err(_) => result.skipped.push(name.clone()),
        }
    }

    Ok(result)
}
