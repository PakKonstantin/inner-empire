/**
 * What is in Settings, so it can be searched.
 *
 * A person looking for the dark theme searches for "dark", and the setting is
 * called "Theme" — the word they are looking for is a *value*, not a label.
 * So each entry carries the words someone might reasonably arrive with,
 * alongside the label it actually has.
 *
 * This is a hand-written list, which would normally mean it drifts away from
 * the dialog the first time a field is added. `settingsIndex.test.ts` reads
 * the dialog's own source and fails when a field is missing from here, so the
 * drift is a broken test rather than a setting nobody can find.
 */

export type SettingsSection =
  | 'general'
  | 'editor'
  | 'files'
  | 'appearance'
  | 'hotkeys'
  | 'templates'
  | 'dailyNotes'
  | 'trash'
  | 'plugins'
  | 'about';

export interface SettingsEntry {
  section: SettingsSection;
  /** Exactly the label the field carries in the dialog. */
  label: string;
  /** The section's own name, for the result's second line. */
  sectionLabel: string;
  /** Words someone might search for that are not in the label. */
  keywords?: string;
}

export const SECTION_LABELS: Record<SettingsSection, string> = {
  general: 'General',
  editor: 'Editor',
  files: 'Files and links',
  appearance: 'Appearance',
  hotkeys: 'Keyboard shortcuts',
  templates: 'Templates',
  dailyNotes: 'Daily notes',
  trash: 'Trash',
  plugins: 'Plugins',
  about: 'About',
};

function entry(section: SettingsSection, label: string, keywords?: string): SettingsEntry {
  return { section, label, sectionLabel: SECTION_LABELS[section], keywords };
}

export const SETTINGS_INDEX: SettingsEntry[] = [
  entry('general', 'Reopen the last vault at launch', 'startup open restore session'),
  entry('general', 'Ask before moving a file to the trash', 'confirm delete prompt'),
  entry('general', 'Keep trashed files for', 'retention days purge empty'),

  entry('editor', 'Font size', 'text bigger smaller type scale'),
  entry('editor', 'Live preview', 'wysiwyg render inline formatting'),
  entry('editor', 'Limit line width', 'readable measure column width wide'),
  entry('editor', 'Line numbers', 'gutter'),
  entry('editor', 'Check spelling', 'spellcheck dictionary typos'),
  entry('editor', 'Continue lists on Enter', 'bullets numbering automatic'),
  entry('editor', 'Close brackets and quotes automatically', 'auto pairs parentheses'),
  entry('editor', 'Indent size', 'tab width spaces'),
  entry('editor', 'Indent with spaces', 'tabs whitespace'),

  entry('files', 'Update links when a note is renamed', 'rename refactor backlinks move'),
  entry('files', 'New links are written as', 'wiki markdown relative absolute shortest'),
  entry('files', 'New attachments go', 'images paste drop folder location'),
  entry('files', 'Attachment folder', 'images files paste drop location'),
  entry('files', 'Template folder', 'templates location'),
  entry(
    'files',
    'Open Markdown files with Inner Empire by default',
    'file association default handler open with windows shell',
  ),
  entry(
    'files',
    'Add \u201cOpen as vault\u201d to the folder right-click menu',
    'context menu shell integration explorer folder windows',
  ),

  entry('appearance', 'Theme', 'dark light system colour color night mode'),
  entry('appearance', 'Interface size', 'zoom scale bigger smaller ui'),
  entry('appearance', 'Show the status bar', 'word count footer bottom'),
  entry('appearance', 'Show tabs', 'tab bar strip'),

  entry('dailyNotes', 'Folder', 'daily journal location'),
  entry('dailyNotes', 'Filename format', 'date pattern journal naming'),
  entry('dailyNotes', 'Template', 'daily journal'),
  entry('dailyNotes', "Open today's note when the vault opens", 'daily journal startup'),
];

/**
 * Settings matching what was typed.
 *
 * Every word has to match something — the label, the section or the keywords
 * — so "dark theme" narrows rather than widening the way an "any word" match
 * would. A label match sorts first, because a person typing the name of a
 * setting means that setting.
 */
export function matchSettings(query: string): SettingsEntry[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [];

  const scored: { entry: SettingsEntry; score: number }[] = [];
  for (const candidate of SETTINGS_INDEX) {
    const label = candidate.label.toLowerCase();
    const haystack = `${label} ${candidate.sectionLabel.toLowerCase()} ${candidate.keywords ?? ''}`;
    if (!words.every((word) => haystack.includes(word))) continue;

    // Typing a setting's name exactly means that setting, even when the same
    // words appear inside a longer name — "Template" should not be beaten by
    // "Template folder".
    const exact = label === query.trim().toLowerCase() ? 100 : 0;
    const inLabel = words.filter((word) => label.includes(word)).length;
    // A shorter label containing the same words is the more specific match.
    const brevity = 1 / (1 + label.length);
    scored.push({ entry: candidate, score: exact + inLabel + brevity });
  }

  // A stable sort, so two equally good matches keep the order they are
  // declared in — which is the order they appear in the dialog.
  return scored.sort((a, b) => b.score - a.score).map((row) => row.entry);
}

/** The sections that hold at least one match, for narrowing the nav. */
export function sectionsMatching(query: string): Set<SettingsSection> {
  return new Set(matchSettings(query).map((found) => found.section));
}
