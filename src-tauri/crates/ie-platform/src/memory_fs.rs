//! An in-memory filesystem.
//!
//! Exists so `ie-core` tests can exercise vault, index and link logic without
//! touching a disk, and so the dependency-injection boundary is demonstrably
//! real rather than aspirational. It models POSIX-style case sensitivity by
//! default and can be switched to case-folding to reproduce NTFS behaviour on
//! a Linux CI runner.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::{PlatformError, Result};
use crate::fs::{DirEntry, FileMetadata, FileSystem};

#[derive(Debug, Clone)]
enum Node {
    File { data: Vec<u8>, modified_ms: i64 },
    Dir,
}

/// An entry stores both the name as it was created and the key it is looked up
/// by. In case-insensitive mode the two differ, which is what makes this double
/// behave like NTFS: case-preserving, case-insensitive. A double that lowercased
/// stored names would hide exactly the bug it exists to catch.
#[derive(Debug, Clone)]
struct Entry {
    display: String,
    node: Node,
}

#[derive(Debug)]
pub struct MemoryFileSystem {
    nodes: Mutex<BTreeMap<String, Entry>>,
    case_sensitive: bool,
    clock: Mutex<i64>,
}

impl Default for MemoryFileSystem {
    fn default() -> Self {
        Self::new(true)
    }
}

impl MemoryFileSystem {
    pub fn new(case_sensitive: bool) -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            "/".to_string(),
            Entry {
                display: "/".to_string(),
                node: Node::Dir,
            },
        );
        Self {
            nodes: Mutex::new(nodes),
            case_sensitive,
            clock: Mutex::new(1_700_000_000_000),
        }
    }

    /// The `/`-separated absolute spelling of a path, as given. Windows-style
    /// input is accepted because `Components` already understands it.
    fn display_path(path: &Path) -> String {
        let mut parts: Vec<String> = Vec::new();
        for component in path.components() {
            match component {
                std::path::Component::Normal(p) => parts.push(p.to_string_lossy().to_string()),
                std::path::Component::ParentDir => {
                    parts.pop();
                }
                _ => {}
            }
        }
        format!("/{}", parts.join("/"))
    }

    /// The key a path is looked up by: folded when the filesystem is
    /// case-insensitive.
    fn key(&self, path: &Path) -> String {
        self.fold(&Self::display_path(path))
    }

    fn fold(&self, path: &str) -> String {
        if self.case_sensitive {
            path.to_string()
        } else {
            path.to_lowercase()
        }
    }

    fn tick(&self) -> i64 {
        let mut clock = self.clock.lock().expect("memory fs clock poisoned");
        *clock += 1000;
        *clock
    }

    fn parent_key(key: &str) -> Option<String> {
        let trimmed = key.trim_end_matches('/');
        let idx = trimmed.rfind('/')?;
        Some(if idx == 0 {
            "/".to_string()
        } else {
            trimmed[..idx].to_string()
        })
    }
}

impl FileSystem for MemoryFileSystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        let nodes = self.nodes.lock().expect("memory fs poisoned");
        match nodes.get(&self.key(path)).map(|e| &e.node) {
            Some(Node::File { data, .. }) => Ok(data.clone()),
            Some(Node::Dir) => Err(PlatformError::IsADirectory {
                path: path.to_path_buf(),
            }),
            None => Err(PlatformError::NotFound {
                path: path.to_path_buf(),
            }),
        }
    }

    fn write_atomic(&self, path: &Path, contents: &[u8]) -> Result<()> {
        let key = self.key(path);
        let parent = Self::parent_key(&key).ok_or_else(|| PlatformError::NotADirectory {
            path: path.to_path_buf(),
        })?;
        let modified_ms = self.tick();
        let display = Self::display_path(path);
        let mut nodes = self.nodes.lock().expect("memory fs poisoned");
        if !matches!(nodes.get(&parent).map(|e| &e.node), Some(Node::Dir)) {
            return Err(PlatformError::NotFound {
                path: PathBuf::from(parent),
            });
        }
        // Writing to an existing file keeps the name it already had, the way a
        // case-insensitive filesystem does.
        let display = nodes
            .get(&key)
            .map(|e| e.display.clone())
            .unwrap_or(display);
        nodes.insert(
            key,
            Entry {
                display,
                node: Node::File {
                    data: contents.to_vec(),
                    modified_ms,
                },
            },
        );
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> Result<()> {
        let display = Self::display_path(path);
        let mut nodes = self.nodes.lock().expect("memory fs poisoned");
        let mut accumulated = String::from("/");
        for segment in display.split('/').filter(|s| !s.is_empty()) {
            if accumulated.len() > 1 {
                accumulated.push('/');
            }
            accumulated.push_str(segment);
            let key = self.fold(&accumulated);
            match nodes.get(&key).map(|e| &e.node) {
                Some(Node::File { .. }) => {
                    return Err(PlatformError::NotADirectory {
                        path: PathBuf::from(accumulated),
                    })
                }
                Some(Node::Dir) => {}
                None => {
                    nodes.insert(
                        key,
                        Entry {
                            display: accumulated.clone(),
                            node: Node::Dir,
                        },
                    );
                }
            }
        }
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        let mut nodes = self.nodes.lock().expect("memory fs poisoned");
        match nodes.remove(&self.key(path)) {
            Some(Entry {
                node: Node::File { .. },
                ..
            }) => Ok(()),
            Some(entry) => {
                nodes.insert(self.key(path), entry);
                Err(PlatformError::IsADirectory {
                    path: path.to_path_buf(),
                })
            }
            None => Err(PlatformError::NotFound {
                path: path.to_path_buf(),
            }),
        }
    }

    fn remove_dir_all(&self, path: &Path) -> Result<()> {
        let key = self.key(path);
        let prefix = format!("{}/", key.trim_end_matches('/'));
        let mut nodes = self.nodes.lock().expect("memory fs poisoned");
        let doomed: Vec<String> = nodes
            .keys()
            .filter(|k| **k == key || k.starts_with(&prefix))
            .cloned()
            .collect();
        if doomed.is_empty() {
            return Err(PlatformError::NotFound {
                path: path.to_path_buf(),
            });
        }
        for k in doomed {
            nodes.remove(&k);
        }
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        let from_key = self.key(from);
        let to_key = self.key(to);
        let prefix = format!("{}/", from_key.trim_end_matches('/'));
        let mut nodes = self.nodes.lock().expect("memory fs poisoned");
        let moving: Vec<String> = nodes
            .keys()
            .filter(|k| **k == from_key || k.starts_with(&prefix))
            .cloned()
            .collect();
        if moving.is_empty() {
            return Err(PlatformError::NotFound {
                path: from.to_path_buf(),
            });
        }
        let to_display = Self::display_path(to);
        for key in moving {
            let mut entry = nodes.remove(&key).expect("key came from this map");
            let suffix = &key[from_key.len()..];
            let new_display_suffix =
                entry.display[entry.display.len() - suffix.len()..].to_string();
            entry.display = format!("{to_display}{new_display_suffix}");
            nodes.insert(self.fold(&entry.display), entry);
        }
        let _ = to_key;
        Ok(())
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<u64> {
        let data = self.read(from)?;
        let len = data.len() as u64;
        self.write_atomic(to, &data)?;
        Ok(len)
    }

    fn metadata(&self, path: &Path) -> Result<FileMetadata> {
        let nodes = self.nodes.lock().expect("memory fs poisoned");
        match nodes.get(&self.key(path)).map(|e| &e.node) {
            Some(Node::File { data, modified_ms }) => Ok(FileMetadata {
                is_dir: false,
                is_symlink: false,
                len: data.len() as u64,
                modified_ms: Some(*modified_ms),
                readonly: false,
            }),
            Some(Node::Dir) => Ok(FileMetadata {
                is_dir: true,
                is_symlink: false,
                len: 0,
                modified_ms: Some(0),
                readonly: false,
            }),
            None => Err(PlatformError::NotFound {
                path: path.to_path_buf(),
            }),
        }
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        let key = self.key(path);
        let nodes = self.nodes.lock().expect("memory fs poisoned");
        if !matches!(nodes.get(&key).map(|e| &e.node), Some(Node::Dir)) {
            return Err(PlatformError::NotFound {
                path: path.to_path_buf(),
            });
        }
        let prefix = if key == "/" {
            "/".to_string()
        } else {
            format!("{key}/")
        };
        let mut out = Vec::new();
        for (child_key, entry) in nodes.iter() {
            let Some(rest) = child_key.strip_prefix(&prefix) else {
                continue;
            };
            // Direct children only.
            if rest.is_empty() || rest.contains('/') {
                continue;
            }
            // Report the name as it was created, not the folded lookup key.
            let display_name = entry.display.rsplit('/').next().unwrap_or(rest).to_string();
            let node = &entry.node;
            let metadata = match node {
                Node::File { data, modified_ms } => FileMetadata {
                    is_dir: false,
                    is_symlink: false,
                    len: data.len() as u64,
                    modified_ms: Some(*modified_ms),
                    readonly: false,
                },
                Node::Dir => FileMetadata {
                    is_dir: true,
                    is_symlink: false,
                    len: 0,
                    modified_ms: Some(0),
                    readonly: false,
                },
            };
            out.push(DirEntry {
                path: PathBuf::from(&entry.display),
                file_name: display_name,
                metadata,
            });
        }
        Ok(out)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        let key = self.key(path);
        let nodes = self.nodes.lock().expect("memory fs poisoned");
        if let Some(entry) = nodes.get(&key) {
            Ok(PathBuf::from(&entry.display))
        } else {
            Err(PlatformError::NotFound {
                path: path.to_path_buf(),
            })
        }
    }

    fn is_case_sensitive(&self, _dir: &Path) -> Result<bool> {
        Ok(self.case_sensitive)
    }

    fn sync_dir(&self, _dir: &Path) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_and_list_round_trip() {
        let fs = MemoryFileSystem::default();
        fs.create_dir_all(Path::new("/vault/Notes")).unwrap();
        fs.write_atomic(Path::new("/vault/Notes/a.md"), b"hello")
            .unwrap();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/Notes/a.md")).unwrap(),
            "hello"
        );
        let listing = fs.read_dir(Path::new("/vault/Notes")).unwrap();
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].file_name, "a.md");
    }

    #[test]
    fn read_dir_returns_direct_children_only() {
        let fs = MemoryFileSystem::default();
        fs.create_dir_all(Path::new("/v/a/b")).unwrap();
        fs.write_atomic(Path::new("/v/a/b/deep.md"), b"x").unwrap();
        fs.write_atomic(Path::new("/v/a/top.md"), b"x").unwrap();

        let names: Vec<_> = fs
            .read_dir(Path::new("/v/a"))
            .unwrap()
            .into_iter()
            .map(|e| e.file_name)
            .collect();
        assert_eq!(names, vec!["b".to_string(), "top.md".to_string()]);
    }

    #[test]
    fn case_insensitive_mode_folds_names_like_ntfs() {
        let fs = MemoryFileSystem::new(false);
        fs.create_dir_all(Path::new("/v")).unwrap();
        fs.write_atomic(Path::new("/v/MyNote.md"), b"one").unwrap();
        assert_eq!(fs.read_to_string(Path::new("/v/mynote.md")).unwrap(), "one");
    }

    #[test]
    fn case_insensitive_mode_preserves_the_name_as_written() {
        // NTFS folds case for lookup but stores the name you gave it. A double
        // that lowercased stored names would hide bugs rather than expose them.
        let fs = MemoryFileSystem::new(false);
        fs.create_dir_all(Path::new("/v")).unwrap();
        fs.write_atomic(Path::new("/v/MyNote.md"), b"one").unwrap();

        let listing = fs.read_dir(Path::new("/v")).unwrap();
        assert_eq!(listing[0].file_name, "MyNote.md");

        // Rewriting through a different spelling keeps the original name.
        fs.write_atomic(Path::new("/v/MYNOTE.MD"), b"two").unwrap();
        let listing = fs.read_dir(Path::new("/v")).unwrap();
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].file_name, "MyNote.md");
    }

    #[test]
    fn case_sensitive_mode_keeps_names_distinct_like_ext4() {
        let fs = MemoryFileSystem::new(true);
        fs.create_dir_all(Path::new("/v")).unwrap();
        fs.write_atomic(Path::new("/v/MyNote.md"), b"one").unwrap();
        assert!(fs.read(Path::new("/v/mynote.md")).is_err());
    }

    #[test]
    fn renaming_a_directory_moves_its_whole_subtree() {
        let fs = MemoryFileSystem::default();
        fs.create_dir_all(Path::new("/v/old/inner")).unwrap();
        fs.write_atomic(Path::new("/v/old/inner/a.md"), b"x")
            .unwrap();
        fs.rename(Path::new("/v/old"), Path::new("/v/new")).unwrap();

        assert!(fs.exists(Path::new("/v/new/inner/a.md")));
        assert!(!fs.exists(Path::new("/v/old/inner/a.md")));
    }
}
