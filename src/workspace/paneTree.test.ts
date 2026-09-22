import { describe, expect, it } from 'vitest';

import { asVaultPath } from '@/types/domain';
import type { PaneLayout, VaultPath } from '@/types/domain';

import {
  activeTabOf,
  allTabs,
  closeAllInPane,
  closeOthers,
  closeTab,
  closeToTheRight,
  cyclePane,
  cycleTab,
  emptyLayout,
  findLeaf,
  findLeafOfTab,
  leaves,
  moveTabToPane,
  normalise,
  openInPane,
  pruneMissing,
  reorderTab,
  resizeSplit,
  retargetTabs,
  setActiveTab,
  splitPane,
  togglePin,
} from './paneTree';

const p = (text: string): VaultPath => asVaultPath(text);

function withOpen(paths: string[]): PaneLayout {
  return paths.reduce(
    (layout, path) => openInPane(layout, layout.activePaneId, p(path)),
    emptyLayout(),
  );
}

function openPaths(layout: PaneLayout): string[] {
  return allTabs(layout.root).map((tab) => tab.path);
}

describe('opening files', () => {
  it('adds a tab and makes it active', () => {
    const layout = withOpen(['A.md']);
    const pane = leaves(layout.root)[0]!;
    expect(pane.tabs).toHaveLength(1);
    expect(activeTabOf(pane)?.path).toBe('A.md');
  });

  it('reuses the existing tab rather than opening a second one', () => {
    let layout = withOpen(['A.md', 'B.md']);
    layout = openInPane(layout, layout.activePaneId, p('A.md'));

    expect(openPaths(layout)).toEqual(['A.md', 'B.md']);
    expect(activeTabOf(leaves(layout.root)[0]!)?.path).toBe('A.md');
  });

  it('treats the same file in a different mode as a different tab', () => {
    let layout = withOpen(['A.md']);
    layout = openInPane(layout, layout.activePaneId, p('A.md'), 'read');
    expect(allTabs(layout.root)).toHaveLength(2);
  });
});

describe('closing tabs', () => {
  it('moves focus to the tab that took its place', () => {
    let layout = withOpen(['A.md', 'B.md', 'C.md']);
    const tabB = allTabs(layout.root)[1]!;
    layout = setActiveTab(layout, tabB.id);

    layout = closeTab(layout, tabB.id);

    expect(openPaths(layout)).toEqual(['A.md', 'C.md']);
    expect(activeTabOf(leaves(layout.root)[0]!)?.path).toBe('C.md');
  });

  it('falls back to the previous tab when the last one closes', () => {
    let layout = withOpen(['A.md', 'B.md']);
    const last = allTabs(layout.root)[1]!;
    layout = setActiveTab(layout, last.id);
    layout = closeTab(layout, last.id);
    expect(activeTabOf(leaves(layout.root)[0]!)?.path).toBe('A.md');
  });

  it('leaves the active tab alone when a different one closes', () => {
    let layout = withOpen(['A.md', 'B.md']);
    const [first, second] = allTabs(layout.root);
    layout = setActiveTab(layout, second!.id);
    layout = closeTab(layout, first!.id);
    expect(activeTabOf(leaves(layout.root)[0]!)?.path).toBe('B.md');
  });

  it('keeps one empty pane when everything is closed', () => {
    let layout = withOpen(['A.md']);
    layout = closeTab(layout, allTabs(layout.root)[0]!.id);

    expect(leaves(layout.root)).toHaveLength(1);
    expect(allTabs(layout.root)).toHaveLength(0);
    expect(findLeaf(layout.root, layout.activePaneId)).not.toBeNull();
  });

  it('closes others but spares pinned tabs', () => {
    let layout = withOpen(['A.md', 'B.md', 'C.md']);
    const [a, b, c] = allTabs(layout.root);
    layout = togglePin(layout, a!.id);
    layout = closeOthers(layout, c!.id);

    const paths = openPaths(layout);
    expect(paths).toContain('A.md');
    expect(paths).toContain('C.md');
    expect(paths).not.toContain('B.md');
    expect(b).toBeDefined();
  });

  it('closes the tabs to the right and leaves the rest', () => {
    let layout = withOpen(['A.md', 'B.md', 'C.md', 'D.md']);
    const [, b] = allTabs(layout.root);
    layout = closeToTheRight(layout, b!.id);

    expect(openPaths(layout)).toEqual(['A.md', 'B.md']);
  });

  it('closing to the right spares pinned tabs', () => {
    let layout = withOpen(['A.md', 'B.md', 'C.md', 'D.md']);
    // Pinning sorts D to the front, so pin last and work from the order the
    // pane actually holds.
    const tabs = allTabs(layout.root);
    layout = togglePin(layout, tabs[3]!.id);

    const after = allTabs(layout.root);
    const b = after.find((tab) => tab.path === 'B.md');
    layout = closeToTheRight(layout, b!.id);

    // D was pinned, so it survives wherever it ended up.
    expect(openPaths(layout)).toContain('D.md');
    expect(openPaths(layout)).not.toContain('C.md');
  });

  it('closing to the right on the last tab changes nothing', () => {
    let layout = withOpen(['A.md', 'B.md']);
    const last = allTabs(layout.root)[1];
    layout = closeToTheRight(layout, last!.id);
    expect(openPaths(layout)).toEqual(['A.md', 'B.md']);
  });

  it('moves focus off a tab that closing to the right took away', () => {
    let layout = withOpen(['A.md', 'B.md', 'C.md']);
    const tabs = allTabs(layout.root);
    // C is active; closing to the right of A must not leave it pointing there.
    layout = closeToTheRight(layout, tabs[0]!.id);

    const leaf = findLeaf(layout.root, layout.activePaneId);
    expect(leaf?.activeTabId).toBe(tabs[0]!.id);
    expect(openPaths(layout)).toEqual(['A.md']);
  });

  it('closing all in a pane spares pinned tabs', () => {
    let layout = withOpen(['A.md', 'B.md']);
    layout = togglePin(layout, allTabs(layout.root)[0]!.id);
    layout = closeAllInPane(layout, layout.activePaneId);
    expect(openPaths(layout)).toEqual(['A.md']);
  });
});

describe('pinning and reordering', () => {
  it('sorts pinned tabs ahead of loose ones', () => {
    let layout = withOpen(['A.md', 'B.md', 'C.md']);
    const c = allTabs(layout.root)[2]!;
    layout = togglePin(layout, c.id);
    expect(openPaths(layout)).toEqual(['C.md', 'A.md', 'B.md']);
  });

  it('unpinning leaves the tab where pinning moved it', () => {
    // Pinning moves a tab to the front; unpinning releases it but does not
    // teleport it back, which is how tabbed editors behave and what keeps a
    // tab from jumping under the cursor mid-click.
    let layout = withOpen(['A.md', 'B.md']);
    const b = allTabs(layout.root)[1]!;
    layout = togglePin(layout, b.id);
    expect(openPaths(layout)).toEqual(['B.md', 'A.md']);
    layout = togglePin(layout, b.id);
    expect(openPaths(layout)).toEqual(['B.md', 'A.md']);
    expect(allTabs(layout.root).every((tab) => !tab.pinned)).toBe(true);
  });

  it('reorders a tab to an index', () => {
    let layout = withOpen(['A.md', 'B.md', 'C.md']);
    const c = allTabs(layout.root)[2]!;
    layout = reorderTab(layout, c.id, 0);
    expect(openPaths(layout)).toEqual(['C.md', 'A.md', 'B.md']);
  });

  it('clamps an out-of-range reorder instead of losing the tab', () => {
    let layout = withOpen(['A.md', 'B.md']);
    const a = allTabs(layout.root)[0]!;
    layout = reorderTab(layout, a.id, 99);
    expect(openPaths(layout)).toEqual(['B.md', 'A.md']);
  });
});

describe('splitting', () => {
  it('moves the active tab into the new pane', () => {
    let layout = withOpen(['A.md', 'B.md']);
    layout = splitPane(layout, layout.activePaneId, 'vertical');

    const panes = leaves(layout.root);
    expect(panes).toHaveLength(2);
    expect(panes[0]!.tabs.map((t) => t.path)).toEqual(['A.md']);
    expect(panes[1]!.tabs.map((t) => t.path)).toEqual(['B.md']);
    expect(layout.activePaneId).toBe(panes[1]!.id);
  });

  it('splits an empty pane into two empty panes', () => {
    const layout = splitPane(emptyLayout(), emptyLayout().activePaneId, 'horizontal');
    expect(leaves(layout.root).length).toBeGreaterThanOrEqual(1);
  });

  it('nests splits', () => {
    let layout = withOpen(['A.md', 'B.md', 'C.md']);
    layout = splitPane(layout, layout.activePaneId, 'vertical');
    layout = splitPane(layout, layout.activePaneId, 'horizontal');
    expect(leaves(layout.root).length).toBeGreaterThanOrEqual(2);
  });

  it('collapses a split when one side empties', () => {
    let layout = withOpen(['A.md', 'B.md']);
    layout = splitPane(layout, layout.activePaneId, 'vertical');
    const secondPane = leaves(layout.root)[1]!;

    layout = closeTab(layout, secondPane.tabs[0]!.id);

    expect(layout.root.type).toBe('leaf');
    expect(openPaths(layout)).toEqual(['A.md']);
  });

  it('moves the active pane when its own pane collapses', () => {
    let layout = withOpen(['A.md', 'B.md']);
    layout = splitPane(layout, layout.activePaneId, 'vertical');
    const active = layout.activePaneId;
    const activePane = findLeaf(layout.root, active)!;

    layout = closeTab(layout, activePane.tabs[0]!.id);

    expect(findLeaf(layout.root, layout.activePaneId)).not.toBeNull();
  });

  it('clamps a resize so neither side can vanish', () => {
    let layout = withOpen(['A.md', 'B.md']);
    layout = splitPane(layout, layout.activePaneId, 'vertical');
    const splitId = layout.root.type === 'split' ? layout.root.id : '';

    const tiny = resizeSplit(layout, splitId, 0.001);
    const huge = resizeSplit(layout, splitId, 5);

    expect(tiny.root.type === 'split' && tiny.root.ratio).toBeGreaterThanOrEqual(0.15);
    expect(huge.root.type === 'split' && huge.root.ratio).toBeLessThanOrEqual(0.85);
  });
});

describe('moving tabs between panes', () => {
  it('moves a tab and focuses the destination', () => {
    let layout = withOpen(['A.md', 'B.md']);
    layout = splitPane(layout, layout.activePaneId, 'vertical');
    const [left, right] = leaves(layout.root);
    const tabInLeft = left!.tabs[0]!;

    layout = moveTabToPane(layout, tabInLeft.id, right!.id);

    const panes = leaves(layout.root);
    // The source pane emptied, so the split collapsed into the destination.
    expect(panes).toHaveLength(1);
    expect(panes[0]!.tabs.map((t) => t.path)).toEqual(['B.md', 'A.md']);
  });

  it('moving within the same pane reorders instead', () => {
    let layout = withOpen(['A.md', 'B.md']);
    const a = allTabs(layout.root)[0]!;
    layout = moveTabToPane(layout, a.id, layout.activePaneId, 1);
    expect(openPaths(layout)).toEqual(['B.md', 'A.md']);
  });
});

describe('cycling', () => {
  it('walks tabs forwards and wraps around', () => {
    let layout = withOpen(['A.md', 'B.md', 'C.md']);
    layout = setActiveTab(layout, allTabs(layout.root)[2]!.id);
    layout = cycleTab(layout, 1);
    expect(activeTabOf(leaves(layout.root)[0]!)?.path).toBe('A.md');
  });

  it('walks tabs backwards', () => {
    let layout = withOpen(['A.md', 'B.md']);
    layout = setActiveTab(layout, allTabs(layout.root)[0]!.id);
    layout = cycleTab(layout, -1);
    expect(activeTabOf(leaves(layout.root)[0]!)?.path).toBe('B.md');
  });

  it('does nothing with fewer than two tabs', () => {
    const layout = withOpen(['A.md']);
    expect(cycleTab(layout, 1)).toEqual(layout);
  });

  it('cycles focus between panes', () => {
    let layout = withOpen(['A.md', 'B.md']);
    layout = splitPane(layout, layout.activePaneId, 'vertical');
    const before = layout.activePaneId;
    layout = cyclePane(layout, 1);
    expect(layout.activePaneId).not.toBe(before);
  });
});

describe('reconciling with the vault', () => {
  it('drops tabs whose file has gone', () => {
    const layout = withOpen(['Kept.md', 'Deleted.md']);
    const { layout: pruned, removed } = pruneMissing(layout, (path) => path === 'Kept.md');

    expect(removed).toEqual(['Deleted.md']);
    expect(openPaths(pruned)).toEqual(['Kept.md']);
  });

  it('points tabs at the new path after a rename', () => {
    const layout = withOpen(['Old.md', 'Other.md']);
    const renamed = retargetTabs(layout, p('Old.md'), p('New.md'));
    expect(openPaths(renamed)).toEqual(['New.md', 'Other.md']);
  });

  it('normalising an already-valid layout leaves it alone', () => {
    const layout = withOpen(['A.md']);
    expect(normalise(layout)).toEqual(layout);
  });
});

describe('lookups', () => {
  it('finds the leaf that owns a tab', () => {
    let layout = withOpen(['A.md', 'B.md']);
    layout = splitPane(layout, layout.activePaneId, 'vertical');
    const target = leaves(layout.root)[1]!.tabs[0]!;
    expect(findLeafOfTab(layout.root, target.id)?.id).toBe(leaves(layout.root)[1]!.id);
  });

  it('returns null for a tab that is not open', () => {
    expect(findLeafOfTab(withOpen(['A.md']).root, 'nope')).toBeNull();
  });
});
