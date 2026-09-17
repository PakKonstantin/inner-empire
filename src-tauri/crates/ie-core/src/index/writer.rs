//! Writing one file's derived data into the index.
//!
//! Everything here happens inside a caller-supplied transaction. A file is
//! either fully indexed or not indexed at all: a crash in the middle leaves
//! the previous row intact rather than a half-updated one.

use rusqlite::{params, Transaction};

use crate::error::Result;
use crate::index::resolve::{self, Resolution};
use crate::markdown::frontmatter;
use crate::model::{FileKind, NoteMetadata};
use crate::vault::path::VaultPath;

/// What the indexer knows about a file before parsing it.
#[derive(Debug, Clone)]
pub struct FileRecord {
    pub path: VaultPath,
    pub kind: FileKind,
    pub size: u64,
    pub mtime_ms: i64,
    /// Present for notes; attachments are identified by size and mtime alone,
    /// because hashing a 200 MB video on every scan is not worth the precision.
    pub content_hash: Option<String>,
    pub indexed_at: i64,
}

/// Insert or update the `files` row, returning its id.
pub fn upsert_file(
    tx: &Transaction<'_>,
    record: &FileRecord,
    title: Option<&str>,
    word_count: usize,
) -> Result<i64> {
    let path = record.path.as_str();
    tx.execute(
        "INSERT INTO files(path, path_fold, name, name_fold, stem_fold, ext, kind,
                           size, mtime_ms, content_hash, title, word_count, indexed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(path) DO UPDATE SET
             path_fold    = excluded.path_fold,
             name         = excluded.name,
             name_fold    = excluded.name_fold,
             stem_fold    = excluded.stem_fold,
             ext          = excluded.ext,
             kind         = excluded.kind,
             size         = excluded.size,
             mtime_ms     = excluded.mtime_ms,
             content_hash = excluded.content_hash,
             title        = excluded.title,
             word_count   = excluded.word_count,
             indexed_at   = excluded.indexed_at",
        params![
            path,
            record.path.fold(),
            record.path.file_name(),
            record.path.file_name().to_lowercase(),
            record.path.stem().to_lowercase(),
            record.path.extension(),
            record.kind.as_str(),
            record.size as i64,
            record.mtime_ms,
            record.content_hash,
            title,
            word_count as i64,
            record.indexed_at,
        ],
    )?;

    let file_id: i64 = tx.query_row("SELECT id FROM files WHERE path = ?1", [path], |row| {
        row.get(0)
    })?;
    Ok(file_id)
}

/// Replace every derived row for a note, then re-resolve its outgoing links.
pub fn index_note(
    tx: &Transaction<'_>,
    record: &FileRecord,
    metadata: &NoteMetadata,
    source: &str,
) -> Result<i64> {
    let title = metadata
        .title
        .clone()
        .unwrap_or_else(|| record.path.stem().to_string());
    let file_id = upsert_file(tx, record, Some(&title), metadata.word_count)?;

    clear_derived(tx, file_id)?;
    write_headings(tx, file_id, metadata)?;
    write_blocks(tx, file_id, metadata)?;
    write_tags(tx, file_id, metadata)?;
    write_properties(tx, file_id, metadata)?;
    write_links(tx, file_id, metadata)?;
    write_fts(tx, file_id, &record.path, &title, source, metadata)?;

    Ok(file_id)
}

/// Record a non-note file. Attachments are indexed so links to them resolve,
/// but nothing is parsed out of them.
pub fn index_attachment(tx: &Transaction<'_>, record: &FileRecord) -> Result<i64> {
    let title = record.path.stem().to_string();
    let file_id = upsert_file(tx, record, Some(&title), 0)?;
    clear_derived(tx, file_id)?;
    tx.execute("DELETE FROM notes_fts WHERE rowid = ?1", [file_id])?;
    Ok(file_id)
}

fn clear_derived(tx: &Transaction<'_>, file_id: i64) -> Result<()> {
    for table in ["headings", "blocks", "tags", "properties", "links"] {
        tx.execute(&format!("DELETE FROM {table} WHERE file_id = ?1"), [file_id])?;
    }
    Ok(())
}

fn write_headings(tx: &Transaction<'_>, file_id: i64, metadata: &NoteMetadata) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO headings(file_id, ordinal, level, text, slug, line, byte_start)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for (ordinal, heading) in metadata.headings.iter().enumerate() {
        stmt.execute(params![
            file_id,
            ordinal as i64,
            heading.level as i64,
            heading.text,
            heading.slug,
            heading.line as i64,
            heading.byte_start as i64,
        ])?;
    }
    Ok(())
}

fn write_blocks(tx: &Transaction<'_>, file_id: i64, metadata: &NoteMetadata) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO blocks(file_id, block_id, line, byte_start, byte_end)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(file_id, block_id) DO UPDATE SET
             line = excluded.line, byte_start = excluded.byte_start, byte_end = excluded.byte_end",
    )?;
    for block in &metadata.blocks {
        stmt.execute(params![
            file_id,
            block.id,
            block.line as i64,
            block.byte_start as i64,
            block.byte_end as i64,
        ])?;
    }
    Ok(())
}

fn write_tags(tx: &Transaction<'_>, file_id: i64, metadata: &NoteMetadata) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO tags(file_id, tag, tag_fold, source, line, byte_start)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;

    for tag in &metadata.tags {
        stmt.execute(params![
            file_id,
            tag.name,
            tag.name.to_lowercase(),
            "body",
            tag.line as i64,
            tag.byte_start as i64,
        ])?;
    }

    // Frontmatter tags count too, and the tag explorer must not distinguish
    // them; only the editor does, when deciding whether it can jump to one.
    for tag in frontmatter::tags(&metadata.properties) {
        stmt.execute(params![
            file_id,
            tag,
            tag.to_lowercase(),
            "frontmatter",
            0i64,
            0i64,
        ])?;
    }
    Ok(())
}

fn write_properties(tx: &Transaction<'_>, file_id: i64, metadata: &NoteMetadata) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO properties(file_id, key, key_fold, kind, text_value, text_fold, num_value, list_index, ordinal)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for (ordinal, property) in metadata.properties.iter().enumerate() {
        // A list becomes one row per item so `status:active` and `tag:AI` are
        // the same shape of query.
        for (list_index, scalar) in property.value.flatten_scalars() {
            let text = scalar.as_text();
            stmt.execute(params![
                file_id,
                property.key,
                property.key.to_lowercase(),
                scalar.kind().as_str(),
                text,
                text.to_lowercase(),
                scalar.as_number(),
                list_index.map(|i| i as i64),
                ordinal as i64,
            ])?;
        }
    }
    Ok(())
}

fn write_links(tx: &Transaction<'_>, file_id: i64, metadata: &NoteMetadata) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO links(file_id, kind, raw, target_text, target_fold, target_tail,
                           target_file_id, heading, block_id, alias, line, byte_start, byte_end, ambiguous)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    )?;

    for link in &metadata.links {
        let resolution = if link.kind.is_internal() && !link.target.is_empty() {
            resolve::resolve(tx, &link.target)?
        } else {
            Resolution::Unresolved
        };

        stmt.execute(params![
            file_id,
            link.kind.as_str(),
            link.raw,
            link.target,
            resolve::normalize_target(&link.target).to_lowercase(),
            resolve::target_tail(&link.target),
            resolution.file_id(),
            link.heading,
            link.block_id,
            link.alias,
            link.line as i64,
            link.byte_start as i64,
            link.byte_end as i64,
            resolution.is_ambiguous() as i64,
        ])?;
    }
    Ok(())
}

fn write_fts(
    tx: &Transaction<'_>,
    file_id: i64,
    path: &VaultPath,
    title: &str,
    source: &str,
    metadata: &NoteMetadata,
) -> Result<()> {
    tx.execute("DELETE FROM notes_fts WHERE rowid = ?1", [file_id])?;
    // Index the body only. Frontmatter is searchable through property filters,
    // and including its YAML would make every note match words like "tags".
    let body = &source[metadata.frontmatter_bytes.min(source.len())..];
    tx.execute(
        "INSERT INTO notes_fts(rowid, path, title, body) VALUES (?1, ?2, ?3, ?4)",
        params![file_id, path.as_str(), title, body],
    )?;
    Ok(())
}

/// Remove a file and everything derived from it.
///
/// Links *pointing at* it survive as unresolved rows: the text is still in the
/// referring note, and the user needs to see that it now points nowhere.
pub fn remove_file(tx: &Transaction<'_>, path: &VaultPath) -> Result<Option<i64>> {
    let file_id: Option<i64> = tx
        .query_row("SELECT id FROM files WHERE path = ?1", [path.as_str()], |row| {
            row.get(0)
        })
        .ok();

    let Some(file_id) = file_id else {
        return Ok(None);
    };

    tx.execute("DELETE FROM notes_fts WHERE rowid = ?1", [file_id])?;
    tx.execute("DELETE FROM files WHERE id = ?1", [file_id])?;

    // `ON DELETE SET NULL` has just unresolved every inbound link. Some of them
    // may now match a different file with the same name, so give them another
    // chance rather than leaving them broken.
    reresolve_pending(tx, &path.stem().to_lowercase())?;
    Ok(Some(file_id))
}

/// Give unresolved links whose tail matches `tail` another chance to resolve.
///
/// Called after any change to the set of files: adding `Project Plan.md` must
/// light up every `[[Project Plan]]` already written elsewhere in the vault,
/// without rescanning those notes.
pub fn reresolve_pending(tx: &Transaction<'_>, tail: &str) -> Result<usize> {
    let pending: Vec<(i64, String)> = {
        let mut stmt = tx.prepare_cached(
            "SELECT id, target_text FROM links
             WHERE target_file_id IS NULL AND target_tail = ?1",
        )?;
        let rows = stmt
            .query_map([tail], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(std::result::Result::ok)
            .collect();
        rows
    };

    let mut updated = 0usize;
    for (link_id, target) in pending {
        let resolution = resolve::resolve(tx, &target)?;
        if let Resolution::Resolved {
            file_id, ambiguous, ..
        } = resolution
        {
            tx.execute(
                "UPDATE links SET target_file_id = ?1, ambiguous = ?2 WHERE id = ?3",
                params![file_id, ambiguous as i64, link_id],
            )?;
            updated += 1;
        }
    }
    Ok(updated)
}

/// Re-resolve every link in the vault. Used after a bulk change, such as the
/// initial scan finishing, where resolving as we went would have missed targets
/// that had not been indexed yet.
pub fn resolve_all_pending(tx: &Transaction<'_>) -> Result<usize> {
    let pending: Vec<(i64, String)> = {
        let mut stmt = tx.prepare_cached(
            "SELECT id, target_text FROM links WHERE target_file_id IS NULL",
        )?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(std::result::Result::ok)
            .collect();
        rows
    };

    let mut updated = 0usize;
    for (link_id, target) in pending {
        if let Resolution::Resolved {
            file_id, ambiguous, ..
        } = resolve::resolve(tx, &target)?
        {
            tx.execute(
                "UPDATE links SET target_file_id = ?1, ambiguous = ?2 WHERE id = ?3",
                params![file_id, ambiguous as i64, link_id],
            )?;
            updated += 1;
        }
    }
    Ok(updated)
}

/// Record a scan diagnostic.
pub fn write_diagnostic(tx: &Transaction<'_>, diagnostic: &crate::error::Diagnostic) -> Result<()> {
    tx.execute(
        "INSERT INTO diagnostics(payload) VALUES (?1)",
        [serde_json::to_string(diagnostic)?],
    )?;
    Ok(())
}

pub fn clear_diagnostics(tx: &Transaction<'_>) -> Result<()> {
    tx.execute("DELETE FROM diagnostics", [])?;
    Ok(())
}

/// Content hash used to detect real changes and to recognise a moved file.
pub fn hash_content(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    // 128 bits is far more than enough to distinguish notes, and halves the
    // space every row spends on this.
    digest[..16].iter().map(|b| format!("{b:02x}")).collect()
}
