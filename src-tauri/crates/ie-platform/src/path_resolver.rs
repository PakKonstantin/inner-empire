use std::path::{Component, Path, PathBuf};

/// Portable path arithmetic.
///
/// Everything here goes through `std::path::Components`, which already knows
/// that Windows accepts both `\` and `/` and that POSIX accepts only `/`.
/// Splitting a path on a literal separator is what this type exists to
/// prevent.
pub struct PathResolver;

impl PathResolver {
    /// Express `path` relative to `base`, or `None` if it escapes `base`.
    ///
    /// Both sides should already be canonical; callers that accept user input
    /// canonicalize first so symlinks cannot be used to step outside a vault.
    pub fn relativize(base: &Path, path: &Path) -> Option<PathBuf> {
        path.strip_prefix(base).ok().map(Path::to_path_buf)
    }

    /// Remove `.` components and resolve `..` textually, without touching the
    /// filesystem. A leading `..` that would escape the root is dropped, so
    /// the result can never point above `path`'s own root.
    pub fn normalize(path: &Path) -> PathBuf {
        let mut out = PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    if !out.pop() {
                        // Nothing to pop: refuse to climb above the root.
                    }
                }
                other => out.push(other.as_os_str()),
            }
        }
        out
    }

    /// True when `path` is inside `base` (or is `base` itself), judged after
    /// normalisation. Used as the vault sandbox check.
    pub fn is_within(base: &Path, path: &Path) -> bool {
        let base = Self::normalize(base);
        let path = Self::normalize(path);
        path.starts_with(&base)
    }

    /// Split a filename into stem and lowercase extension.
    pub fn split_extension(file_name: &str) -> (&str, Option<String>) {
        match file_name.rfind('.') {
            // A leading dot means a hidden file, not an extension.
            Some(0) | None => (file_name, None),
            Some(idx) => (
                &file_name[..idx],
                Some(file_name[idx + 1..].to_ascii_lowercase()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PathResolver;
    use std::path::Path;

    #[test]
    fn normalize_resolves_dot_segments() {
        assert_eq!(
            PathResolver::normalize(Path::new("a/./b/../c")),
            Path::new("a/c")
        );
    }

    #[test]
    fn normalize_refuses_to_escape_the_root() {
        assert_eq!(PathResolver::normalize(Path::new("../../etc")), Path::new("etc"));
    }

    #[test]
    fn is_within_rejects_siblings() {
        assert!(PathResolver::is_within(Path::new("/vault"), Path::new("/vault/a/b.md")));
        assert!(!PathResolver::is_within(
            Path::new("/vault"),
            Path::new("/vault-other/b.md")
        ));
    }

    #[test]
    fn split_extension_handles_dotfiles_and_multiple_dots() {
        assert_eq!(PathResolver::split_extension("note.md"), ("note", Some("md".into())));
        assert_eq!(
            PathResolver::split_extension("archive.tar.GZ"),
            ("archive.tar", Some("gz".into()))
        );
        assert_eq!(PathResolver::split_extension(".gitignore"), (".gitignore", None));
        assert_eq!(PathResolver::split_extension("README"), ("README", None));
    }
}
