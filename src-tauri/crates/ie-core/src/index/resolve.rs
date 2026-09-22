//! Turning what the writer wrote into the file they meant.
//!
//! `[[Project Plan]]` has to find `Projects/2026/Project Plan.md` without the
//! writer having typed a path, and it has to keep finding it after the file
//! moves. The rules below are the ones people expect from a wiki, made
//! explicit and made deterministic — a link never silently picks a different
//! file between two runs, and when two files answer equally well the link is
//! flagged rather than guessed at.

use rusqlite::Connection;

use crate::error::Result;
use crate::vault::path::VaultPath;

/// What a link target points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// A single best file.
    Resolved {
        file_id: i64,
        path: VaultPath,
        /// Other files answered equally well. The link still resolves, to the
        /// deterministic best candidate, but the UI should offer to make it
        /// explicit.
        ambiguous: bool,
    },
    /// Nothing in the vault answers to this target. The UI offers to create it.
    Unresolved,
}

impl Resolution {
    pub fn file_id(&self) -> Option<i64> {
        match self {
            Resolution::Resolved { file_id, .. } => Some(*file_id),
            Resolution::Unresolved => None,
        }
    }

    pub fn is_ambiguous(&self) -> bool {
        matches!(
            self,
            Resolution::Resolved {
                ambiguous: true,
                ..
            }
        )
    }
}

/// Extensions a bare target may be assumed to have.
const IMPLICIT_EXTENSIONS: [&str; 2] = ["md", "canvas"];

/// Normalise a link target for matching: forward slashes, no leading `./`,
/// trimmed.
pub fn normalize_target(target: &str) -> String {
    let unified = target.trim().replace('\\', "/");
    let stripped = unified.strip_prefix("./").unwrap_or(&unified);
    stripped.trim_start_matches('/').to_string()
}

/// The final path segment, lowercased and stripped of any extension.
///
/// This is the key stored on every link row. Given a new file with stem
/// `project plan`, one indexed lookup finds every unresolved link that could
/// now point at it.
pub fn target_tail(target: &str) -> String {
    let normalized = normalize_target(target);
    let last = normalized.rsplit('/').next().unwrap_or("").to_string();
    match last.rfind('.') {
        Some(0) | None => last.to_lowercase(),
        Some(idx) => last[..idx].to_lowercase(),
    }
}

#[derive(Debug, Clone)]
struct Candidate {
    file_id: i64,
    path: VaultPath,
}

/// Resolve `target` against the files in the index.
pub fn resolve(conn: &Connection, target: &str) -> Result<Resolution> {
    let normalized = normalize_target(target);
    if normalized.is_empty() {
        return Ok(Resolution::Unresolved);
    }

    let tail = target_tail(&normalized);
    if tail.is_empty() {
        return Ok(Resolution::Unresolved);
    }

    // One indexed query gathers everything that could possibly match: any file
    // whose stem or full name equals the target's last segment. Ranking then
    // happens in Rust, where the rules are readable and testable.
    let mut stmt =
        conn.prepare_cached("SELECT id, path FROM files WHERE stem_fold = ?1 OR name_fold = ?1")?;
    let candidates: Vec<Candidate> = stmt
        .query_map([&tail], |row| {
            Ok(Candidate {
                file_id: row.get(0)?,
                path: VaultPath::from_indexed(row.get::<_, String>(1)?),
            })
        })?
        .filter_map(std::result::Result::ok)
        .collect();

    Ok(rank(&normalized, candidates))
}

/// Score and pick, given the candidate set.
///
/// Separated from the query so the policy can be tested without a database.
fn rank(normalized_target: &str, candidates: Vec<Candidate>) -> Resolution {
    let mut scored: Vec<(Score, Candidate)> = candidates
        .into_iter()
        .filter_map(|candidate| score(normalized_target, &candidate).map(|s| (s, candidate)))
        .collect();

    if scored.is_empty() {
        return Resolution::Unresolved;
    }

    // Best first; ties broken by path so the choice is stable across runs.
    scored.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.path.as_str().cmp(b.1.path.as_str()))
    });

    let best_score = scored[0].0;
    let equally_good = scored.iter().filter(|(s, _)| *s == best_score).count();
    let (_, winner) = scored.into_iter().next().expect("checked non-empty");

    Resolution::Resolved {
        file_id: winner.file_id,
        path: winner.path,
        ambiguous: equally_good > 1,
    }
}

/// How well a candidate answers a target. Lower is better.
///
/// The ordering encodes the policy:
///   0 — the target is the file's exact path
///   1 — the target is the exact path once an implicit `.md` is added
///   2 — same, ignoring case
///   3 — the target is a path suffix of the file, exact case
///   4 — the target is a path suffix, ignoring case
/// Within a tier, a shallower file wins, which is the "shortest unique path"
/// rule people expect from a wiki.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Score {
    tier: u8,
    depth: usize,
}

fn score(target: &str, candidate: &Candidate) -> Option<Score> {
    let path = candidate.path.as_str();
    let depth = candidate.path.depth();

    if path == target {
        return Some(Score { tier: 0, depth });
    }
    for extension in IMPLICIT_EXTENSIONS {
        if path == format!("{target}.{extension}") {
            return Some(Score { tier: 1, depth });
        }
    }

    let path_fold = path.to_lowercase();
    let target_fold = target.to_lowercase();
    if path_fold == target_fold
        || IMPLICIT_EXTENSIONS
            .iter()
            .any(|ext| path_fold == format!("{target_fold}.{ext}"))
    {
        return Some(Score { tier: 2, depth });
    }

    if is_path_suffix(path, target) {
        return Some(Score { tier: 3, depth });
    }
    if is_path_suffix(&path_fold, &target_fold) {
        return Some(Score { tier: 4, depth });
    }

    None
}

/// Does `path` end with `suffix`, aligned on a path separator?
///
/// `Projects/Alpha/Plan.md` ends with `Alpha/Plan`, but
/// `Projects/BetaAlpha/Plan.md` does not — matching mid-segment would make
/// links resolve to files the writer never meant.
fn is_path_suffix(path: &str, suffix: &str) -> bool {
    let candidates: Vec<String> = std::iter::once(suffix.to_string())
        .chain(
            IMPLICIT_EXTENSIONS
                .iter()
                .map(|ext| format!("{suffix}.{ext}")),
        )
        .collect();

    candidates.iter().any(|candidate| {
        path.len() > candidate.len()
            && path.ends_with(candidate.as_str())
            && path.as_bytes()[path.len() - candidate.len() - 1] == b'/'
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates(paths: &[&str]) -> Vec<Candidate> {
        paths
            .iter()
            .enumerate()
            .map(|(i, p)| Candidate {
                file_id: i as i64 + 1,
                path: VaultPath::parse(p).unwrap(),
            })
            .collect()
    }

    fn resolved_path(target: &str, paths: &[&str]) -> Option<String> {
        match rank(&normalize_target(target), candidates(paths)) {
            Resolution::Resolved { path, .. } => Some(path.as_str().to_string()),
            Resolution::Unresolved => None,
        }
    }

    #[test]
    fn a_bare_title_finds_the_note_anywhere_in_the_vault() {
        assert_eq!(
            resolved_path("Project Plan", &["Projects/2026/Project Plan.md"]).as_deref(),
            Some("Projects/2026/Project Plan.md")
        );
    }

    #[test]
    fn a_full_path_wins_over_a_bare_name_match() {
        assert_eq!(
            resolved_path(
                "Archive/Plan",
                &["Plan.md", "Archive/Plan.md", "Projects/Plan.md"]
            )
            .as_deref(),
            Some("Archive/Plan.md")
        );
    }

    #[test]
    fn a_shallower_file_wins_when_several_share_a_name() {
        assert_eq!(
            resolved_path("Plan", &["Deep/Nested/Plan.md", "Plan.md"]).as_deref(),
            Some("Plan.md")
        );
    }

    #[test]
    fn an_explicit_extension_matches_the_file_exactly() {
        assert_eq!(
            resolved_path("diagram.png", &["Attachments/diagram.png"]).as_deref(),
            Some("Attachments/diagram.png")
        );
    }

    #[test]
    fn a_canvas_file_resolves_from_a_bare_name() {
        assert_eq!(
            resolved_path("Roadmap", &["Boards/Roadmap.canvas"]).as_deref(),
            Some("Boards/Roadmap.canvas")
        );
    }

    #[test]
    fn matching_only_happens_on_whole_path_segments() {
        // `Alpha/Plan` must not match `BetaAlpha/Plan`.
        assert_eq!(resolved_path("Alpha/Plan", &["BetaAlpha/Plan.md"]), None);
    }

    #[test]
    fn exact_case_beats_a_case_insensitive_match() {
        assert_eq!(
            resolved_path("MyNote", &["mynote.md", "MyNote.md"]).as_deref(),
            Some("MyNote.md")
        );
    }

    #[test]
    fn a_link_still_resolves_when_only_the_case_differs() {
        // Written on Windows where case did not matter, opened on Linux.
        assert_eq!(
            resolved_path("mynote", &["MyNote.md"]).as_deref(),
            Some("MyNote.md")
        );
    }

    #[test]
    fn two_equally_good_candidates_resolve_deterministically_and_are_flagged() {
        let first = rank("Plan", candidates(&["Beta/Plan.md", "Alpha/Plan.md"]));
        let second = rank("Plan", candidates(&["Alpha/Plan.md", "Beta/Plan.md"]));
        assert!(first.is_ambiguous());
        assert!(second.is_ambiguous());
        // The chosen file must not depend on the order rows came back in.
        let chosen = |r: &Resolution| match r {
            Resolution::Resolved { path, .. } => path.as_str().to_string(),
            Resolution::Unresolved => panic!("expected a resolution"),
        };
        assert_eq!(chosen(&first), "Alpha/Plan.md");
        assert_eq!(chosen(&first), chosen(&second));
    }

    #[test]
    fn an_unambiguous_match_is_not_flagged() {
        assert!(!rank("Plan", candidates(&["Plan.md"])).is_ambiguous());
    }

    #[test]
    fn nothing_matching_resolves_to_nothing() {
        assert_eq!(
            rank("Ghost", candidates(&["Plan.md"])),
            Resolution::Unresolved
        );
        assert_eq!(rank("Ghost", Vec::new()), Resolution::Unresolved);
    }

    #[test]
    fn target_normalization_accepts_windows_separators_and_relative_prefixes() {
        assert_eq!(normalize_target("  ./Notes\\A.md "), "Notes/A.md");
        assert_eq!(normalize_target("/Notes/A.md"), "Notes/A.md");
    }

    #[test]
    fn the_tail_key_strips_folders_and_extensions() {
        assert_eq!(target_tail("Projects/2026/Project Plan"), "project plan");
        assert_eq!(target_tail("Projects/2026/Project Plan.md"), "project plan");
        assert_eq!(target_tail("diagram.PNG"), "diagram");
        assert_eq!(target_tail(".hidden"), ".hidden");
    }

    #[test]
    fn resolution_against_a_live_index_matches_the_ranking_rules() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::index::schema::initialize(&conn).unwrap();
        for (id, path) in [
            (1, "Plan.md"),
            (2, "Projects/Plan.md"),
            (3, "Attachments/diagram.png"),
        ] {
            let vp = VaultPath::parse(path).unwrap();
            conn.execute(
                "INSERT INTO files(id, path, path_fold, name, name_fold, stem_fold, ext, kind, size, mtime_ms, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'note', 0, 0, 0)",
                rusqlite::params![
                    id,
                    vp.as_str(),
                    vp.fold(),
                    vp.file_name(),
                    vp.file_name().to_lowercase(),
                    vp.stem().to_lowercase(),
                    vp.extension()
                ],
            )
            .unwrap();
        }

        assert_eq!(resolve(&conn, "Plan").unwrap().file_id(), Some(1));
        assert_eq!(resolve(&conn, "Projects/Plan").unwrap().file_id(), Some(2));
        assert_eq!(resolve(&conn, "diagram.png").unwrap().file_id(), Some(3));
        assert_eq!(resolve(&conn, "Nothing").unwrap(), Resolution::Unresolved);
        assert_eq!(resolve(&conn, "   ").unwrap(), Resolution::Unresolved);
    }
}
