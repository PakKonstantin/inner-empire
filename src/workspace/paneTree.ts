/**
 * Operations on the pane tree.
 *
 * All pure: they take a tree and return a new one. Keeping the tab and split
 * logic out of the store means it can be tested exhaustively without React,
 * and the store becomes a thin holder of the result.
 *
 * The tree is binary — a leaf holds tabs, a split holds two children — because
 * that makes arbitrary nesting representable with no special cases and makes a
 * drag on a divider affect exactly two panes.
 */

import type { PaneLayout, PaneNode, SplitDirection, TabMode, TabState, VaultPath } from '@/types/domain';

let counter = 0;

/** Ids only need to be unique within one session; the workspace file keeps them. */
export function nextId(prefix: string): string {
  counter += 1;
  return `${prefix}-${Date.now().toString(36)}-${counter.toString(36)}`;
}

export function emptyLayout(): PaneLayout {
  const id = nextId('pane');
  return {
    root: { type: 'leaf', id, tabs: [], activeTabId: null },
    activePaneId: id,
  };
}

export function leaves(node: PaneNode): Extract<PaneNode, { type: 'leaf' }>[] {
  if (node.type === 'leaf') return [node];
  return [...leaves(node.first), ...leaves(node.second)];
}

export function findLeaf(
  node: PaneNode,
  paneId: string,
): Extract<PaneNode, { type: 'leaf' }> | null {
  return leaves(node).find((leaf) => leaf.id === paneId) ?? null;
}

/** The leaf that holds a given tab, if any. */
export function findLeafOfTab(
  node: PaneNode,
  tabId: string,
): Extract<PaneNode, { type: 'leaf' }> | null {
  return leaves(node).find((leaf) => leaf.tabs.some((tab) => tab.id === tabId)) ?? null;
}

export function allTabs(node: PaneNode): TabState[] {
  return leaves(node).flatMap((leaf) => leaf.tabs);
}

export function activeTabOf(leaf: Extract<PaneNode, { type: 'leaf' }>): TabState | null {
  return leaf.tabs.find((tab) => tab.id === leaf.activeTabId) ?? leaf.tabs[0] ?? null;
}

/** Replace one leaf, rebuilding the spine above it. */
export function mapLeaf(
  node: PaneNode,
  paneId: string,
  transform: (leaf: Extract<PaneNode, { type: 'leaf' }>) => PaneNode,
): PaneNode {
  if (node.type === 'leaf') {
    return node.id === paneId ? transform(node) : node;
  }
  return {
    ...node,
    first: mapLeaf(node.first, paneId, transform),
    second: mapLeaf(node.second, paneId, transform),
  };
}

export function createTab(path: VaultPath, mode: TabMode = 'edit'): TabState {
  return {
    id: nextId('tab'),
    path,
    pinned: false,
    mode,
    scrollLine: 0,
    cursorOffset: 0,
  };
}

/**
 * Open a file in a pane.
 *
 * Reuses an existing tab for the same file rather than opening a second one,
 * which is what people expect from a tabbed editor and what stops a vault
 * exploring session from accumulating forty tabs of the same note.
 */
export function openInPane(
  layout: PaneLayout,
  paneId: string,
  path: VaultPath,
  mode: TabMode = 'edit',
): PaneLayout {
  const leaf = findLeaf(layout.root, paneId) ?? leaves(layout.root)[0];
  if (!leaf) return layout;

  const existing = leaf.tabs.find((tab) => tab.path === path && tab.mode === mode);
  if (existing) {
    return {
      ...layout,
      activePaneId: leaf.id,
      root: mapLeaf(layout.root, leaf.id, (target) => ({
        ...target,
        activeTabId: existing.id,
      })),
    };
  }

  const tab = createTab(path, mode);
  return {
    ...layout,
    activePaneId: leaf.id,
    root: mapLeaf(layout.root, leaf.id, (target) => ({
      ...target,
      tabs: [...target.tabs, tab],
      activeTabId: tab.id,
    })),
  };
}

/** Close a tab, collapsing the pane if it was the last one and not the root. */
export function closeTab(layout: PaneLayout, tabId: string): PaneLayout {
  const owner = findLeafOfTab(layout.root, tabId);
  if (!owner) return layout;

  const remaining = owner.tabs.filter((tab) => tab.id !== tabId);
  const closedIndex = owner.tabs.findIndex((tab) => tab.id === tabId);

  // Focus moves to the tab that took its place, or to the one before it.
  const nextActive =
    owner.activeTabId === tabId
      ? (remaining[closedIndex] ?? remaining[closedIndex - 1] ?? remaining[0])?.id ?? null
      : owner.activeTabId;

  const updated = mapLeaf(layout.root, owner.id, (leaf) => ({
    ...leaf,
    tabs: remaining,
    activeTabId: nextActive,
  }));

  return normalise({ ...layout, root: updated });
}

/** Close every tab in a pane except one. Pinned tabs survive. */
export function closeOthers(layout: PaneLayout, tabId: string): PaneLayout {
  const owner = findLeafOfTab(layout.root, tabId);
  if (!owner) return layout;
  return normalise({
    ...layout,
    root: mapLeaf(layout.root, owner.id, (leaf) => ({
      ...leaf,
      tabs: leaf.tabs.filter((tab) => tab.id === tabId || tab.pinned),
      activeTabId: tabId,
    })),
  });
}

export function closeAllInPane(layout: PaneLayout, paneId: string): PaneLayout {
  return normalise({
    ...layout,
    root: mapLeaf(layout.root, paneId, (leaf) => ({
      ...leaf,
      tabs: leaf.tabs.filter((tab) => tab.pinned),
      activeTabId: leaf.tabs.find((tab) => tab.pinned)?.id ?? null,
    })),
  });
}

export function setActiveTab(layout: PaneLayout, tabId: string): PaneLayout {
  const owner = findLeafOfTab(layout.root, tabId);
  if (!owner) return layout;
  return {
    ...layout,
    activePaneId: owner.id,
    root: mapLeaf(layout.root, owner.id, (leaf) => ({ ...leaf, activeTabId: tabId })),
  };
}

export function togglePin(layout: PaneLayout, tabId: string): PaneLayout {
  const owner = findLeafOfTab(layout.root, tabId);
  if (!owner) return layout;
  return {
    ...layout,
    root: mapLeaf(layout.root, owner.id, (leaf) => {
      const tabs = leaf.tabs.map((tab) =>
        tab.id === tabId ? { ...tab, pinned: !tab.pinned } : tab,
      );
      // Pinned tabs sort ahead of unpinned ones, keeping their relative order.
      const pinned = tabs.filter((tab) => tab.pinned);
      const loose = tabs.filter((tab) => !tab.pinned);
      return { ...leaf, tabs: [...pinned, ...loose] };
    }),
  };
}

/** Move a tab within its pane, by index. */
export function reorderTab(layout: PaneLayout, tabId: string, toIndex: number): PaneLayout {
  const owner = findLeafOfTab(layout.root, tabId);
  if (!owner) return layout;
  return {
    ...layout,
    root: mapLeaf(layout.root, owner.id, (leaf) => {
      const from = leaf.tabs.findIndex((tab) => tab.id === tabId);
      if (from === -1) return leaf;
      const tabs = [...leaf.tabs];
      const [moved] = tabs.splice(from, 1);
      if (!moved) return leaf;
      tabs.splice(Math.max(0, Math.min(toIndex, tabs.length)), 0, moved);
      return { ...leaf, tabs };
    }),
  };
}

/** Move a tab into a different pane. */
export function moveTabToPane(
  layout: PaneLayout,
  tabId: string,
  targetPaneId: string,
  toIndex?: number,
): PaneLayout {
  const owner = findLeafOfTab(layout.root, tabId);
  if (!owner || owner.id === targetPaneId) {
    return toIndex === undefined ? layout : reorderTab(layout, tabId, toIndex);
  }
  const tab = owner.tabs.find((candidate) => candidate.id === tabId);
  if (!tab) return layout;

  let root = mapLeaf(layout.root, owner.id, (leaf) => {
    const tabs = leaf.tabs.filter((candidate) => candidate.id !== tabId);
    return {
      ...leaf,
      tabs,
      activeTabId: leaf.activeTabId === tabId ? (tabs[0]?.id ?? null) : leaf.activeTabId,
    };
  });

  root = mapLeaf(root, targetPaneId, (leaf) => {
    const tabs = [...leaf.tabs];
    tabs.splice(toIndex ?? tabs.length, 0, tab);
    return { ...leaf, tabs, activeTabId: tab.id };
  });

  return normalise({ ...layout, root, activePaneId: targetPaneId });
}

/**
 * Split a pane, moving the active tab into the new half.
 *
 * Splitting with nothing open still produces two panes, which is what a user
 * who splits before opening anything expects.
 */
export function splitPane(
  layout: PaneLayout,
  paneId: string,
  direction: SplitDirection,
  moveActiveTab = true,
): PaneLayout {
  const source = findLeaf(layout.root, paneId);
  if (!source) return layout;

  const active = moveActiveTab ? activeTabOf(source) : null;
  const newPaneId = nextId('pane');

  const root = mapLeaf(layout.root, paneId, (leaf) => {
    const keptTabs = active ? leaf.tabs.filter((tab) => tab.id !== active.id) : leaf.tabs;
    const first: PaneNode = {
      ...leaf,
      tabs: keptTabs,
      activeTabId: keptTabs.some((tab) => tab.id === leaf.activeTabId)
        ? leaf.activeTabId
        : (keptTabs[0]?.id ?? null),
    };
    const second: PaneNode = {
      type: 'leaf',
      id: newPaneId,
      tabs: active ? [active] : [],
      activeTabId: active?.id ?? null,
    };
    return {
      type: 'split',
      id: nextId('split'),
      direction,
      ratio: 0.5,
      first,
      second,
    };
  });

  return { root, activePaneId: newPaneId };
}

/** Change the size ratio of a split, clamped so neither side can vanish. */
export function resizeSplit(layout: PaneLayout, splitId: string, ratio: number): PaneLayout {
  const clamped = Math.max(0.15, Math.min(0.85, ratio));
  const apply = (node: PaneNode): PaneNode => {
    if (node.type === 'leaf') return node;
    if (node.id === splitId) return { ...node, ratio: clamped };
    return { ...node, first: apply(node.first), second: apply(node.second) };
  };
  return { ...layout, root: apply(layout.root) };
}

export function closePane(layout: PaneLayout, paneId: string): PaneLayout {
  const remainingLeaves = leaves(layout.root);
  if (remainingLeaves.length <= 1) {
    // Never remove the last pane; empty it instead, so there is always
    // somewhere to open a note.
    return {
      ...layout,
      root: mapLeaf(layout.root, paneId, (leaf) => ({ ...leaf, tabs: [], activeTabId: null })),
    };
  }
  return normalise({
    ...layout,
    root: mapLeaf(layout.root, paneId, (leaf) => ({ ...leaf, tabs: [], activeTabId: null })),
  });
}

/**
 * Collapse splits whose leaves are empty, and make sure the active pane still
 * exists.
 *
 * Called after anything that can empty a pane, so closing the last tab in a
 * split does not leave a dead region on screen.
 */
export function normalise(layout: PaneLayout): PaneLayout {
  const collapse = (node: PaneNode): PaneNode | null => {
    if (node.type === 'leaf') return node.tabs.length > 0 ? node : null;
    const first = collapse(node.first);
    const second = collapse(node.second);
    if (first && second) return { ...node, first, second };
    return first ?? second;
  };

  const collapsed = collapse(layout.root);
  if (!collapsed) {
    // Everything closed: keep one empty pane rather than an empty tree.
    const onlyLeaf = leaves(layout.root)[0];
    const id = onlyLeaf?.id ?? nextId('pane');
    return {
      root: { type: 'leaf', id, tabs: [], activeTabId: null },
      activePaneId: id,
    };
  }

  const liveIds = leaves(collapsed).map((leaf) => leaf.id);
  const activePaneId = liveIds.includes(layout.activePaneId)
    ? layout.activePaneId
    : (liveIds[0] ?? layout.activePaneId);

  return { root: collapsed, activePaneId };
}

/** Drop tabs whose file no longer exists. */
export function pruneMissing(
  layout: PaneLayout,
  exists: (path: VaultPath) => boolean,
): { layout: PaneLayout; removed: VaultPath[] } {
  const removed: VaultPath[] = [];
  const prune = (node: PaneNode): PaneNode => {
    if (node.type === 'leaf') {
      const tabs = node.tabs.filter((tab) => {
        if (exists(tab.path)) return true;
        removed.push(tab.path);
        return false;
      });
      return {
        ...node,
        tabs,
        activeTabId: tabs.some((tab) => tab.id === node.activeTabId)
          ? node.activeTabId
          : (tabs[0]?.id ?? null),
      };
    }
    return { ...node, first: prune(node.first), second: prune(node.second) };
  };
  return { layout: normalise({ ...layout, root: prune(layout.root) }), removed };
}

/** Point every tab for `from` at `to`, after a rename. */
export function retargetTabs(layout: PaneLayout, from: VaultPath, to: VaultPath): PaneLayout {
  const apply = (node: PaneNode): PaneNode => {
    if (node.type === 'leaf') {
      return {
        ...node,
        tabs: node.tabs.map((tab) => (tab.path === from ? { ...tab, path: to } : tab)),
      };
    }
    return { ...node, first: apply(node.first), second: apply(node.second) };
  };
  return { ...layout, root: apply(layout.root) };
}

/** Remember where the cursor was, so reopening a tab lands in the same place. */
export function rememberPosition(
  layout: PaneLayout,
  tabId: string,
  position: { scrollLine?: number; cursorOffset?: number },
): PaneLayout {
  const owner = findLeafOfTab(layout.root, tabId);
  if (!owner) return layout;
  return {
    ...layout,
    root: mapLeaf(layout.root, owner.id, (leaf) => ({
      ...leaf,
      tabs: leaf.tabs.map((tab) => (tab.id === tabId ? { ...tab, ...position } : tab)),
    })),
  };
}

/** The next or previous tab in the active pane, for Ctrl+Tab. */
export function cycleTab(layout: PaneLayout, direction: 1 | -1): PaneLayout {
  const pane = findLeaf(layout.root, layout.activePaneId) ?? leaves(layout.root)[0];
  if (!pane || pane.tabs.length < 2) return layout;
  const index = pane.tabs.findIndex((tab) => tab.id === pane.activeTabId);
  const next = pane.tabs[(index + direction + pane.tabs.length) % pane.tabs.length];
  return next ? setActiveTab(layout, next.id) : layout;
}

/** Move focus to the next pane, for cycling between splits. */
export function cyclePane(layout: PaneLayout, direction: 1 | -1): PaneLayout {
  const panes = leaves(layout.root);
  if (panes.length < 2) return layout;
  const index = panes.findIndex((pane) => pane.id === layout.activePaneId);
  const next = panes[(index + direction + panes.length) % panes.length];
  return next ? { ...layout, activePaneId: next.id } : layout;
}
