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

  test('the tag panel lists the vault’s tags', async ({ page }) => {
    await openApp(page);

    await page.locator('.ie-sidebar--left .ie-sidebar__tab[title="Tags"]').click();
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

    await page.locator('.ie-sidebar--right .ie-sidebar__tab[title="Outline"]').click();
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
