//! Index schema and migrations.
//!
//! Every table here is derived data. Nothing in this file is the source of
//! truth for anything: delete `index.db` and a rescan reproduces it exactly
//! (invariant I2). That is why the migration strategy for a version mismatch
//! is "throw it away and rebuild" rather than a careful `ALTER TABLE` — the
//! data is free to regenerate, and a rebuild cannot be wrong.

use rusqlite::Connection;

use crate::error::Result;

/// Bump this whenever the shape below changes. A database carrying a different
/// number is discarded and rebuilt.
pub const SCHEMA_VERSION: i64 = 1;

pub const META_SCHEMA_VERSION: &str = "schema_version";
pub const META_VAULT_ID: &str = "vault_id";
pub const META_LAST_FULL_SCAN: &str = "last_full_scan_ms";

/// Connection settings applied to every connection.
///
/// WAL matters for responsiveness: the indexer writes on a background thread
/// while the UI reads, and without WAL every read would block behind the
/// current write transaction.
pub fn apply_pragmas(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // NORMAL rather than FULL: losing the last few index writes to a power cut
    // costs a partial rescan, never user data, and FULL would fsync on every
    // note save.
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    // 64 MiB page cache. Large enough that a 50k-note vault's hot indexes stay
    // resident, small enough not to matter on a modest machine.
    conn.pragma_update(None, "cache_size", -64_000)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

pub const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;

-- One row per file in the vault, including attachments: links must resolve to
-- images and PDFs too, so every file is tracked even though only notes are
-- parsed.
CREATE TABLE IF NOT EXISTS files (
    id           INTEGER PRIMARY KEY,
    path         TEXT NOT NULL UNIQUE,
    -- Lowercased path. Two rows sharing this value are the case collision that
    -- would make the vault unusable on NTFS.
    path_fold    TEXT NOT NULL,
    name         TEXT NOT NULL,
    name_fold    TEXT NOT NULL,
    -- Filename without extension, lowercased: the key wiki links match on.
    stem_fold    TEXT NOT NULL,
    ext          TEXT,
    kind         TEXT NOT NULL,
    size         INTEGER NOT NULL,
    mtime_ms     INTEGER NOT NULL,
    -- Settles the case where mtime granularity hides a change, and identifies
    -- a move as the same content appearing under a new path.
    content_hash TEXT,
    title        TEXT,
    word_count   INTEGER NOT NULL DEFAULT 0,
    indexed_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS files_path_fold ON files(path_fold);
CREATE INDEX IF NOT EXISTS files_stem_fold ON files(stem_fold);
CREATE INDEX IF NOT EXISTS files_name_fold ON files(name_fold);
CREATE INDEX IF NOT EXISTS files_kind      ON files(kind);
CREATE INDEX IF NOT EXISTS files_hash      ON files(content_hash);
CREATE INDEX IF NOT EXISTS files_mtime     ON files(mtime_ms DESC);

CREATE TABLE IF NOT EXISTS headings (
    file_id    INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    ordinal    INTEGER NOT NULL,
    level      INTEGER NOT NULL,
    text       TEXT NOT NULL,
    slug       TEXT NOT NULL,
    line       INTEGER NOT NULL,
    byte_start INTEGER NOT NULL,
    PRIMARY KEY (file_id, ordinal)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS headings_slug ON headings(slug);

CREATE TABLE IF NOT EXISTS blocks (
    file_id    INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    block_id   TEXT NOT NULL,
    line       INTEGER NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_end   INTEGER NOT NULL,
    PRIMARY KEY (file_id, block_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS blocks_id ON blocks(block_id);

CREATE TABLE IF NOT EXISTS links (
    id             INTEGER PRIMARY KEY,
    file_id        INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    kind           TEXT NOT NULL,
    raw            TEXT NOT NULL,
    target_text    TEXT NOT NULL,
    target_fold    TEXT NOT NULL,
    -- Final path segment of the target, lowercased and stripped of its
    -- extension. Indexed so that adding a file can find, in one query, every
    -- unresolved link that might now point at it.
    target_tail    TEXT NOT NULL,
    -- NULL means unresolved: the link is written but no file answers to it.
    target_file_id INTEGER REFERENCES files(id) ON DELETE SET NULL,
    heading        TEXT,
    block_id       TEXT,
    alias          TEXT,
    line           INTEGER NOT NULL,
    byte_start     INTEGER NOT NULL,
    byte_end       INTEGER NOT NULL,
    -- Several files answered to this target equally well. Resolved to the best
    -- of them, but flagged so the user can disambiguate.
    ambiguous      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS links_file        ON links(file_id);
CREATE INDEX IF NOT EXISTS links_target_file ON links(target_file_id);
CREATE INDEX IF NOT EXISTS links_target_fold ON links(target_fold);
CREATE INDEX IF NOT EXISTS links_target_tail ON links(target_tail);
CREATE INDEX IF NOT EXISTS links_unresolved  ON links(target_file_id, target_fold)
    WHERE target_file_id IS NULL;

CREATE TABLE IF NOT EXISTS tags (
    file_id    INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    tag        TEXT NOT NULL,
    tag_fold   TEXT NOT NULL,
    -- 'body' or 'frontmatter'; the tag explorer shows both but the editor only
    -- knows how to jump to body occurrences.
    source     TEXT NOT NULL,
    line       INTEGER NOT NULL,
    byte_start INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS tags_file ON tags(file_id);
CREATE INDEX IF NOT EXISTS tags_fold ON tags(tag_fold);

-- One row per scalar. A list property becomes one row per item, which is what
-- makes `tag:AI` and `status:active` the same kind of query.
CREATE TABLE IF NOT EXISTS properties (
    file_id    INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    key        TEXT NOT NULL,
    key_fold   TEXT NOT NULL,
    kind       TEXT NOT NULL,
    text_value TEXT,
    text_fold  TEXT,
    num_value  REAL,
    list_index INTEGER,
    ordinal    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS properties_file     ON properties(file_id);
CREATE INDEX IF NOT EXISTS properties_key_text ON properties(key_fold, text_fold);
CREATE INDEX IF NOT EXISTS properties_key_num  ON properties(key_fold, num_value);

-- Full-text search. `rowid` is deliberately `files.id` so a match joins
-- straight back to the file with no lookup table.
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
    path,
    title,
    body,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- Problems found while scanning, kept so they survive a restart and can be
-- shown as one report instead of a stream of dialogs.
CREATE TABLE IF NOT EXISTS diagnostics (
    id      INTEGER PRIMARY KEY,
    payload TEXT NOT NULL
);
"#;

/// Create the schema if absent and record its version.
pub fn initialize(conn: &Connection) -> Result<()> {
    conn.execute_batch(DDL)?;
    conn.execute(
        "INSERT INTO meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![META_SCHEMA_VERSION, SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
}

/// The schema version recorded in this database, if any.
pub fn stored_version(conn: &Connection) -> Option<i64> {
    conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        [META_SCHEMA_VERSION],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|v| v.parse().ok())
}

/// Remove every derived row while keeping the schema, for a full rebuild that
/// does not need to recreate the file.
pub fn clear(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DELETE FROM notes_fts;
         DELETE FROM properties;
         DELETE FROM tags;
         DELETE FROM links;
         DELETE FROM blocks;
         DELETE FROM headings;
         DELETE FROM diagnostics;
         DELETE FROM files;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        apply_pragmas(&conn).ok();
        initialize(&conn).unwrap();
        conn
    }

    #[test]
    fn initialization_is_idempotent() {
        let conn = memory_db();
        initialize(&conn).unwrap();
        assert_eq!(stored_version(&conn), Some(SCHEMA_VERSION));
    }

    #[test]
    fn every_expected_table_exists() {
        let conn = memory_db();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table','view') ORDER BY name")
            .unwrap();
        let names: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(std::result::Result::ok)
            .collect();
        for expected in [
            "blocks",
            "diagnostics",
            "files",
            "headings",
            "links",
            "meta",
            "notes_fts",
            "properties",
            "tags",
        ] {
            assert!(
                names.contains(&expected.to_string()),
                "missing table {expected}"
            );
        }
    }

    #[test]
    fn full_text_search_is_available_with_snippets() {
        let conn = memory_db();
        conn.execute(
            "INSERT INTO notes_fts(rowid, path, title, body) VALUES (1, 'a.md', 'Title', 'the quick brown fox')",
            [],
        )
        .unwrap();
        let snippet: String = conn
            .query_row(
                "SELECT snippet(notes_fts, 2, '<b>', '</b>', '…', 8) FROM notes_fts WHERE notes_fts MATCH 'brown'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(snippet.contains("<b>brown</b>"), "{snippet}");
    }

    #[test]
    fn full_text_search_ignores_diacritics() {
        let conn = memory_db();
        conn.execute(
            "INSERT INTO notes_fts(rowid, path, title, body) VALUES (1, 'a.md', 'Café', 'a café note')",
            [],
        )
        .unwrap();
        let hits: i64 = conn
            .query_row(
                "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'cafe'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);
    }

    #[test]
    fn deleting_a_file_cascades_to_its_derived_rows() {
        let conn = memory_db();
        conn.execute(
            "INSERT INTO files(id, path, path_fold, name, name_fold, stem_fold, ext, kind, size, mtime_ms, indexed_at)
             VALUES (1, 'a.md', 'a.md', 'a.md', 'a.md', 'a', 'md', 'note', 1, 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tags(file_id, tag, tag_fold, source, line, byte_start) VALUES (1,'AI','ai','body',0,0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO headings(file_id, ordinal, level, text, slug, line, byte_start) VALUES (1,0,1,'T','t',0,0)",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM files WHERE id = 1", []).unwrap();

        let tags: i64 = conn
            .query_row("SELECT count(*) FROM tags", [], |r| r.get(0))
            .unwrap();
        let headings: i64 = conn
            .query_row("SELECT count(*) FROM headings", [], |r| r.get(0))
            .unwrap();
        assert_eq!((tags, headings), (0, 0));
    }

    #[test]
    fn deleting_a_link_target_leaves_the_link_but_unresolves_it() {
        let conn = memory_db();
        for (id, path) in [(1, "source.md"), (2, "target.md")] {
            conn.execute(
                "INSERT INTO files(id, path, path_fold, name, name_fold, stem_fold, ext, kind, size, mtime_ms, indexed_at)
                 VALUES (?1, ?2, ?2, ?2, ?2, ?2, 'md', 'note', 1, 0, 0)",
                rusqlite::params![id, path],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO links(file_id, kind, raw, target_text, target_fold, target_tail, target_file_id, line, byte_start, byte_end)
             VALUES (1, 'wikiLink', '[[target]]', 'target', 'target', 'target', 2, 0, 0, 10)",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM files WHERE id = 2", []).unwrap();

        let (count, target): (i64, Option<i64>) = conn
            .query_row(
                "SELECT count(*), max(target_file_id) FROM links WHERE file_id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1, "the link text still exists in the note");
        assert_eq!(target, None, "but it no longer points anywhere");
    }

    #[test]
    fn clearing_removes_rows_but_keeps_the_schema() {
        let conn = memory_db();
        conn.execute(
            "INSERT INTO files(id, path, path_fold, name, name_fold, stem_fold, ext, kind, size, mtime_ms, indexed_at)
             VALUES (1,'a.md','a.md','a.md','a.md','a','md','note',1,0,0)",
            [],
        )
        .unwrap();
        clear(&conn).unwrap();
        let files: i64 = conn
            .query_row("SELECT count(*) FROM files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(files, 0);
        assert_eq!(stored_version(&conn), Some(SCHEMA_VERSION));
    }
}
