//! Application state shared by every command.
//!
//! One vault is open at a time, behind a mutex. Commands are short — they take
//! the lock, do one indexed operation, and release it — so contention is not
//! the issue; the long-running work (a full scan) happens on a worker thread
//! that takes the lock in batches, which is why the indexer commits per batch
//! rather than once at the end.

use std::sync::{Arc, Mutex, MutexGuard};

use ie_core::session::VaultSession;
use ie_core::CoreError;
use ie_platform::HostServices;

use crate::error::CommandError;

pub struct AppState {
    pub host: HostServices,
    session: Mutex<Option<VaultSession>>,
    /// Vaults the user has opened before, most recent first. Lives in the OS
    /// config directory, not in any vault.
    recent: Mutex<Vec<RecentVault>>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentVault {
    pub path: String,
    pub name: String,
    pub last_opened_ms: i64,
}

pub const RECENT_FILE: &str = "recent-vaults.json";
pub const MAX_RECENT: usize = 12;

impl AppState {
    pub fn new(host: HostServices) -> Self {
        let recent = Self::load_recent(&host);
        Self {
            host,
            session: Mutex::new(None),
            recent: Mutex::new(recent),
        }
    }

    /// Borrow the open vault, or fail with a typed error the UI can act on.
    pub fn session(&self) -> Result<MutexGuard<'_, Option<VaultSession>>, CommandError> {
        let guard = self
            .session
            .lock()
            .map_err(|_| CommandError::from(CoreError::Refused {
                operation: "lock vault",
                reason: "the vault state was left inconsistent by an earlier failure".into(),
            }))?;
        if guard.is_none() {
            return Err(CoreError::NoVaultOpen.into());
        }
        Ok(guard)
    }

    /// Run `f` against the open vault.
    pub fn with_session<T>(
        &self,
        f: impl FnOnce(&mut VaultSession) -> Result<T, CoreError>,
    ) -> Result<T, CommandError> {
        let mut guard = self.session()?;
        let session = guard.as_mut().expect("checked by session()");
        f(session).map_err(CommandError::from)
    }

    /// Read-only access, for the many commands that only query the index.
    pub fn with_index<T>(
        &self,
        f: impl FnOnce(&VaultSession) -> Result<T, CoreError>,
    ) -> Result<T, CommandError> {
        let guard = self.session()?;
        let session = guard.as_ref().expect("checked by session()");
        f(session).map_err(CommandError::from)
    }

    pub fn set_session(&self, session: Option<VaultSession>) {
        if let Ok(mut guard) = self.session.lock() {
            *guard = session;
        }
    }

    pub fn is_open(&self) -> bool {
        self.session
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    pub fn recent_vaults(&self) -> Vec<RecentVault> {
        self.recent.lock().map(|r| r.clone()).unwrap_or_default()
    }

    pub fn remember_vault(&self, path: &str, name: &str) {
        let Ok(mut recent) = self.recent.lock() else {
            return;
        };
        recent.retain(|v| v.path != path);
        recent.insert(
            0,
            RecentVault {
                path: path.to_string(),
                name: name.to_string(),
                last_opened_ms: self.host.clock.now_ms(),
            },
        );
        recent.truncate(MAX_RECENT);
        Self::save_recent(&self.host, &recent);
    }

    pub fn forget_vault(&self, path: &str) {
        let Ok(mut recent) = self.recent.lock() else {
            return;
        };
        recent.retain(|v| v.path != path);
        Self::save_recent(&self.host, &recent);
    }

    fn recent_path(host: &HostServices) -> Option<std::path::PathBuf> {
        host.dirs.config_dir().ok().map(|d| d.join(RECENT_FILE))
    }

    fn load_recent(host: &HostServices) -> Vec<RecentVault> {
        let Some(path) = Self::recent_path(host) else {
            return Vec::new();
        };
        // A missing or damaged list is an empty list. It is a convenience, not
        // data the user would miss.
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save_recent(host: &HostServices, recent: &[RecentVault]) {
        let Some(path) = Self::recent_path(host) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(recent) {
            if let Err(e) = host.fs.write_atomic(&path, json.as_bytes()) {
                tracing::warn!(error = %e, "could not save the recent-vault list");
            }
        }
    }
}

pub type SharedState = Arc<AppState>;
