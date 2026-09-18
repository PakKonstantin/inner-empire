/**
 * Open tabs, panes and unsaved buffers.
 *
 * Only *open* notes are held here. The vault's other forty-nine thousand notes
 * live on disk and in the index, which is the whole reason there is an index.
 *
 * Saving is debounced rather than immediate, because writing on every keystroke
 * would make the watcher, the indexer and the disk all work far harder than the
 * user's typing warrants. The debounce is bounded by a flush on blur, on tab
 * switch and on window close, so the longest a change can sit unsaved is the
 * debounce interval.
 */

import { create } from 'zustand';

import { api } from '@/services/api';
import { events } from '@/services/events';
import type {
  PaneLayout,
  SplitDirection,
  TabMode,
  VaultPath,
  Workspace,
  WorkspaceSidebar,
} from '@/types/domain';
import * as tree from '@/workspace/paneTree';

/** How long after the last keystroke a note is written. */
export const AUTOSAVE_DELAY_MS = 800;
/**
 * How often unsaved text is journalled.
 *
 * Shorter than a person notices and long enough to be cheap. Between this and
 * the autosave above, a crash costs at most a few seconds of typing, and the
 * atomic write guarantees the file itself is never damaged.
 */
export const JOURNAL_INTERVAL_MS = 5000;
/** How often the workspace layout itself is persisted. */
const LAYOUT_SAVE_DELAY_MS = 1200;

/**
 * Everything in a workspace this store does not own.
 *
 * Written as "all of it, minus the managed keys" rather than a list of the
 * unmanaged ones: a field added to `Workspace` later is then carried by
 * default instead of being silently dropped until someone notices.
 */
function unmanagedPartsOf(workspace: Workspace): Partial<Workspace> {
  const managed = new Set(['version', 'layout', 'leftSidebar', 'rightSidebar', 'activeFile']);
  return Object.fromEntries(
    Object.entries(workspace).filter(([key]) => !managed.has(key)),
  ) as Partial<Workspace>;
}

export interface Buffer {
  path: VaultPath;
  /** What the editor currently shows. */
  content: string;
  /** What is on disk, as far as we know. */
  savedContent: string;
  dirty: boolean;
  saving: boolean;
  /** Set when the last save failed, so the status bar can say so. */
  error: string | null;
  /** Modification time when loaded, to detect an external edit. */
  loadedMs: number;
}

interface WorkspaceState {
  layout: PaneLayout;
  leftSidebar: WorkspaceSidebar;
  rightSidebar: WorkspaceSidebar;
  buffers: Record<string, Buffer>;
  /** Files being loaded, so a tab can show a spinner rather than "empty". */
  loading: Record<string, boolean>;
  ready: boolean;
  /**
   * The parts of the workspace file this store does not manage.
   *
   * Favourites are pinned on a phone; the graph's zoom and pan belong to the
   * graph view. `persist` rebuilds the workspace from what it knows, so
   * without carrying these they are replaced by whatever `serde(default)`
   * gives — an empty list, a null — and the user's stars quietly disappear
   * the first time the desktop saves a layout.
   */
  carried: Partial<Workspace>;

  // Lifecycle
  hydrate: () => Promise<void>;
  persist: () => Promise<void>;
  reset: () => Promise<void>;

  // Tabs and panes
  openFile: (path: VaultPath, options?: { mode?: TabMode; paneId?: string }) => Promise<void>;
  closeTab: (tabId: string) => Promise<void>;
  closeOthers: (tabId: string) => void;
  closeAllInPane: (paneId: string) => void;
  setActiveTab: (tabId: string) => void;
  setActivePane: (paneId: string) => void;
  togglePin: (tabId: string) => void;
  reorderTab: (tabId: string, index: number) => void;
  moveTabToPane: (tabId: string, paneId: string, index?: number) => void;
  duplicateTab: (tabId: string) => void;
  setTabMode: (tabId: string, mode: TabMode) => void;
  splitActivePane: (direction: SplitDirection) => void;
  resizeSplit: (splitId: string, ratio: number) => void;
  closePane: (paneId: string) => void;
  cycleTab: (direction: 1 | -1) => void;
  cyclePane: (direction: 1 | -1) => void;
  rememberPosition: (tabId: string, position: { scrollLine?: number; cursorOffset?: number }) => void;

  // Sidebars
  setSidebar: (side: 'left' | 'right', patch: Partial<WorkspaceSidebar>) => void;
  toggleSidebar: (side: 'left' | 'right') => void;

  // Buffers
  editBuffer: (path: VaultPath, content: string) => void;
  saveBuffer: (path: VaultPath) => Promise<void>;
  saveAll: () => Promise<void>;
  reloadBuffer: (path: VaultPath) => Promise<void>;
  discardBuffer: (path: VaultPath) => void;

  // Reacting to the vault changing underneath us
  handleExternalChange: (path: VaultPath) => Promise<void>;
  handleExternalDelete: (path: VaultPath) => void;
  handleRename: (from: VaultPath, to: VaultPath) => void;
}

const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();
let layoutTimer: ReturnType<typeof setTimeout> | null = null;
let journalTimer: ReturnType<typeof setInterval> | null = null;

function defaultSidebar(side: 'left' | 'right'): WorkspaceSidebar {
  return side === 'left'
    ? { visible: true, width: 260, activePanel: 'files' }
    : { visible: true, width: 300, activePanel: 'backlinks' };
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  carried: {},
  layout: tree.emptyLayout(),
  leftSidebar: defaultSidebar('left'),
  rightSidebar: defaultSidebar('right'),
  buffers: {},
  loading: {},
  ready: false,

  async hydrate() {
    try {
      const { workspace, removed } = await api.loadWorkspace();
      set({
        layout: workspace.layout,
        leftSidebar: workspace.leftSidebar,
        rightSidebar: workspace.rightSidebar,
        carried: unmanagedPartsOf(workspace),
        ready: true,
      });
      if (removed.length > 0) {
        events.emit('notice', {
          level: 'info',
          message:
            removed.length === 1
              ? `Closed one tab whose file no longer exists.`
              : `Closed ${removed.length} tabs whose files no longer exist.`,
        });
      }
      // Load the content of whatever was open, so restoring a session does not
      // show a row of blank editors.
      await Promise.all(
        tree.allTabs(workspace.layout.root).map((tab) => get().reloadBuffer(tab.path)),
      );
    } catch {
      set({ layout: tree.emptyLayout(), ready: true });
    }
  },

  async persist() {
    const { layout, leftSidebar, rightSidebar, carried } = get();
    const activeTab = activeTabOfLayout(layout);
    const workspace: Workspace = {
      // Whatever this store does not manage goes back untouched, and is
      // listed first so the managed fields below always win.
      ...carried,
      version: 1,
      layout,
      leftSidebar,
      rightSidebar,
      activeFile: activeTab?.path ?? null,
      // Required by the type, so it cannot come from the spread alone — but
      // the carried value is what goes back, and null only when there was
      // none to begin with.
      graphState: carried.graphState ?? null,
    };
    try {
      await api.saveWorkspace(workspace);
    } catch {
      // The layout is a convenience; failing to save it must not interrupt
      // the user's work. It will be retried on the next change.
    }
  },

  async reset() {
    const workspace = await api.resetWorkspace();
    set({
      layout: workspace.layout,
      leftSidebar: workspace.leftSidebar,
      rightSidebar: workspace.rightSidebar,
      carried: unmanagedPartsOf(workspace),
      buffers: {},
    });
  },

  async openFile(path, options = {}) {
    const paneId = options.paneId ?? get().layout.activePaneId;
    const mode = options.mode ?? inferMode(path);
    set((state) => ({ layout: tree.openInPane(state.layout, paneId, path, mode) }));
    schedulePersist(get);
    if (mode === 'edit' || mode === 'read') {
      await get().reloadBuffer(path);
    }
  },

  async closeTab(tabId) {
    const tab = tree.allTabs(get().layout.root).find((candidate) => candidate.id === tabId);
    // Flush before closing, so a tab closed straight after typing does not lose
    // the last keystrokes.
    if (tab) {
      const buffer = get().buffers[tab.path];
      if (buffer?.dirty) await get().saveBuffer(tab.path);
    }

    set((state) => ({ layout: tree.closeTab(state.layout, tabId) }));

    // Drop the buffer once no tab references the file any more.
    if (tab) {
      const stillOpen = tree
        .allTabs(get().layout.root)
        .some((candidate) => candidate.path === tab.path);
      if (!stillOpen) get().discardBuffer(tab.path);
    }
    schedulePersist(get);
  },

  closeOthers(tabId) {
    set((state) => ({ layout: tree.closeOthers(state.layout, tabId) }));
    schedulePersist(get);
  },

  closeAllInPane(paneId) {
    set((state) => ({ layout: tree.closeAllInPane(state.layout, paneId) }));
    schedulePersist(get);
  },

  setActiveTab(tabId) {
    // Switching tabs flushes the outgoing one, so its content is on disk
    // before anything else can read it.
    const previous = activeTabOfLayout(get().layout);
    if (previous && previous.id !== tabId && get().buffers[previous.path]?.dirty) {
      void get().saveBuffer(previous.path);
    }
    set((state) => ({ layout: tree.setActiveTab(state.layout, tabId) }));
    schedulePersist(get);
  },

  setActivePane(paneId) {
    set((state) => ({ layout: { ...state.layout, activePaneId: paneId } }));
    schedulePersist(get);
  },

  togglePin(tabId) {
    set((state) => ({ layout: tree.togglePin(state.layout, tabId) }));
    schedulePersist(get);
  },

  reorderTab(tabId, index) {
    set((state) => ({ layout: tree.reorderTab(state.layout, tabId, index) }));
    schedulePersist(get);
  },

  moveTabToPane(tabId, paneId, index) {
    set((state) => ({ layout: tree.moveTabToPane(state.layout, tabId, paneId, index) }));
    schedulePersist(get);
  },

  duplicateTab(tabId) {
    const tab = tree.allTabs(get().layout.root).find((candidate) => candidate.id === tabId);
    if (!tab) return;
    set((state) => ({
      layout: tree.openInPane(state.layout, state.layout.activePaneId, tab.path, tab.mode),
    }));
    schedulePersist(get);
  },

  setTabMode(tabId, mode) {
    set((state) => {
      const owner = tree.findLeafOfTab(state.layout.root, tabId);
      if (!owner) return state;
      return {
        layout: {
          ...state.layout,
          root: tree.mapLeaf(state.layout.root, owner.id, (leaf) => ({
            ...leaf,
            tabs: leaf.tabs.map((tab) => (tab.id === tabId ? { ...tab, mode } : tab)),
          })),
        },
      };
    });
    schedulePersist(get);
  },

  splitActivePane(direction) {
    set((state) => ({ layout: tree.splitPane(state.layout, state.layout.activePaneId, direction) }));
    schedulePersist(get);
  },

  resizeSplit(splitId, ratio) {
    set((state) => ({ layout: tree.resizeSplit(state.layout, splitId, ratio) }));
    schedulePersist(get);
  },

  closePane(paneId) {
    set((state) => ({ layout: tree.closePane(state.layout, paneId) }));
    schedulePersist(get);
  },

  cycleTab(direction) {
    set((state) => ({ layout: tree.cycleTab(state.layout, direction) }));
  },

  cyclePane(direction) {
    set((state) => ({ layout: tree.cyclePane(state.layout, direction) }));
  },

  rememberPosition(tabId, position) {
    set((state) => ({ layout: tree.rememberPosition(state.layout, tabId, position) }));
  },

  setSidebar(side, patch) {
    set((state) =>
      side === 'left'
        ? { leftSidebar: { ...state.leftSidebar, ...patch } }
        : { rightSidebar: { ...state.rightSidebar, ...patch } },
    );
    schedulePersist(get);
  },

  toggleSidebar(side) {
    const current = side === 'left' ? get().leftSidebar : get().rightSidebar;
    get().setSidebar(side, { visible: !current.visible });
  },

  editBuffer(path, content) {
    set((state) => {
      const buffer = state.buffers[path];
      if (!buffer) return state;
      return {
        buffers: {
          ...state.buffers,
          [path]: { ...buffer, content, dirty: content !== buffer.savedContent, error: null },
        },
      };
    });

    const existing = saveTimers.get(path);
    if (existing) clearTimeout(existing);
    saveTimers.set(
      path,
      setTimeout(() => {
        saveTimers.delete(path);
        void get().saveBuffer(path);
      }, AUTOSAVE_DELAY_MS),
    );
  },

  async saveBuffer(path) {
    const timer = saveTimers.get(path);
    if (timer) {
      clearTimeout(timer);
      saveTimers.delete(path);
    }

    const buffer = get().buffers[path];
    if (!buffer || !buffer.dirty || buffer.saving) return;

    const content = buffer.content;
    set((state) => ({
      buffers: { ...state.buffers, [path]: { ...buffer, saving: true } },
    }));

    try {
      await api.saveNote(path, content);
      set((state) => {
        const current = state.buffers[path];
        if (!current) return state;
        // Compare against what was written, not against the buffer as it is
        // now: the user may have typed more while the write was in flight.
        return {
          buffers: {
            ...state.buffers,
            [path]: {
              ...current,
              savedContent: content,
              dirty: current.content !== content,
              saving: false,
              error: null,
            },
          },
        };
      });
    } catch (error) {
      const message = error instanceof Object && 'message' in error ? String(error.message) : String(error);
      set((state) => {
        const current = state.buffers[path];
        if (!current) return state;
        return {
          buffers: { ...state.buffers, [path]: { ...current, saving: false, error: message } },
        };
      });
      events.emit('notice', { level: 'error', message: `Could not save ${path}: ${message}` });
    }
  },

  async saveAll() {
    const dirty = Object.values(get().buffers).filter((buffer) => buffer.dirty);
    await Promise.all(dirty.map((buffer) => get().saveBuffer(buffer.path)));
  },

  async reloadBuffer(path) {
    set((state) => ({ loading: { ...state.loading, [path]: true } }));
    try {
      const note = await api.readNote(path);
      set((state) => ({
        buffers: {
          ...state.buffers,
          [path]: {
            path,
            content: note.content,
            savedContent: note.content,
            dirty: false,
            saving: false,
            error: null,
            loadedMs: note.modifiedMs,
          },
        },
        loading: { ...state.loading, [path]: false },
      }));
    } catch (error) {
      const message = error instanceof Object && 'message' in error ? String(error.message) : String(error);
      set((state) => ({ loading: { ...state.loading, [path]: false } }));
      events.emit('notice', { level: 'error', message: `Could not open ${path}: ${message}` });
    }
  },

  discardBuffer(path) {
    const timer = saveTimers.get(path);
    if (timer) {
      clearTimeout(timer);
      saveTimers.delete(path);
    }
    set((state) => ({ buffers: without(state.buffers, path) }));
  },

  async handleExternalChange(path) {
    const buffer = get().buffers[path];
    if (!buffer) return;

    if (buffer.dirty) {
      // Two writers. Keeping the user's unsaved text is the only safe choice —
      // it is the one thing the disk does not already have.
      events.emit('notice', {
        level: 'warning',
        message: `${path} changed on disk while you had unsaved edits. Your version is still open; save to overwrite, or close the tab without saving to keep the version on disk.`,
      });
      return;
    }
    await get().reloadBuffer(path);
  },

  handleExternalDelete(path) {
    const stillOpen = tree.allTabs(get().layout.root).some((tab) => tab.path === path);
    if (!stillOpen) return;
    const buffer = get().buffers[path];
    if (buffer?.dirty) {
      events.emit('notice', {
        level: 'warning',
        message: `${path} was deleted elsewhere, but you have unsaved edits. Save to recreate it.`,
      });
      return;
    }
    set((state) => {
      const { layout } = tree.pruneMissing(state.layout, (candidate) => candidate !== path);
      return { layout, buffers: without(state.buffers, path) };
    });
    schedulePersist(get);
  },

  handleRename(from, to) {
    set((state) => {
      const layout = tree.retargetTabs(state.layout, from, to);
      const buffer = state.buffers[from];
      if (!buffer) return { layout };
      return {
        layout,
        buffers: { ...without(state.buffers, from), [to]: { ...buffer, path: to } },
      };
    });
    schedulePersist(get);
  },
}));

/** A copy of `record` without one key. */
function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  const copy = { ...record };
  delete copy[key];
  return copy;
}

function activeTabOfLayout(layout: PaneLayout) {
  const pane = tree.findLeaf(layout.root, layout.activePaneId) ?? tree.leaves(layout.root)[0];
  return pane ? tree.activeTabOf(pane) : null;
}

/** Coalesce layout writes; a drag on a divider fires many of them. */
function schedulePersist(get: () => WorkspaceState): void {
  if (layoutTimer) clearTimeout(layoutTimer);
  layoutTimer = setTimeout(() => {
    layoutTimer = null;
    void get().persist();
  }, LAYOUT_SAVE_DELAY_MS);
}

/** Which view a file should open in, decided by its extension. */
export function inferMode(path: VaultPath): TabMode {
  const extension = path.slice(path.lastIndexOf('.') + 1).toLowerCase();
  if (extension === 'canvas') return 'canvas';
  if (extension === 'pdf') return 'pdf';
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'avif'].includes(extension)) {
    return 'image';
  }
  return 'edit';
}

/** Subscribe to backend events that affect open tabs. */
export function subscribeWorkspaceEvents(): () => void {
  const offs = [
    events.on('fileModified', ({ path }) => {
      void useWorkspaceStore.getState().handleExternalChange(path);
    }),
    events.on('fileDeleted', ({ path }) => {
      useWorkspaceStore.getState().handleExternalDelete(path);
    }),
    events.on('fileRenamed', ({ from, to }) => {
      useWorkspaceStore.getState().handleRename(from, to);
    }),
  ];
  return () => offs.forEach((off) => off());
}

/**
 * Start journalling unsaved buffers.
 *
 * Runs for the life of the window; returns a teardown so tests and hot reloads
 * do not accumulate timers.
 */
export function startRecoveryJournal(): () => void {
  if (journalTimer) clearInterval(journalTimer);

  journalTimer = setInterval(() => {
    const buffers = Object.values(useWorkspaceStore.getState().buffers);
    for (const buffer of buffers) {
      if (!buffer.dirty) continue;
      // A failure here is not worth telling the user about: the journal is a
      // safety net, and the note itself is unaffected.
      void api.journalUnsaved(buffer.path, buffer.content).catch(() => {});
    }
  }, JOURNAL_INTERVAL_MS);

  return () => {
    if (journalTimer) {
      clearInterval(journalTimer);
      journalTimer = null;
    }
  };
}

/** Flush everything, for window close. */
export async function flushPendingWrites(): Promise<void> {
  const store = useWorkspaceStore.getState();
  await store.saveAll();
  if (layoutTimer) {
    clearTimeout(layoutTimer);
    layoutTimer = null;
  }
  await store.persist();
}
