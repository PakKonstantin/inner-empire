//! Templates and daily notes.
//!
//! A template is an ordinary Markdown file in the vault's template folder. The
//! only thing that makes it a template is that `{{variables}}` in it are
//! expanded when it is inserted.
//!
//! The variable set is fixed and small. A template language that can call into
//! the app would be a scripting surface reachable by anything that can write a
//! file into the vault, and the brief is explicit that nothing should be able
//! to act without the user asking.

pub mod datefmt;

use ie_platform::SharedClock;

use crate::error::Result;
use crate::vault::path::VaultPath;
use crate::vault::settings::DailyNoteSettings;
use crate::vault::{Collision, FileOps};

/// What the expander knows about the note being created.
#[derive(Debug, Clone)]
pub struct TemplateContext {
    pub title: String,
    pub path: VaultPath,
    pub now_ms: i64,
    pub offset_seconds: i32,
}

impl TemplateContext {
    pub fn new(title: impl Into<String>, path: VaultPath, clock: &SharedClock) -> Self {
        Self {
            title: title.into(),
            path,
            now_ms: clock.now_ms(),
            offset_seconds: clock.local_offset_seconds(),
        }
    }
}

/// Expand `{{variables}}` in a template.
///
/// Recognised:
///
/// ```text
/// {{title}}              the new note's title
/// {{path}}               its vault path
/// {{folder}}             the folder it is being created in
/// {{date}}               today, as YYYY-MM-DD
/// {{time}}               now, as HH:mm
/// {{datetime}}           both
/// {{date:FORMAT}}        today in a custom format
/// {{time:FORMAT}}        now in a custom format
/// {{yesterday}} {{tomorrow}}   adjacent days, as YYYY-MM-DD
/// ```
///
/// An unknown variable is left exactly as written, so a template containing
/// `{{mustache}}` for some other tool is not silently emptied.
pub fn expand(template: &str, context: &TemplateContext) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'{' && index + 1 < bytes.len() && bytes[index + 1] == b'{' {
            if let Some(close) = template[index + 2..].find("}}") {
                let name = &template[index + 2..index + 2 + close];
                match substitute(name.trim(), context) {
                    Some(value) => out.push_str(&value),
                    None => out.push_str(&template[index..index + 2 + close + 2]),
                }
                index += 2 + close + 2;
                continue;
            }
        }
        // Push one whole character so multi-byte text survives.
        let ch = template[index..].chars().next().expect("index is on a boundary");
        out.push(ch);
        index += ch.len_utf8();
    }

    out
}

const MILLIS_PER_DAY: i64 = 86_400_000;

fn substitute(name: &str, context: &TemplateContext) -> Option<String> {
    let at = |offset_days: i64| {
        datefmt::at(
            context.now_ms + offset_days * MILLIS_PER_DAY,
            context.offset_seconds,
        )
    };

    let (key, argument) = match name.find(':') {
        Some(index) => (&name[..index], Some(name[index + 1..].trim())),
        None => (name, None),
    };

    Some(match (key.to_lowercase().as_str(), argument) {
        ("title", _) => context.title.clone(),
        ("path", _) => context.path.as_str().to_string(),
        ("folder", _) => context.path.parent().as_str().to_string(),
        ("date", Some(format)) => datefmt::format(format, at(0)),
        ("date", None) => datefmt::format("YYYY-MM-DD", at(0)),
        ("time", Some(format)) => datefmt::format(format, at(0)),
        ("time", None) => datefmt::format("HH:mm", at(0)),
        ("datetime", Some(format)) => datefmt::format(format, at(0)),
        ("datetime", None) => datefmt::format("YYYY-MM-DD HH:mm", at(0)),
        ("yesterday", format) => datefmt::format(format.unwrap_or("YYYY-MM-DD"), at(-1)),
        ("tomorrow", format) => datefmt::format(format.unwrap_or("YYYY-MM-DD"), at(1)),
        _ => return None,
    })
}

/// A template the user can pick from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub path: VaultPath,
    pub name: String,
}

/// List the templates in the vault's template folder.
pub fn list(ops: &FileOps, folder: &VaultPath) -> Result<Vec<TemplateInfo>> {
    if !ops.exists(folder) {
        return Ok(Vec::new());
    }
    let mut templates: Vec<TemplateInfo> = ops
        .list_folder(folder)?
        .into_iter()
        .filter(|(path, is_dir)| {
            !is_dir && matches!(path.extension().as_deref(), Some("md" | "markdown"))
        })
        .map(|(path, _)| TemplateInfo {
            name: path.stem().to_string(),
            path,
        })
        .collect();
    templates.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(templates)
}

/// Read a template and expand it.
pub fn render(ops: &FileOps, template: &VaultPath, context: &TemplateContext) -> Result<String> {
    let source = ops.read(template)?;
    Ok(expand(&source, context))
}

/// Where today's daily note lives.
pub fn daily_note_path(
    settings: &DailyNoteSettings,
    now_ms: i64,
    offset_seconds: i32,
    day_offset: i64,
) -> Result<VaultPath> {
    let at = datefmt::at(now_ms + day_offset * MILLIS_PER_DAY, offset_seconds);
    let name = datefmt::format(&settings.format, at);
    // The format is user-supplied, so it can contain anything; sanitising here
    // means a bad format yields an odd filename rather than a failure to
    // create today's note.
    let safe = crate::vault::path::sanitize_segment(&name);
    settings.folder.join(&format!("{safe}.md"))
}

/// Open today's daily note, creating it from the template if it is not there.
///
/// Returns the path and whether it had to be created.
pub fn ensure_daily_note(
    ops: &FileOps,
    settings: &DailyNoteSettings,
    clock: &SharedClock,
    day_offset: i64,
) -> Result<(VaultPath, bool)> {
    let path = daily_note_path(
        settings,
        clock.now_ms(),
        clock.local_offset_seconds(),
        day_offset,
    )?;
    if ops.exists(&path) {
        return Ok((path, false));
    }

    let context = TemplateContext {
        title: path.stem().to_string(),
        path: path.clone(),
        now_ms: clock.now_ms() + day_offset * MILLIS_PER_DAY,
        offset_seconds: clock.local_offset_seconds(),
    };

    let contents = match &settings.template {
        Some(template) if ops.exists(template) => render(ops, template, &context)?,
        _ => format!("# {}\n\n", context.title),
    };

    let created = ops.create_note(&path, &contents, Collision::Fail)?;
    Ok((created, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ie_platform::{FileSystem, FixedClock, MemoryFileSystem, SharedFileSystem};
    use std::sync::Arc;

    /// 2026-09-17T14:05:09Z
    const NOW: i64 = 1_789_653_909_000;

    fn context() -> TemplateContext {
        TemplateContext {
            title: "My Note".into(),
            path: VaultPath::parse("Projects/My Note.md").unwrap(),
            now_ms: NOW,
            offset_seconds: 0,
        }
    }

    fn harness() -> (FileOps, SharedClock) {
        let fs: SharedFileSystem = Arc::new(MemoryFileSystem::default());
        let root = std::path::PathBuf::from("/vault");
        fs.create_dir_all(&root).unwrap();
        (
            FileOps::new(fs, &root),
            Arc::new(FixedClock::new(NOW)) as SharedClock,
        )
    }

    #[test]
    fn the_documented_variables_expand() {
        let c = context();
        assert_eq!(expand("{{title}}", &c), "My Note");
        assert_eq!(expand("{{path}}", &c), "Projects/My Note.md");
        assert_eq!(expand("{{folder}}", &c), "Projects");
        assert_eq!(expand("{{date}}", &c), "2026-09-17");
        assert_eq!(expand("{{time}}", &c), "14:05");
        assert_eq!(expand("{{datetime}}", &c), "2026-09-17 14:05");
    }

    #[test]
    fn a_variable_can_carry_its_own_format() {
        let c = context();
        assert_eq!(expand("{{date:dddd}}", &c), "Thursday");
        assert_eq!(expand("{{date:YYYY/MM}}", &c), "2026/09");
        assert_eq!(expand("{{time:HH.mm.ss}}", &c), "14.05.09");
    }

    #[test]
    fn adjacent_days_are_available_for_daily_note_navigation() {
        let c = context();
        assert_eq!(expand("{{yesterday}}", &c), "2026-09-16");
        assert_eq!(expand("{{tomorrow}}", &c), "2026-09-18");
    }

    #[test]
    fn variable_names_are_matched_case_insensitively_and_trimmed() {
        let c = context();
        assert_eq!(expand("{{ TITLE }}", &c), "My Note");
        assert_eq!(expand("{{Title}}", &c), "My Note");
    }

    #[test]
    fn an_unknown_variable_is_left_exactly_as_written() {
        let c = context();
        assert_eq!(expand("{{mustache}}", &c), "{{mustache}}");
        assert_eq!(expand("{{a.b.c}}", &c), "{{a.b.c}}");
    }

    #[test]
    fn an_unclosed_variable_is_left_alone() {
        let c = context();
        assert_eq!(expand("{{title", &c), "{{title");
    }

    #[test]
    fn surrounding_markdown_is_untouched() {
        let c = context();
        let template = "---\ntitle: {{title}}\ncreated: {{date}}\n---\n\n# {{title}}\n\n- [ ] task\n";
        assert_eq!(
            expand(template, &c),
            "---\ntitle: My Note\ncreated: 2026-09-17\n---\n\n# My Note\n\n- [ ] task\n"
        );
    }

    #[test]
    fn multi_byte_text_survives_expansion() {
        let c = context();
        assert_eq!(expand("café {{title}} — 日本語", &c), "café My Note — 日本語");
    }

    #[test]
    fn templates_are_listed_alphabetically_and_only_markdown_counts() {
        let (ops, _) = harness();
        let folder = VaultPath::parse("Templates").unwrap();
        for name in ["Project.md", "Daily Note.md", "notes.txt", "Meeting.md"] {
            ops.create_note(&folder.join(name).unwrap(), "", Collision::Fail)
                .unwrap();
        }

        let names: Vec<String> = list(&ops, &folder)
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, vec!["Daily Note", "Meeting", "Project"]);
    }

    #[test]
    fn a_missing_template_folder_lists_nothing_rather_than_failing() {
        let (ops, _) = harness();
        assert!(list(&ops, &VaultPath::parse("Nowhere").unwrap())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn the_daily_note_path_follows_the_configured_folder_and_format() {
        let settings = DailyNoteSettings {
            folder: VaultPath::parse("Journal/2026").unwrap(),
            format: "YYYY-MM-DD dddd".into(),
            ..DailyNoteSettings::default()
        };
        let path = daily_note_path(&settings, NOW, 0, 0).unwrap();
        assert_eq!(path.as_str(), "Journal/2026/2026-09-17 Thursday.md");
    }

    #[test]
    fn a_format_producing_illegal_characters_still_yields_a_usable_path() {
        let settings = DailyNoteSettings {
            format: "YYYY/MM/DD".into(),
            ..DailyNoteSettings::default()
        };
        let path = daily_note_path(&settings, NOW, 0, 0).unwrap();
        assert_eq!(path.as_str(), "Daily/2026-09-17.md");
    }

    #[test]
    fn opening_the_daily_note_creates_it_the_first_time_only() {
        let (ops, clock) = harness();
        let settings = DailyNoteSettings::default();

        let (path, created) = ensure_daily_note(&ops, &settings, &clock, 0).unwrap();
        assert!(created);
        assert_eq!(path.as_str(), "Daily/2026-09-17.md");
        assert_eq!(ops.read(&path).unwrap(), "# 2026-09-17\n\n");

        ops.write(&path, "# 2026-09-17\n\nMy notes for today.\n").unwrap();
        let (again, created_again) = ensure_daily_note(&ops, &settings, &clock, 0).unwrap();
        assert!(!created_again);
        assert_eq!(again, path);
        assert!(
            ops.read(&path).unwrap().contains("My notes for today"),
            "an existing daily note must never be overwritten"
        );
    }

    #[test]
    fn a_daily_note_uses_its_template_when_one_is_configured() {
        let (ops, clock) = harness();
        let template = VaultPath::parse("Templates/Daily.md").unwrap();
        ops.create_note(
            &template,
            "---\ndate: {{date}}\n---\n\n# {{title}}\n\n## Yesterday\n[[{{yesterday}}]]\n",
            Collision::Fail,
        )
        .unwrap();

        let settings = DailyNoteSettings {
            template: Some(template),
            ..DailyNoteSettings::default()
        };
        let (path, _) = ensure_daily_note(&ops, &settings, &clock, 0).unwrap();

        let contents = ops.read(&path).unwrap();
        assert!(contents.contains("date: 2026-09-17"), "{contents}");
        assert!(contents.contains("# 2026-09-17"), "{contents}");
        assert!(contents.contains("[[2026-09-16]]"), "{contents}");
    }

    #[test]
    fn a_configured_template_that_is_missing_falls_back_to_a_plain_note() {
        let (ops, clock) = harness();
        let settings = DailyNoteSettings {
            template: Some(VaultPath::parse("Templates/Gone.md").unwrap()),
            ..DailyNoteSettings::default()
        };
        let (path, created) = ensure_daily_note(&ops, &settings, &clock, 0).unwrap();
        assert!(created);
        assert!(ops.read(&path).unwrap().starts_with("# 2026-09-17"));
    }

    #[test]
    fn yesterdays_note_can_be_opened_with_a_day_offset() {
        let (ops, clock) = harness();
        let (path, _) = ensure_daily_note(&ops, &DailyNoteSettings::default(), &clock, -1).unwrap();
        assert_eq!(path.as_str(), "Daily/2026-09-16.md");
    }
}
