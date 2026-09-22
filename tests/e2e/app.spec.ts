/**
 * The flows a user actually performs.
 *
 * Each test walks a path through the real components, stores and editor, with
 * the backend replaced by a double faithful enough that links resolve,
 * backlinks appear and saving persists. What is being checked is the wiring —
 * the part that breaks when a component is changed in isolation.
 */

import { expect, test, type Page } from '@playwright/test';

import { installMockBackend, type MockVaultFile } from './mockBackend';

const STARTER_FILES: MockVaultFile[] = [
  {
    path: 'Notes/Welcome.md',
    content: '# Welcome\n\nThis vault is a folder of Markdown files. #getting-started\n',
  },
  {
    path: 'Notes/Machine Learning.md',
    content: '# Machine Learning\n\nNotes on gradient descent and neural networks. #AI/Research\n',
  },
  {
    path: 'Notes/Statistics.md',
    content: '# Statistics\n\nSee also [[Machine Learning]] for the applied side. #AI\n',
  },
  { path: 'Projects/Roadmap.md', content: '# Roadmap\n\nShip by Friday. #planning\n' },
];

async function openApp(page: Page, files: MockVaultFile[] = STARTER_FILES): Promise<void> {
  await page.addInitScript(installMockBackend({ files, openVault: true }));
  await page.goto('/');
  // The explorer appearing is the signal that the vault opened and the first
  // listing arrived.
  await expect(page.locator('.ie-explorer')).toBeVisible();
}

/** Open a note from the file tree and wait for the editor to hold it. */
async function openNote(page: Page, label: string): Promise<void> {
  await page.locator('.ie-tree-row--file', { hasText: label }).first().click();
  await expect(page.locator('.ie-editor .cm-content')).toBeVisible();
}

/**
 * Show a sidebar panel by clicking its icon in the rail.
 *
 * Asked for by role and name rather than by class, so moving the rail or
 * restyling it does not break the test — and so that a panel unreachable by
 * its accessible name fails here rather than silently.
 */
async function openPanel(page: Page, side: 'left' | 'right', label: string): Promise<void> {
  await page.locator(`.ie-rail--${side}`).getByRole('tab', { name: label }).click();
  await expect(page.locator(`.ie-sidebar--${side}`)).toBeVisible();
}

test.describe('opening a vault', () => {
  test('shows the file tree with the vault’s notes', async ({ page }) => {
    await openApp(page);

    await expect(page.locator('.ie-tree-row--folder', { hasText: 'Notes' })).toBeVisible();
    await expect(page.locator('.ie-tree-row--folder', { hasText: 'Projects' })).toBeVisible();
  });

  test('offers to open or create a vault when none is open', async ({ page }) => {
    // No recent vault either: with one, the app reopens it at launch, which is
    // the behaviour the next test covers.
    await page.addInitScript(
      installMockBackend({ files: [], openVault: false, recentVaults: false }),
    );
    await page.goto('/');

    await expect(page.getByRole('heading', { name: 'Inner Empire' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Open a folder' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Create a vault' })).toBeVisible();
  });
});

test.describe('editing', () => {
  test('opens a note and shows its text', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await openNote(page, 'Welcome');

    await expect(page.locator('.cm-content')).toContainText('folder of Markdown files');
  });

  test('typing reaches the file', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Projects' }).click();
    await openNote(page, 'Roadmap');

    const editor = page.locator('.cm-content');
    await editor.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('\nA line added by the test.');

    // Autosave is debounced, so the assertion waits for the write rather than
    // for a fixed delay.
    await expect
      .poll(async () =>
        page.evaluate(() => window.__mockVault?.read('Projects/Roadmap.md') ?? ''),
      )
      .toContain('A line added by the test.');
  });

  test('a tab shows an unsaved marker until the write lands', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Projects' }).click();
    await openNote(page, 'Roadmap');

    await page.locator('.cm-content').click();
    await page.keyboard.type('x');

    await expect(page.locator('.ie-tab.is-dirty')).toBeVisible();
    await expect(page.locator('.ie-tab.is-dirty')).toHaveCount(0, { timeout: 10_000 });
  });
});

test.describe('links', () => {
  test('a wiki link opens the note it points at', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await openNote(page, 'Statistics');

    // Live preview renders the link; clicking it navigates.
    await page.locator('.cm-ie-link', { hasText: 'Machine Learning' }).click();

    await expect(page.locator('.ie-tab.is-active')).toContainText('Machine Learning');
  });

  test('backlinks show which notes point here', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await openNote(page, 'Machine Learning');

    const backlinks = page.locator('.ie-backlinks');
    await expect(backlinks).toContainText('Statistics');
  });

  test('a link to a note that does not exist creates it', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Projects' }).click();
    await openNote(page, 'Roadmap');

    const editor = page.locator('.cm-content');
    await editor.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('\nPlanning [[Future Project]] next.');

    await expect
      .poll(async () =>
        page.evaluate(() => window.__mockVault?.read('Projects/Roadmap.md') ?? ''),
      )
      .toContain('[[Future Project]]');

    // Move the cursor off the line so live preview renders the link, then
    // follow it.
    await page.keyboard.press('Control+Home');
    await page.locator('.cm-ie-link-unresolved', { hasText: 'Future Project' }).click();

    await expect
      .poll(async () => page.evaluate(() => window.__mockVault?.has('Projects/Future Project.md')))
      .toBe(true);
  });

  test('renaming a note rewrites the links pointing at it', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();

    await page
      .locator('.ie-tree-row--file', { hasText: 'Machine Learning' })
      .first()
      .click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Rename' }).click();

    const field = page.locator('.ie-modal input');
    await field.fill('Deep Learning.md');
    await page.getByRole('button', { name: 'Rename' }).click();

    await expect
      .poll(async () =>
        page.evaluate(() => window.__mockVault?.read('Notes/Statistics.md') ?? ''),
      )
      .toContain('[[Deep Learning]]');
  });
});

test.describe('finding things', () => {
  test('the quick switcher opens a note by name', async ({ page }) => {
    await openApp(page);

    await page.keyboard.press('Control+o');
    await expect(page.locator('.ie-palette')).toBeVisible();

    await page.locator('.ie-palette__input').fill('roadmap');
    await page.keyboard.press('Enter');

    await expect(page.locator('.ie-tab.is-active')).toContainText('Roadmap');
  });

  test('the command palette lists and runs commands', async ({ page }) => {
    await openApp(page);

    await page.keyboard.press('Control+p');
    await expect(page.locator('.ie-palette')).toBeVisible();

    await page.locator('.ie-palette__input').fill('sidebar');
    await expect(page.locator('.ie-palette__item').first()).toContainText('sidebar');

    await page.keyboard.press('Enter');
    await expect(page.locator('.ie-palette')).toHaveCount(0);
  });

  test('search finds notes by their text', async ({ page }) => {
    await openApp(page);

    await page.keyboard.press('Control+Shift+f');
    await page.locator('.ie-search input[type="search"]').fill('gradient');

    await expect(page.locator('.ie-search-result')).toHaveCount(1);
    await expect(page.locator('.ie-search-result')).toContainText('Machine Learning');
  });

  test('search filters by tag, including nested tags', async ({ page }) => {
    await openApp(page);

    await page.keyboard.press('Control+Shift+f');
    await page.locator('.ie-search input[type="search"]').fill('tag:AI');

    // #AI and #AI/Research both count.
    await expect(page.locator('.ie-search-result')).toHaveCount(2);
  });

  test('a search shows its clauses as chips, and removing one rewrites it', async ({ page }) => {
    await openApp(page);
    await openPanel(page, 'left', 'Search');

    const box = page.getByRole('searchbox', { name: 'Search notes' });
    await box.fill('neural tag:AI');

    const chips = page.locator('.ie-search__chips .ie-chip');
    await expect(chips).toHaveCount(2);
    await expect(chips.nth(1)).toContainText('tagged #AI');

    // Removing the tag chip takes that clause out of the query itself, so the
    // box and the chips cannot disagree about what is being searched for.
    await chips.nth(1).getByRole('button', { name: /^Remove/ }).click();
    await expect(box).toHaveValue('neural');
    await expect(chips).toHaveCount(1);
  });

  test('the tag panel lists the vault’s tags', async ({ page }) => {
    await openApp(page);

    await openPanel(page, 'left', 'Tags');
    await expect(page.locator('.ie-tag-row', { hasText: 'AI' }).first()).toBeVisible();
    await expect(page.locator('.ie-tag-row', { hasText: 'planning' })).toBeVisible();
  });
});

test.describe('the workspace', () => {
  test('splitting a pane shows two editors side by side', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await openNote(page, 'Welcome');

    await page.keyboard.press('Control+\\');

    await expect(page.locator('.ie-pane')).toHaveCount(2);
    await expect(page.locator('.ie-split__divider')).toBeVisible();
  });

  test('a tab closes with the middle button', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await openNote(page, 'Welcome');
    await openNote(page, 'Statistics');

    await expect(page.locator('.ie-tab')).toHaveCount(2);
    await page.locator('.ie-tab', { hasText: 'Welcome' }).click({ button: 'middle' });
    await expect(page.locator('.ie-tab')).toHaveCount(1);
  });

  test('a closed tab can be brought back', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await openNote(page, 'Welcome');
    await openNote(page, 'Statistics');
    await expect(page.locator('.ie-tab')).toHaveCount(2);

    await page.getByRole('button', { name: /^Close Statistics/ }).click();
    await expect(page.locator('.ie-tab')).toHaveCount(1);

    await page.keyboard.press('Control+Shift+W');
    await expect(page.locator('.ie-tab')).toHaveCount(2);
    await expect(page.locator('.ie-tab').last()).toContainText('Statistics');
  });

  test('the layout comes back after a restart', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await openNote(page, 'Welcome');
    await openNote(page, 'Statistics');

    // Wait for the layout to be persisted before reloading.
    await expect
      .poll(async () => page.evaluate(() => window.__mockVault?.savedWorkspace() !== null), {
        timeout: 10_000,
      })
      .toBe(true);

    await page.reload();

    await expect(page.locator('.ie-tab')).toHaveCount(2);
    await expect(page.locator('.ie-tab', { hasText: 'Statistics' })).toBeVisible();
  });

  test('the reading view renders a callout', async ({ page }) => {
    await openApp(page, [
      {
        path: 'Notes/Guide.md',
        content:
          '# Guide\n\n> [!warning] Mind the gap\n> Stand clear of the doors.\n\n> An ordinary quotation.\n',
      },
    ]);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await openNote(page, 'Guide');
    await page.keyboard.press('Control+Shift+R');

    const callout = page.locator('.ie-callout');
    await expect(callout).toHaveCount(1);
    await expect(callout).toHaveClass(/ie-callout--warning/);
    await expect(callout).toContainText('Mind the gap');
    await expect(callout).toContainText('Stand clear of the doors.');
    // The marker itself is not part of what the reader sees.
    await expect(callout).not.toContainText('[!warning]');

    // A blockquote without a marker stays a blockquote.
    await expect(page.locator('.ie-reading__body blockquote')).toHaveCount(1);
  });

  test('the outline lists the note’s headings', async ({ page }) => {
    await openApp(page, [
      ...STARTER_FILES,
      {
        path: 'Notes/Structured.md',
        content: '# Structured\n\ntext\n\n## Middle\n\ntext\n\n### Deep\n\ntext\n',
      },
    ]);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await openNote(page, 'Structured');

    await openPanel(page, 'right', 'Outline');
    await expect(page.locator('.ie-outline__item')).toHaveCount(3);
    await expect(page.locator('.ie-outline__item').first()).toContainText('Structured');
  });
});

test.describe('the graph', () => {
  test('opens and reports what it drew', async ({ page }) => {
    await openApp(page);

    await page.keyboard.press('Control+g');

    await expect(page.locator('.ie-graph__canvas')).toBeVisible();
    // Four notes, one link between Statistics and Machine Learning.
    await expect(page.locator('.ie-graph__status')).toContainText('4 notes');
    await expect(page.locator('.ie-graph__status')).toContainText('1 links');
  });

  test('the filters narrow what the graph draws', async ({ page }) => {
    await openApp(page);
    await page.keyboard.press('Control+g');
    await expect(page.locator('.ie-graph__status')).toContainText('4 notes');

    // Scoped to the graph's own panel: the explorer has a "New folder" button
    // that an unscoped label would also match.
    const controls = page.locator('.ie-graph__controls');

    // Three of the four notes live in Notes/; Roadmap.md is in Projects/.
    await controls.getByLabel('Folder').selectOption('Notes');
    await expect(page.locator('.ie-graph__status')).toContainText('3 notes');

    // Of those three, only Statistics and Machine Learning are linked.
    await controls.getByRole('switch', { name: 'Only notes with links' }).click();
    await expect(page.locator('.ie-graph__status')).toContainText('2 notes');
    await expect(page.locator('.ie-graph__status')).toContainText('1 links');

    await controls.getByLabel('Folder').selectOption('');
    await expect(page.locator('.ie-graph__status')).toContainText('2 notes');
  });

  test('the zoom controls change the zoom readout', async ({ page }) => {
    await openApp(page);
    await page.keyboard.press('Control+g');

    const level = page.locator('.ie-graph__zoom-level');
    await expect(level).toHaveText('100%');

    await page.getByRole('button', { name: 'Zoom in' }).click();
    await expect(level).toHaveText('125%');

    await page.getByRole('button', { name: 'Zoom out' }).click();
    await expect(level).toHaveText('100%');
  });
});

test.describe('the interface', () => {
  test('sidebars toggle', async ({ page }) => {
    await openApp(page);

    await expect(page.locator('.ie-sidebar--left')).toBeVisible();
    await page.keyboard.press('Control+b');
    await expect(page.locator('.ie-sidebar--left')).toHaveCount(0);
    await page.keyboard.press('Control+b');
    await expect(page.locator('.ie-sidebar--left')).toBeVisible();
  });

  test('the rail reopens a sidebar it closed', async ({ page }) => {
    await openApp(page);

    // The rail is the one part that never collapses, so it has to be able to
    // bring back a sidebar it just put away.
    const tags = page.locator('.ie-rail--left').getByRole('tab', { name: 'Tags' });
    await tags.click();
    await expect(page.locator('.ie-tag-row').first()).toBeVisible();

    // Choosing the panel already showing collapses the sidebar.
    await tags.click();
    await expect(page.locator('.ie-sidebar--left')).toHaveCount(0);
    await expect(page.locator('.ie-rail--left')).toBeVisible();

    await tags.click();
    await expect(page.locator('.ie-sidebar--left')).toBeVisible();
  });

  test('back and forward walk the notes that were opened', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();

    await openNote(page, 'Welcome');
    await openNote(page, 'Statistics');
    await expect(page.locator('.ie-toolbar')).toContainText('Statistics');

    await page.getByRole('button', { name: 'Back', exact: true }).click();
    await expect(page.locator('.ie-toolbar')).toContainText('Welcome');

    await page.getByRole('button', { name: 'Forward', exact: true }).click();
    await expect(page.locator('.ie-toolbar')).toContainText('Statistics');
  });

  test('the breadcrumb shows the folder a note lives in', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await openNote(page, 'Statistics');

    const trail = page.locator('.ie-breadcrumb');
    await expect(trail).toContainText('Notes');

    // Collapse the explorer, then use the trail to bring it back showing that
    // folder — which is the only reason the segments are clickable.
    await page.keyboard.press('Control+b');
    await expect(page.locator('.ie-sidebar--left')).toHaveCount(0);
    await trail.getByRole('button', { name: 'Notes' }).click();
    await expect(page.locator('.ie-tree-row--folder', { hasText: 'Notes' })).toBeVisible();
  });

  test('the file tree can be driven from the keyboard', async ({ page }) => {
    await openApp(page);

    // The tree is a virtualized list, so focus stays on the scroller and the
    // current row is named rather than focused.
    const tree = page.getByRole('tree', { name: 'Files' });
    await tree.click({ position: { x: 4, y: 4 } });
    await page.keyboard.press('ArrowDown');
    await expect(tree).toHaveAttribute('aria-activedescendant', /.+/);

    // Right opens a closed folder, and its contents appear below it.
    await page.keyboard.press('ArrowRight');
    await expect(page.locator('.ie-tree-row--file').first()).toBeVisible();

    // Down then Enter opens the first note inside it.
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await expect(page.locator('.ie-editor .cm-content')).toBeVisible();
  });

  test('settings open and show the sections', async ({ page }) => {
    await openApp(page);

    await page.keyboard.press('Control+,');
    await expect(page.getByRole('dialog', { name: 'Settings' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Editor' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Keyboard shortcuts' })).toBeVisible();

    await page.getByRole('button', { name: 'Appearance' }).click();
    // Scoped to the dialog: the explorer behind it also has a dropdown.
    await expect(page.getByRole('dialog', { name: 'Settings' }).getByLabel('Theme')).toHaveValue(
      'system',
    );
  });

  test('searching the settings finds a setting by one of its values', async ({ page }) => {
    await openApp(page);
    await page.keyboard.press('Control+,');
    const dialog = page.getByRole('dialog', { name: 'Settings' });

    // "Dark" is a value of the Theme setting, not its name — a search that
    // only looked at labels would find nothing.
    await dialog.getByRole('searchbox', { name: 'Search the settings' }).fill('dark');

    const result = dialog.getByRole('button', { name: /Theme/ });
    await expect(result).toBeVisible();
    await result.click();

    // Choosing a result opens its section and points at the field.
    await expect(dialog.locator('.ie-field.is-highlighted')).toContainText('Theme');
    await expect(dialog.getByLabel('Theme')).toBeVisible();
  });

  test('the theme switches between light and dark', async ({ page }) => {
    await openApp(page);

    const themeBefore = await page.evaluate(() => document.documentElement.dataset.theme);
    await page.keyboard.press('Control+p');
    await page.locator('.ie-palette__input').fill('light and dark');
    await page.keyboard.press('Enter');

    await expect
      .poll(async () => page.evaluate(() => document.documentElement.dataset.theme))
      .not.toBe(themeBefore);
  });

  test('the status bar reports the note and its counts', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await openNote(page, 'Welcome');

    const status = page.locator('.ie-statusbar');
    await expect(status).toContainText('Test Vault');
    await expect(status).toContainText('words');
    await expect(status).toContainText('Line');
  });

  test('a right-click offers the file actions', async ({ page }) => {
    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();

    await page.locator('.ie-tree-row--file', { hasText: 'Welcome' }).first().click({ button: 'right' });

    await expect(page.getByRole('menuitem', { name: 'Open in a split' })).toBeVisible();
    await expect(page.getByRole('menuitem', { name: 'Rename' })).toBeVisible();
    await expect(page.getByRole('menuitem', { name: 'Move to trash' })).toBeVisible();
    await expect(page.getByRole('menuitem', { name: 'Copy link' })).toBeVisible();
  });
});

declare global {
  interface Window {
    __mockVault?: {
      read: (path: string) => string | undefined;
      has: (path: string) => boolean;
      paths: () => string[];
      savedWorkspace: () => unknown;
    };
  }
}

/**
 * Nothing should reach the console.
 *
 * A React key warning, a failed fetch, an unhandled rejection: none of them
 * stop the app, all of them mean something is wrong, and every one is
 * invisible unless someone happens to have the console open. Installed on
 * every page this suite opens, and asserted at the end of the walk-through
 * below.
 *
 * Errors are collected rather than thrown at once, so a test reports what
 * broke rather than dying at the first message.
 */
function watchConsole(page: Page): string[] {
  const noise: string[] = [];

  page.on('console', (message) => {
    if (message.type() !== 'error' && message.type() !== 'warning') return;
    const text = message.text();
    // Vite's preview server and Chromium itself say things that are not the
    // application's doing.
    if (/favicon|DevTools|Download the React DevTools/i.test(text)) return;
    noise.push(`console.${message.type()}: ${text}`);
  });

  page.on('pageerror', (error) => noise.push(`uncaught: ${error.message}`));
  page.on('requestfailed', (request) => {
    const failure = request.failure()?.errorText ?? 'failed';
    // A cancelled request is normal when a view unmounts mid-flight.
    if (/ABORTED/i.test(failure)) return;
    noise.push(`request failed: ${request.url()} — ${failure}`);
  });

  // Chromium's own console message for a bad status does not name the URL,
  // which makes it useless to whoever has to fix it. This does.
  page.on('response', (response) => {
    if (response.status() < 400) return;
    noise.push(`${response.status()} for ${response.url()}`);
  });

  return noise;
}

test.describe('the console', () => {
  test('stays quiet through a full walk of the application', async ({ page }) => {
    const noise = watchConsole(page);

    await openApp(page);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await openNote(page, 'Statistics');

    // Type, so the editor, the autosave and the index all run.
    await page.locator('.ie-editor .cm-content').click();
    await page.keyboard.type(' and a note about variance.');

    // Every panel on both sides. The order starts away from whichever panel
    // is already showing, because choosing the active one collapses the
    // sidebar — which is the rail's job, not a fault.
    for (const panel of ['Search', 'Tags', 'Files']) {
      await openPanel(page, 'left', panel);
    }
    for (const panel of ['Outline', 'Properties', 'Local graph', 'Backlinks']) {
      await openPanel(page, 'right', panel);
    }

    // Reading view, the graph, a split, the palette and the settings.
    await page.keyboard.press('Control+Shift+R');
    await expect(page.locator('.ie-reading__body')).toBeVisible();
    await page.keyboard.press('Control+Shift+R');

    await page.keyboard.press('Control+g');
    await expect(page.locator('.ie-graph__canvas')).toBeVisible();

    await page.keyboard.press('Control+p');
    await expect(page.getByRole('dialog')).toBeVisible();
    await page.keyboard.press('Escape');

    await page.keyboard.press('Control+,');
    await expect(page.getByRole('dialog', { name: 'Settings' })).toBeVisible();
    await page.keyboard.press('Escape');

    // Joined rather than compared as an array: a failed array comparison
    // shows only the first entry, and the whole list is what tells you what
    // went wrong.
    expect(noise.join('\n')).toBe('');
  });
});

test.describe('scale', () => {
  /**
   * A vault big enough that rendering every row would show.
   *
   * 4,000 notes in one folder is the shape that hurts: a flat list defeats
   * the "load a level at a time" strategy entirely, so the windowing is the
   * only thing standing between the user and four thousand DOM nodes.
   */
  const MANY = Array.from({ length: 4000 }, (_, index) => ({
    path: `Notes/Note ${String(index).padStart(4, '0')}.md`,
    content: `# Note ${index}\n\nBody text for note ${index}.\n`,
  }));

  test('the explorer renders a constant number of rows however large the vault', async ({
    page,
  }) => {
    await openApp(page, MANY);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();

    // Windowed: what is in the DOM is what fits on screen plus the overscan,
    // not what is in the vault. The exact number depends on the viewport, so
    // what is asserted is the order of magnitude.
    await expect(page.locator('.ie-tree-row--file').first()).toBeVisible();
    const rendered = await page.locator('.ie-tree-row').count();
    expect(rendered).toBeLessThan(120);

    // The scrollbar still reflects the whole list, so the user can reach the
    // end — a windowed list that forgets its own height cannot be scrolled.
    const spacer = await page
      .locator('.ie-explorer__list > div')
      .first()
      .evaluate((element) => element.getBoundingClientRect().height);
    expect(spacer).toBeGreaterThan(4000 * 20);
  });

  test('scrolling four thousand rows stays responsive', async ({ page }) => {
    await openApp(page, MANY);
    await page.locator('.ie-tree-row--folder', { hasText: 'Notes' }).click();
    await expect(page.locator('.ie-tree-row--file').first()).toBeVisible();

    const list = page.locator('.ie-explorer__list');
    const started = Date.now();
    for (let step = 1; step <= 20; step += 1) {
      await list.evaluate((element, offset) => {
        element.scrollTop = offset;
      }, step * 2000);
    }
    const elapsed = Date.now() - started;

    // Generous on purpose: this is a shared CI runner, and the point is to
    // catch a list that has started rendering everything, not to police
    // milliseconds.
    expect(elapsed).toBeLessThan(6000);

    // Still windowed at the far end of the list.
    expect(await page.locator('.ie-tree-row').count()).toBeLessThan(120);
  });

  test('the quick switcher stays usable in a large vault', async ({ page }) => {
    await openApp(page, MANY);

    await page.keyboard.press('Control+o');
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();

    await page.keyboard.type('Note 3999');
    await expect(dialog.locator('.ie-palette__item').first()).toContainText('Note 3999');
  });
});
