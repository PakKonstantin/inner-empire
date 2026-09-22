/**
 * The workspace round-trip.
 *
 * `persist` rebuilds the workspace object from the fields this store manages.
 * Anything else the file carries — favourites pinned on a phone, the graph's
 * zoom and pan — is not in that object, and `serde(default)` fills the gap
 * with an empty value on the way back in. The user's stars vanish, and the
 * only clue is that it happened after opening the desktop.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Workspace } from '@/types/domain';

const saved: Workspace[] = [];

vi.mock('@/services/api', () => ({
  api: {
    loadWorkspace: vi.fn(async () => ({
      workspace: {
        version: 1,
        layout: {
          root: { type: 'leaf', id: 'pane-root', tabs: [], activeTabId: null },
          activePaneId: 'pane-root',
        },
        leftSidebar: { visible: true, width: 280, activePanel: 'files' },
        rightSidebar: { visible: false, width: 280, activePanel: 'backlinks' },
        activeFile: null,
        graphState: { zoom: 2.5, pan: { x: 10, y: 20 } },
        favourites: ['Pinned/On A Phone.md'],
      } as unknown as Workspace,
      removed: [],
    })),
    saveWorkspace: vi.fn(async (workspace: Workspace) => {
      saved.push(workspace);
    }),
    readNote: vi.fn(async () => ({ content: '', modifiedMs: 0 })),
  },
}));

vi.mock('@/services/events', () => ({
  events: { emit: vi.fn(), on: vi.fn(() => () => {}) },
}));

const { useWorkspaceStore } = await import('@/state/workspaceStore');

describe('workspace round-trip', () => {
  beforeEach(() => {
    saved.length = 0;
  });

  it('keeps favourites that this platform does not manage', async () => {
    await useWorkspaceStore.getState().hydrate();
    await useWorkspaceStore.getState().persist();

    expect(saved).toHaveLength(1);
    // A note starred on a phone must still be starred after the desktop has
    // saved the layout. Dropping it here deletes it from the vault.
    expect((saved[0] as unknown as { favourites: string[] }).favourites).toEqual([
      'Pinned/On A Phone.md',
    ]);
  });

  it('keeps the graph view state it does not manage either', async () => {
    await useWorkspaceStore.getState().hydrate();
    await useWorkspaceStore.getState().persist();

    expect(saved).toHaveLength(1);
    expect(saved[0]?.graphState).toEqual({ zoom: 2.5, pan: { x: 10, y: 20 } });
  });
});
