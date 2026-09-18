use std::path::{Path, PathBuf};

use crate::error::Result;

/// Handing a path to the desktop environment.
///
/// The brief forbids `xdg-open` and `explorer.exe` from appearing in core, so
/// core depends on this trait and the concrete invocations live in
/// `platform/linux` and `platform/windows`.
pub trait ShellIntegration: Send + Sync {
    /// Open a file or folder with whatever the desktop associates with it.
    fn open_path(&self, path: &Path) -> Result<()>;
    /// Open the containing folder with the item selected, where the desktop
    /// supports it; otherwise just open the folder.
    fn reveal_in_file_manager(&self, path: &Path) -> Result<()>;
    /// Open a URL in the user's browser. Refuses non-http(s) schemes so a
    /// crafted note cannot launch an arbitrary handler.
    fn open_external_url(&self, url: &str) -> Result<()>;
}

/// Spawning child processes. Kept behind a trait so tests can assert on what
/// would have been launched instead of launching it.
pub trait ProcessManager: Send + Sync {
    fn spawn_detached(&self, program: &str, args: &[&str]) -> Result<()>;
}

/// The system clipboard.
pub trait Clipboard: Send + Sync {
    fn read_text(&self) -> Result<Option<String>>;
    fn write_text(&self, text: &str) -> Result<()>;
}

#[derive(Debug, Clone, Default)]
pub struct FileDialogOptions {
    pub title: Option<String>,
    pub start_directory: Option<PathBuf>,
    /// `(label, extensions)` pairs, e.g. `("Markdown", ["md", "markdown"])`.
    pub filters: Vec<(String, Vec<String>)>,
    pub default_file_name: Option<String>,
}

/// Native file and folder pickers, plus message boxes.
pub trait SystemDialog: Send + Sync {
    fn pick_folder(&self, options: FileDialogOptions) -> Result<Option<PathBuf>>;
    fn pick_files(&self, options: FileDialogOptions) -> Result<Vec<PathBuf>>;
    fn save_file(&self, options: FileDialogOptions) -> Result<Option<PathBuf>>;
    fn confirm(&self, title: &str, message: &str) -> Result<bool>;
}

/// Reject anything that is not plain http(s). A note is untrusted input, so
/// `file:`, `javascript:` and custom protocol handlers must not be reachable
/// from a link click.
pub fn is_safe_external_url(url: &str) -> bool {
    let lowered = url.trim().to_ascii_lowercase();
    (lowered.starts_with("http://") || lowered.starts_with("https://"))
        && !lowered.contains(['\n', '\r', '\0'])
}

#[cfg(test)]
mod tests {
    use super::is_safe_external_url;

    #[test]
    fn accepts_plain_web_urls() {
        assert!(is_safe_external_url("https://example.org/a?b=c"));
        assert!(is_safe_external_url("http://localhost:1420"));
    }

    #[test]
    fn rejects_other_schemes_and_injection() {
        assert!(!is_safe_external_url("javascript:alert(1)"));
        assert!(!is_safe_external_url("file:///etc/passwd"));
        assert!(!is_safe_external_url("ms-settings:"));
        assert!(!is_safe_external_url("https://ok\nmalicious"));
    }
}
