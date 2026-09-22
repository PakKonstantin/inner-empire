//! Opening, validating and repairing the index database.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{CoreError, Result};
use crate::index::schema;

/// Why the index had to be rebuilt, so the UI can say something truthful
/// rather than "loading…".
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpenOutcome {
    /// Opened an existing, usable index.
    Reused,
    /// No index file existed.
    Created,
    /// The file was from an older or newer build of the app.
    RebuiltForSchemaChange,
    /// The file was corrupt or not a database at all.
    RebuiltAfterCorruption,
}

impl OpenOutcome {
    pub fn needs_full_scan(self) -> bool {
        !matches!(self, OpenOutcome::Reused)
    }
}

/// The index database for one vault.
pub struct IndexDb {
    conn: Connection,
    path: PathBuf,
}

impl IndexDb {
    /// Open the index at `path`, rebuilding from scratch if it cannot be used.
    ///
    /// Losing an index is never a data-loss event — the Markdown files are the
    /// truth — so every failure mode here resolves to "start over" rather than
    /// to an error the user has to act on.
    pub fn open(path: &Path) -> Result<(Self, OpenOutcome)> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                CoreError::Platform(ie_platform::PlatformError::from_io(
                    "create_dir_all",
                    parent,
                    e,
                ))
            })?;
        }

        let existed = path.exists();
        match Self::try_open(path) {
            Ok(db) => {
                let outcome = if existed {
                    OpenOutcome::Reused
                } else {
                    OpenOutcome::Created
                };
                Ok((db, outcome))
            }
            Err(reason) => {
                tracing::warn!(
                    index = %path.display(),
                    reason = %reason.message,
                    "index unusable, rebuilding from the vault"
                );
                Self::discard(path)?;
                let db = Self::try_open(path)
                    .map_err(|e| CoreError::IndexCorrupt { message: e.message })?;
                Ok((db, reason.outcome))
            }
        }
    }

    /// Open an index that lives only in memory, for tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        schema::apply_pragmas(&conn).ok();
        schema::initialize(&conn)?;
        Ok(Self {
            conn,
            path: PathBuf::from(":memory:"),
        })
    }

    fn try_open(path: &Path) -> std::result::Result<Self, OpenFailure> {
        let conn = Connection::open(path).map_err(|e| OpenFailure {
            message: e.to_string(),
            outcome: OpenOutcome::RebuiltAfterCorruption,
        })?;
        schema::apply_pragmas(&conn).ok();

        // `integrity_check` on a large database is slow, so use the cheap
        // `quick_check`: it catches a truncated or non-database file, which is
        // what a crash or a partial sync actually produces.
        let check: String = conn
            .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
            .map_err(|e| OpenFailure {
                message: e.to_string(),
                outcome: OpenOutcome::RebuiltAfterCorruption,
            })?;
        if check != "ok" {
            return Err(OpenFailure {
                message: check,
                outcome: OpenOutcome::RebuiltAfterCorruption,
            });
        }

        if let Some(version) = schema::stored_version(&conn) {
            if version != schema::SCHEMA_VERSION {
                return Err(OpenFailure {
                    message: format!(
                        "index schema is version {version}, this build expects {}",
                        schema::SCHEMA_VERSION
                    ),
                    outcome: OpenOutcome::RebuiltForSchemaChange,
                });
            }
        }

        schema::initialize(&conn).map_err(|e| OpenFailure {
            message: e.to_string(),
            outcome: OpenOutcome::RebuiltAfterCorruption,
        })?;

        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    /// Remove the database and the files WAL mode creates beside it.
    fn discard(path: &Path) -> Result<()> {
        for suffix in ["", "-wal", "-shm"] {
            let victim = if suffix.is_empty() {
                path.to_path_buf()
            } else {
                PathBuf::from(format!("{}{suffix}", path.display()))
            };
            if victim.exists() {
                std::fs::remove_file(&victim).map_err(|e| {
                    CoreError::Platform(ie_platform::PlatformError::from_io(
                        "remove_file",
                        &victim,
                        e,
                    ))
                })?;
            }
        }
        Ok(())
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Drop every derived row, for a rebuild that keeps the file.
    pub fn clear(&self) -> Result<()> {
        schema::clear(&self.conn)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .ok()
    }

    pub fn file_count(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT count(*) FROM files", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Reclaim space and refresh query-planner statistics. Worth doing after a
    /// large rescan, not after every save.
    pub fn optimize(&self) -> Result<()> {
        self.conn.execute_batch("PRAGMA optimize; ANALYZE;")?;
        Ok(())
    }
}

struct OpenFailure {
    message: String,
    outcome: OpenOutcome,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_fresh_index_reports_that_it_was_created() {
        let dir = tempfile::tempdir().unwrap();
        let (db, outcome) = IndexDb::open(&dir.path().join("index.db")).unwrap();
        assert_eq!(outcome, OpenOutcome::Created);
        assert!(outcome.needs_full_scan());
        assert_eq!(db.file_count().unwrap(), 0);
    }

    #[test]
    fn reopening_a_good_index_reuses_it_without_a_rescan() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.db");
        {
            let (db, _) = IndexDb::open(&path).unwrap();
            db.set_meta("vault_id", "abc").unwrap();
        }
        let (db, outcome) = IndexDb::open(&path).unwrap();
        assert_eq!(outcome, OpenOutcome::Reused);
        assert!(!outcome.needs_full_scan());
        assert_eq!(db.meta("vault_id").as_deref(), Some("abc"));
    }

    #[test]
    fn a_corrupt_index_is_discarded_and_recreated_instead_of_failing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.db");
        {
            let mut file = std::fs::File::create(&path).unwrap();
            file.write_all(b"this is definitely not a database")
                .unwrap();
        }

        let (db, outcome) = IndexDb::open(&path).unwrap();
        assert_eq!(outcome, OpenOutcome::RebuiltAfterCorruption);
        assert!(outcome.needs_full_scan());
        assert_eq!(db.file_count().unwrap(), 0);
    }

    #[test]
    fn an_index_from_a_different_schema_version_is_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.db");
        {
            let (db, _) = IndexDb::open(&path).unwrap();
            db.set_meta(schema::META_SCHEMA_VERSION, "999").unwrap();
            db.set_meta("vault_id", "stale").unwrap();
        }

        let (db, outcome) = IndexDb::open(&path).unwrap();
        assert_eq!(outcome, OpenOutcome::RebuiltForSchemaChange);
        assert_eq!(db.meta("vault_id"), None, "stale rows must not survive");
        assert_eq!(
            schema::stored_version(db.connection()),
            Some(schema::SCHEMA_VERSION)
        );
    }

    #[test]
    fn opening_creates_missing_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deeper").join("index.db");
        let (_, outcome) = IndexDb::open(&path).unwrap();
        assert_eq!(outcome, OpenOutcome::Created);
        assert!(path.exists());
    }
}
