//! Assembling the host services from what the application supplies.
//!
//! The desktop calls `HostServices::for_current_platform()` and everything is
//! derived. iOS cannot: the container path changes between launches, the vault
//! is reached through a bookmark, and whether the vault is behind a file
//! provider is something only the application knows. So the host describes
//! itself, once, and this builds the services from that description.

use std::path::PathBuf;
use std::sync::Arc;

use ie_platform::platform::ios::{
    ContainerDirs, CoordinatedFileSystem, FileProvider, IosPlatform, LocalFolder, PresenterWatch,
    PresenterWatcher,
};
use ie_platform::{HostOffsetClock, HostServices, PlatformError};

use crate::error::{FfiError, Result};

/// How a vault's files are reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum StorageKind {
    /// A plain directory — "On My iPhone", or a folder the app was given.
    /// Nothing is ever evicted and `rename(2)` is the correct atomic replace.
    LocalFolder,
    /// A File Provider: iCloud Drive, or a third-party provider in Files.app.
    /// Contents may not be on the device, and the provider must perform the
    /// replace itself.
    FileProvider,
}

/// Everything the host must tell the bridge before a vault can be opened.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HostConfig {
    /// The container's `Library` directory, from `FileManager.urls(for:in:)`.
    ///
    /// Passed in rather than derived because iOS reassigns it between
    /// launches; a path remembered from last time points at nothing.
    pub library_dir: String,
    /// `TimeZone.current.secondsFromGMT()`.
    ///
    /// Supplied rather than read here: `time` refuses to look up the local
    /// offset from a multithreaded process and silently answers UTC, which
    /// would put the daily note on the wrong day.
    #[uniffi(default = 0)]
    pub utc_offset_seconds: i32,
    pub storage: StorageKind,
    /// How long the vault must be quiet before a batch of changes is reported.
    #[uniffi(default = 150)]
    pub watch_debounce_ms: u64,
}

impl HostConfig {
    pub(crate) fn dirs(&self) -> ContainerDirs {
        ContainerDirs::under_library(PathBuf::from(&self.library_dir))
    }
}

/// The host's implementation of the two operations POSIX cannot do.
///
/// Declared as a callback interface so Swift can supply it; see
/// `docs/ios/STORAGE.md` for why these two and nothing else.
#[uniffi::export(with_foreign)]
pub trait StorageHost: Send + Sync {
    /// Download `relative_path`'s contents onto the device, blocking until
    /// they are there. Called only after a placeholder has been seen.
    fn ensure_materialized(&self, relative_path: String) -> Result<()>;

    /// Atomically replace `relative_target` with `relative_source`, a sibling
    /// temporary this crate has already written and fsynced. The temporary
    /// must not survive the call.
    fn replace_item(&self, relative_target: String, relative_source: String) -> Result<()>;
}

/// Adapts a host's [`StorageHost`] to the platform trait, translating between
/// absolute paths (which the core works in) and vault-relative ones (which are
/// all the host ever sees, so it never holds a path it must not persist).
struct HostBackedProvider {
    host: Arc<dyn StorageHost>,
    root: PathBuf,
}

impl HostBackedProvider {
    fn relative(&self, path: &std::path::Path) -> std::result::Result<String, PlatformError> {
        path.strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .map_err(|_| PlatformError::PermissionDenied {
                path: path.to_path_buf(),
            })
    }
}

impl FileProvider for HostBackedProvider {
    fn ensure_materialized(&self, path: &std::path::Path) -> ie_platform::Result<()> {
        let relative = self.relative(path)?;
        self.host
            .ensure_materialized(relative)
            .map_err(|e| PlatformError::Io {
                operation: "ensure_materialized",
                path: path.to_path_buf(),
                source: std::io::Error::other(e.to_string()),
            })
    }

    fn replace_item(
        &self,
        target: &std::path::Path,
        source: &std::path::Path,
    ) -> ie_platform::Result<()> {
        let (target_rel, source_rel) = (self.relative(target)?, self.relative(source)?);
        match self.host.replace_item(target_rel, source_rel) {
            Ok(()) => Ok(()),
            Err(e) => {
                // A failed replace must not leave the temporary behind, or the
                // next scan reports an interrupted write that never happened.
                let _ = std::fs::remove_file(source);
                Err(PlatformError::Io {
                    operation: "replace_item",
                    path: target.to_path_buf(),
                    source: std::io::Error::other(e.to_string()),
                })
            }
        }
    }

    fn placeholder_name(&self, file_name: &str) -> Option<String> {
        Some(ie_platform::platform::ios::icloud::placeholder_name(
            file_name,
        ))
    }

    fn name_behind_placeholder(&self, entry_name: &str) -> Option<String> {
        ie_platform::platform::ios::icloud::name_behind_placeholder(entry_name)
    }
}

/// The services, plus the handles the bridge keeps hold of afterwards.
pub(crate) struct Host {
    pub services: HostServices,
    pub watcher: Arc<PresenterWatcher>,
    pub clock: Arc<HostOffsetClock>,
    pub dirs: ContainerDirs,
    pub debounce_ms: u64,
}

impl Host {
    /// Build the services for a vault rooted at `root`.
    pub(crate) fn build(
        config: &HostConfig,
        root: &std::path::Path,
        storage: Option<Arc<dyn StorageHost>>,
    ) -> Result<Self> {
        let dirs = config.dirs();
        let platform: Arc<dyn ie_platform::PlatformOps> =
            Arc::new(IosPlatform::with_dirs(dirs.clone()));

        let provider: Arc<dyn FileProvider> = match (config.storage, storage) {
            (StorageKind::FileProvider, Some(host)) => Arc::new(HostBackedProvider {
                host,
                root: root.to_path_buf(),
            }),
            (StorageKind::FileProvider, None) => {
                // Silently falling back to POSIX here would mean an evicted
                // note reading as empty and being indexed as empty, which is
                // data loss the user would only notice much later.
                return Err(FfiError::InvalidArgument {
                    message: "a file-provider vault needs a StorageHost".into(),
                });
            }
            (StorageKind::LocalFolder, _) => Arc::new(LocalFolder),
        };

        let watcher = Arc::new(PresenterWatcher::new());
        let clock = Arc::new(HostOffsetClock::new(config.utc_offset_seconds));
        let fs = Arc::new(CoordinatedFileSystem::new(
            Arc::clone(&platform),
            Arc::clone(&provider),
        ));

        let dirs_arc: ie_platform::SharedAppDirs = Arc::new(dirs.clone());
        ie_platform::AppDirs::ensure_all(dirs_arc.as_ref())?;

        Ok(Self {
            services: HostServices::new(
                fs,
                Arc::clone(&watcher) as ie_platform::SharedFileWatcher,
                Arc::clone(&clock) as ie_platform::SharedClock,
                dirs_arc,
                platform,
            ),
            watcher,
            clock,
            dirs,
            debounce_ms: config.watch_debounce_ms,
        })
    }
}

/// A live watch, kept alive by the handle that owns it.
pub(crate) type Watch = PresenterWatch;
