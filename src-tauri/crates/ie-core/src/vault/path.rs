//! `VaultPath` — how everything in the system names a file.
//!
//! The index, every link on disk, every workspace entry, every canvas node and
//! every IPC payload refers to files by `VaultPath` and never by an OS path.
//! That single decision is what makes a vault open identically on Windows and
//! Linux: nothing persisted anywhere contains a drive letter, a backslash or
//! an absolute path.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

use crate::error::{CoreError, Result};

/// A location inside a vault.
///
/// Invariants, established at construction and preserved by every operation:
///
/// * relative to the vault root — never absolute, never a drive prefix
/// * `/`-separated on every platform, including Windows
/// * no `.` or `..` components, so it cannot address anything outside the vault
/// * no empty components and no trailing separator
/// * Unicode NFC, so a name typed on one platform matches the same name typed
///   on another (macOS historically stores NFD, and some Linux tools do too)
///
/// The inner string is private precisely so these cannot be broken by a caller.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct VaultPath(String);

/// Names the app owns inside a vault. Never indexed, never shown in the
/// explorer, and never a valid link target.
pub const APP_DIR: &str = ".inner-empire";

impl VaultPath {
    /// The vault root itself.
    pub fn root() -> Self {
        VaultPath(String::new())
    }

    /// Build from a `/`-separated relative string, normalising and validating.
    ///
    /// Accepts backslashes as separators too, because links pasted from a
    /// Windows path would otherwise be silently wrong; they are converted.
    pub fn parse(input: &str) -> Result<Self> {
        let unified = input.replace('\\', "/");
        let mut segments: Vec<String> = Vec::new();

        for raw in unified.split('/') {
            match raw {
                "" | "." => continue,
                ".." => {
                    // Refuse rather than silently resolve: a link containing
                    // `..` is either a mistake or an escape attempt, and both
                    // deserve to be visible.
                    return Err(CoreError::InvalidName {
                        name: input.to_string(),
                        reason: "a vault path may not contain '..'".into(),
                    });
                }
                segment => {
                    let normalized: String = segment.nfc().collect();
                    validate_segment(&normalized)?;
                    segments.push(normalized);
                }
            }
        }

        Ok(VaultPath(segments.join("/")))
    }

    /// Build from an OS path that is already known to be inside `root`.
    ///
    /// Iterating `Components` is what makes this correct on both platforms:
    /// the standard library already knows that Windows accepts `\` and `/`
    /// while POSIX accepts only `/`, so no separator ever appears literally
    /// in this code.
    pub fn from_fs_path(root: &Path, path: &Path) -> Result<Self> {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| CoreError::OutsideVault {
                path: path.to_path_buf(),
            })?;

        let mut segments: Vec<String> = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(part) => {
                    let text: String = part.to_string_lossy().nfc().collect();
                    segments.push(text);
                }
                Component::CurDir => continue,
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(CoreError::OutsideVault {
                        path: path.to_path_buf(),
                    })
                }
            }
        }
        Ok(VaultPath(segments.join("/")))
    }

    /// Resolve back to an OS path under `root`.
    ///
    /// `PathBuf::push` inserts the platform's own separator, so this produces
    /// `C:\Vault\Notes\a.md` on Windows and `/home/me/Vault/Notes/a.md` on
    /// Linux from the identical `Notes/a.md`.
    pub fn to_fs_path(&self, root: &Path) -> PathBuf {
        let mut out = root.to_path_buf();
        for segment in self.segments() {
            out.push(segment);
        }
        out
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/').filter(|s| !s.is_empty())
    }

    /// The final component, including any extension.
    pub fn file_name(&self) -> &str {
        match self.0.rfind('/') {
            Some(idx) => &self.0[idx + 1..],
            None => &self.0,
        }
    }

    /// The final component without its extension. This is the note's title for
    /// linking purposes: `[[Project Plan]]` targets `Project Plan.md`.
    pub fn stem(&self) -> &str {
        let name = self.file_name();
        match name.rfind('.') {
            Some(0) | None => name,
            Some(idx) => &name[..idx],
        }
    }

    /// Lowercase extension without the dot, or `None` for extensionless and
    /// dot-prefixed names.
    pub fn extension(&self) -> Option<String> {
        let name = self.file_name();
        match name.rfind('.') {
            Some(0) | None => None,
            Some(idx) => Some(name[idx + 1..].to_ascii_lowercase()),
        }
    }

    /// The containing folder, or the root.
    pub fn parent(&self) -> VaultPath {
        match self.0.rfind('/') {
            Some(idx) => VaultPath(self.0[..idx].to_string()),
            None => VaultPath::root(),
        }
    }

    /// Append a single already-validated component.
    pub fn join(&self, segment: &str) -> Result<VaultPath> {
        let normalized: String = segment.nfc().collect();
        validate_segment(&normalized)?;
        Ok(if self.0.is_empty() {
            VaultPath(normalized)
        } else {
            VaultPath(format!("{}/{}", self.0, normalized))
        })
    }

    /// Same path, different final component.
    pub fn with_file_name(&self, name: &str) -> Result<VaultPath> {
        self.parent().join(name)
    }

    /// Same path, different extension. Pass `None` to drop it.
    pub fn with_extension(&self, extension: Option<&str>) -> Result<VaultPath> {
        let new_name = match extension {
            Some(ext) => format!("{}.{}", self.stem(), ext),
            None => self.stem().to_string(),
        };
        self.with_file_name(&new_name)
    }

    /// The key used for collision detection and case-insensitive lookup.
    ///
    /// Stored alongside the exact path in the index so a vault authored on
    /// ext4 can be checked for names that would collapse on NTFS.
    pub fn fold(&self) -> String {
        self.0.to_lowercase()
    }

    /// Is this path inside `folder` (at any depth)?
    pub fn is_within(&self, folder: &VaultPath) -> bool {
        if folder.is_root() {
            return true;
        }
        self.0 == folder.0 || self.0.starts_with(&format!("{}/", folder.0))
    }

    /// Does this path live in the app's private directory?
    pub fn is_app_internal(&self) -> bool {
        self.0 == APP_DIR || self.0.starts_with(&format!("{APP_DIR}/"))
    }

    /// Depth below the root: `root` is 0, `a.md` is 1, `a/b.md` is 2.
    pub fn depth(&self) -> usize {
        self.segments().count()
    }

    /// Construct without validating. Only for reading rows the indexer itself
    /// wrote, where the value was validated on the way in and re-checking every
    /// row on every query is wasted work.
    pub(crate) fn from_indexed(value: String) -> Self {
        VaultPath(value)
    }
}

/// The default is the vault root, which is the only path that is always valid.
impl Default for VaultPath {
    fn default() -> Self {
        VaultPath::root()
    }
}

impl fmt::Display for VaultPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            f.write_str("/")
        } else {
            f.write_str(&self.0)
        }
    }
}

impl<'de> serde::Deserialize<'de> for VaultPath {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        VaultPath::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// Characters no platform we target accepts in a file name, plus the ones only
/// Windows rejects.
///
/// The stricter, unioned rule is applied everywhere on purpose: a vault whose
/// names are legal on Linux but not on Windows is not portable, and the brief
/// makes portability a requirement rather than a nicety.
const FORBIDDEN_CHARS: [char; 9] = ['<', '>', ':', '"', '|', '?', '*', '/', '\\'];

/// Device names MS-DOS reserved, which Windows still refuses — with or without
/// an extension, so `CON.md` is rejected too.
const RESERVED_STEMS: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// The longest a single path component may be. 255 is the limit on ext4, NTFS
/// and APFS alike.
pub const MAX_SEGMENT_BYTES: usize = 255;

/// Validate one path component against the union of Windows and POSIX rules.
pub fn validate_segment(segment: &str) -> Result<()> {
    let invalid = |reason: &str| {
        Err(CoreError::InvalidName {
            name: segment.to_string(),
            reason: reason.to_string(),
        })
    };

    if segment.is_empty() {
        return invalid("a name may not be empty");
    }
    if segment.len() > MAX_SEGMENT_BYTES {
        return invalid(&format!(
            "a name may be at most {MAX_SEGMENT_BYTES} bytes long"
        ));
    }
    if let Some(bad) = segment.chars().find(|c| FORBIDDEN_CHARS.contains(c)) {
        return invalid(&format!("the character {bad:?} is not allowed"));
    }
    if segment.chars().any(|c| (c as u32) < 0x20 || c == '\u{7f}') {
        return invalid("control characters are not allowed");
    }
    // Windows strips these silently, so a file saved as "note " becomes "note"
    // and the link stops resolving.
    if segment.ends_with(' ') || segment.ends_with('.') {
        return invalid("a name may not end with a space or a dot");
    }
    if segment == "." || segment == ".." {
        return invalid("'.' and '..' are not usable names");
    }

    let stem_lower = match segment.find('.') {
        Some(0) => segment.to_ascii_lowercase(),
        Some(idx) => segment[..idx].to_ascii_lowercase(),
        None => segment.to_ascii_lowercase(),
    };
    if RESERVED_STEMS.contains(&stem_lower.as_str()) {
        return invalid(&format!(
            "{stem_lower} is a reserved device name on Windows"
        ));
    }

    Ok(())
}

/// Turn arbitrary text — a wiki link target, a pasted title, a template
/// expansion — into a name that is legal everywhere, without failing.
///
/// Used when the user asks to create a note from an unresolved link, where
/// refusing would be less useful than sanitising.
pub fn sanitize_segment(input: &str) -> String {
    let mut out: String = input
        .chars()
        .map(|c| {
            if FORBIDDEN_CHARS.contains(&c) || (c as u32) < 0x20 || c == '\u{7f}' {
                '-'
            } else {
                c
            }
        })
        .collect::<String>()
        .nfc()
        .collect();

    while out.ends_with(' ') || out.ends_with('.') {
        out.pop();
    }
    let trimmed = out.trim_start();
    if trimmed.len() != out.len() {
        out = trimmed.to_string();
    }

    if out.is_empty() {
        return "Untitled".to_string();
    }

    let stem_lower = match out.find('.') {
        Some(0) => out.to_ascii_lowercase(),
        Some(idx) => out[..idx].to_ascii_lowercase(),
        None => out.to_ascii_lowercase(),
    };
    if RESERVED_STEMS.contains(&stem_lower.as_str()) {
        out.insert(0, '_');
    }

    // Truncate on a character boundary so the result stays valid UTF-8.
    while out.len() > MAX_SEGMENT_BYTES {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_normalizes_separators_and_redundant_segments() {
        assert_eq!(VaultPath::parse("Notes/a.md").unwrap().as_str(), "Notes/a.md");
        assert_eq!(VaultPath::parse("Notes\\a.md").unwrap().as_str(), "Notes/a.md");
        assert_eq!(VaultPath::parse("/Notes//./a.md").unwrap().as_str(), "Notes/a.md");
        assert_eq!(VaultPath::parse("Notes/").unwrap().as_str(), "Notes");
    }

    #[test]
    fn parse_refuses_to_climb_above_the_vault() {
        let err = VaultPath::parse("../secrets.md").unwrap_err();
        assert_eq!(err.code(), "invalid_name");
        assert!(VaultPath::parse("Notes/../../etc/passwd").is_err());
    }

    #[test]
    fn from_fs_path_produces_the_same_value_on_both_platforms() {
        // The separator the OS uses is invisible to the result: both spellings
        // of the same location yield the identical VaultPath.
        let posix = VaultPath::from_fs_path(Path::new("/home/me/Vault"), Path::new("/home/me/Vault/Notes/a.md"));
        assert_eq!(posix.unwrap().as_str(), "Notes/a.md");

        let nested = VaultPath::from_fs_path(
            Path::new("/v"),
            &Path::new("/v").join("Projects").join("2026").join("plan.md"),
        );
        assert_eq!(nested.unwrap().as_str(), "Projects/2026/plan.md");
    }

    #[test]
    fn from_fs_path_rejects_paths_outside_the_root() {
        let err = VaultPath::from_fs_path(Path::new("/home/me/Vault"), Path::new("/etc/passwd"))
            .unwrap_err();
        assert_eq!(err.code(), "outside_vault");
    }

    #[test]
    fn round_trip_through_the_filesystem_preserves_the_path() {
        let root = Path::new("/home/me/Vault");
        let original = VaultPath::parse("Projects/Deep/Note.md").unwrap();
        let fs_path = original.to_fs_path(root);
        let back = VaultPath::from_fs_path(root, &fs_path).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn name_stem_extension_and_parent() {
        let p = VaultPath::parse("Projects/2026/Plan.MD").unwrap();
        assert_eq!(p.file_name(), "Plan.MD");
        assert_eq!(p.stem(), "Plan");
        assert_eq!(p.extension().as_deref(), Some("md"));
        assert_eq!(p.parent().as_str(), "Projects/2026");
        assert_eq!(p.parent().parent().as_str(), "Projects");
        assert!(p.parent().parent().parent().is_root());
        assert_eq!(p.depth(), 3);
    }

    #[test]
    fn dotfiles_have_no_extension() {
        let p = VaultPath::parse(".gitignore").unwrap();
        assert_eq!(p.stem(), ".gitignore");
        assert_eq!(p.extension(), None);
    }

    #[test]
    fn fold_is_used_for_case_collision_detection() {
        let a = VaultPath::parse("Notes/MyNote.md").unwrap();
        let b = VaultPath::parse("Notes/mynote.md").unwrap();
        assert_ne!(a, b, "the two paths are distinct on a case-sensitive mount");
        assert_eq!(a.fold(), b.fold(), "but they collapse onto one another on NTFS");
    }

    #[test]
    fn containment_checks_respect_folder_boundaries() {
        let note = VaultPath::parse("Projects/Alpha/plan.md").unwrap();
        assert!(note.is_within(&VaultPath::parse("Projects").unwrap()));
        assert!(note.is_within(&VaultPath::root()));
        assert!(!note.is_within(&VaultPath::parse("Proj").unwrap()));
        assert!(!note.is_within(&VaultPath::parse("Projects/Beta").unwrap()));
    }

    #[test]
    fn app_directory_is_recognised() {
        assert!(VaultPath::parse(".inner-empire/index.db").unwrap().is_app_internal());
        assert!(VaultPath::parse(".inner-empire").unwrap().is_app_internal());
        assert!(!VaultPath::parse(".inner-empire-notes/a.md")
            .unwrap()
            .is_app_internal());
    }

    #[test]
    fn windows_forbidden_characters_are_rejected_on_every_platform() {
        for bad in ["a<b.md", "a>b.md", "a:b.md", "a\"b.md", "a|b.md", "a?b.md", "a*b.md"] {
            assert!(
                VaultPath::parse(bad).is_err(),
                "{bad} should be refused so the vault stays portable"
            );
        }
    }

    #[test]
    fn windows_reserved_device_names_are_rejected() {
        for bad in ["CON.md", "con", "nul.txt", "COM1.md", "lpt9"] {
            assert!(VaultPath::parse(bad).is_err(), "{bad} should be refused");
        }
        // Only the exact reserved stems, not anything containing them.
        assert!(VaultPath::parse("console.md").is_ok());
        assert!(VaultPath::parse("Concept.md").is_ok());
    }

    #[test]
    fn trailing_space_or_dot_is_rejected_because_windows_strips_it() {
        assert!(VaultPath::parse("note .md").is_ok());
        assert!(VaultPath::parse("note. ").is_err());
        assert!(VaultPath::parse("note ").is_err());
        assert!(VaultPath::parse("note.").is_err());
    }

    #[test]
    fn unicode_is_normalized_so_the_same_name_compares_equal() {
        // "é" as one codepoint vs. "e" plus a combining acute.
        let composed = VaultPath::parse("Caf\u{e9}.md").unwrap();
        let decomposed = VaultPath::parse("Cafe\u{301}.md").unwrap();
        assert_eq!(composed, decomposed);
    }

    #[test]
    fn sanitize_always_produces_a_usable_name() {
        assert_eq!(sanitize_segment("My: Project?"), "My- Project-");
        assert_eq!(sanitize_segment("  trailing dots... "), "trailing dots");
        assert_eq!(sanitize_segment(""), "Untitled");
        assert_eq!(sanitize_segment("   "), "Untitled");
        assert_eq!(sanitize_segment("CON"), "_CON");
        assert!(validate_segment(&sanitize_segment("a/b\\c:d")).is_ok());
    }

    #[test]
    fn sanitize_truncates_on_a_character_boundary() {
        let long = "é".repeat(300);
        let result = sanitize_segment(&long);
        assert!(result.len() <= MAX_SEGMENT_BYTES);
        assert!(validate_segment(&result).is_ok());
    }

    #[test]
    fn serde_round_trip_keeps_forward_slashes() {
        let original = VaultPath::parse("Projects/2026/Plan.md").unwrap();
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "\"Projects/2026/Plan.md\"");
        let back: VaultPath = serde_json::from_str(&json).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn deserializing_an_escaping_path_fails_rather_than_being_silently_clamped() {
        assert!(serde_json::from_str::<VaultPath>("\"../outside.md\"").is_err());
    }
}
