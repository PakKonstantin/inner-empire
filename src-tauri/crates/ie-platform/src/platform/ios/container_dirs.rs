//! Application directories inside an iOS container.
//!
//! Unlike the desktop adapters, nothing here is computed. iOS reassigns the
//! container path between launches — the UUID in
//! `…/Containers/Data/Application/<uuid>/` is not stable — so a path derived
//! once and remembered would break, and a path hardcoded would be wrong on the
//! first run. The host resolves the four locations with
//! `FileManager.urls(for:in:)` and passes them in.

use std::path::{Path, PathBuf};

use crate::dirs::AppDirs;
use crate::error::Result;

/// The four `AppDirs` locations, supplied by the host.
#[derive(Debug, Clone)]
pub struct ContainerDirs {
    config: PathBuf,
    data: PathBuf,
    log: PathBuf,
    cache: PathBuf,
}

impl ContainerDirs {
    pub fn new(
        config: impl Into<PathBuf>,
        data: impl Into<PathBuf>,
        log: impl Into<PathBuf>,
        cache: impl Into<PathBuf>,
    ) -> Self {
        Self {
            config: config.into(),
            data: data.into(),
            log: log.into(),
            cache: cache.into(),
        }
    }

    /// The conventional layout, derived from the container's `Library`.
    ///
    /// `Application Support` is backed up and not purgeable, which is right for
    /// settings and for the recovery journal. `Caches` is purgeable under
    /// storage pressure, which is right for the search index — it is a cache,
    /// and losing it costs a rescan and nothing else.
    pub fn under_library(library: impl AsRef<Path>) -> Self {
        let library = library.as_ref();
        let support = library.join("Application Support");
        Self::new(
            support.join("config"),
            support.join("data"),
            library.join("Logs"),
            library.join("Caches"),
        )
    }

    /// Where a vault's index cache goes.
    ///
    /// Not inside the vault, as on desktop: SQLite wants POSIX advisory locks
    /// and its `-wal`/`-shm` siblings, and a File Provider directory guarantees
    /// neither. Keyed by the vault's own id — which lives in the vault and
    /// travels with it — so the same vault finds its own cache again, and two
    /// vaults never collide.
    pub fn index_path(&self, vault_id: &str) -> PathBuf {
        self.cache.join("index").join(format!("{vault_id}.db"))
    }
}

impl AppDirs for ContainerDirs {
    fn config_dir(&self) -> Result<PathBuf> {
        Ok(self.config.clone())
    }
    fn data_dir(&self) -> Result<PathBuf> {
        Ok(self.data.clone())
    }
    fn log_dir(&self) -> Result<PathBuf> {
        Ok(self.log.clone())
    }
    fn cache_dir(&self) -> Result<PathBuf> {
        Ok(self.cache.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_layout_puts_the_index_in_a_purgeable_location() {
        let dirs = ContainerDirs::under_library(Path::new("/container/Library"));

        assert_eq!(
            dirs.cache_dir().unwrap(),
            PathBuf::from("/container/Library/Caches")
        );
        assert_eq!(
            dirs.index_path("018f-abc"),
            PathBuf::from("/container/Library/Caches/index/018f-abc.db")
        );
        // Settings and the recovery journal must survive a cache purge.
        assert_eq!(
            dirs.data_dir().unwrap(),
            PathBuf::from("/container/Library/Application Support/data")
        );
    }

    #[test]
    fn two_vaults_do_not_share_an_index() {
        let dirs = ContainerDirs::under_library(Path::new("/container/Library"));
        assert_ne!(dirs.index_path("vault-a"), dirs.index_path("vault-b"));
    }
}
