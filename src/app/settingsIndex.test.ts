/**
 * The settings index, and whether it still describes the dialog.
 *
 * A hand-written index of a UI drifts the first time someone adds a field and
 * forgets this file — and the failure is silent: the setting simply cannot be
 * found. So the first test reads the dialog's own source and insists that
 * every field in it is listed here. Adding a setting without indexing it is
 * then a broken test rather than a setting nobody can reach.
 */

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { SETTINGS_INDEX, matchSettings, sectionsMatching } from './settingsIndex';

/**
 * Every label the dialog gives to a `Field`.
 *
 * Only `Field`s, deliberately: the dialog also labels its own search box and
 * its navigation, and neither is a setting. Matching the element rather than
 * keeping a list of exceptions means the next label that is not a setting
 * does not have to be remembered here.
 */
function labelsInDialog(): string[] {
  // Resolved from the project root: under vitest `import.meta.url` is not a
  // file URL, and the suite always runs from the root.
  const source = readFileSync(join(process.cwd(), 'src/app/SettingsDialog.tsx'), 'utf8');
  const fields = [...source.matchAll(/<Field\b([^>]*)>/g)].map((match) => match[1]!);
  const labels = fields
    .map((attributes) => /label="([^"]+)"/.exec(attributes)?.[1])
    .filter((label): label is string => label !== undefined);
  return [...new Set(labels)];
}

describe('the index covers the dialog', () => {
  it('lists every field the dialog renders', () => {
    const indexed = new Set(SETTINGS_INDEX.map((entry) => entry.label));
    const missing = labelsInDialog().filter((label) => !indexed.has(label));
    expect(missing).toEqual([]);
  });

  it('does not list fields the dialog no longer has', () => {
    const rendered = new Set(labelsInDialog());
    const stale = SETTINGS_INDEX.map((entry) => entry.label).filter(
      (label) => !rendered.has(label),
    );
    expect(stale).toEqual([]);
  });
});

describe('matchSettings', () => {
  it('finds a setting by a word in its label', () => {
    expect(matchSettings('spelling').map((entry) => entry.label)).toEqual(['Check spelling']);
  });

  /**
   * The acceptance test from the plan. "Dark" is a *value* of the theme
   * setting, not its name, and a search that only looked at labels would
   * find nothing at all.
   */
  it('finds the theme by a word that is only one of its values', () => {
    const found = matchSettings('dark');
    expect(found.map((entry) => entry.label)).toContain('Theme');
    expect(found[0]?.section).toBe('appearance');
  });

  it('narrows rather than widens as more words are typed', () => {
    const one = matchSettings('folder');
    const two = matchSettings('folder daily');
    expect(two.length).toBeLessThan(one.length);
    expect(two.every((entry) => entry.section === 'dailyNotes')).toBe(true);
  });

  it('puts an exact label match first', () => {
    // "Template" and "Template folder" both contain the word; typing the
    // shorter name exactly means the shorter setting.
    const found = matchSettings('template');
    expect(found[0]?.label).toBe('Template');
    expect(found.map((entry) => entry.label)).toContain('Template folder');
  });

  it('matches a section by name', () => {
    expect(matchSettings('appearance').every((entry) => entry.section === 'appearance')).toBe(true);
  });

  it('ignores case and surrounding space', () => {
    expect(matchSettings('  THEME ').map((entry) => entry.label)).toContain('Theme');
  });

  it('finds nothing for an empty query rather than everything', () => {
    // An empty box means "I have not asked yet", not "show me all of them".
    expect(matchSettings('')).toEqual([]);
    expect(matchSettings('   ')).toEqual([]);
  });

  it('finds nothing for a word that is in no setting', () => {
    expect(matchSettings('parsnip')).toEqual([]);
  });
});

describe('sectionsMatching', () => {
  it('reports the sections holding a match', () => {
    expect([...sectionsMatching('dark')]).toEqual(['appearance']);
  });

  it('is empty when nothing matches', () => {
    expect(sectionsMatching('parsnip').size).toBe(0);
  });
});
