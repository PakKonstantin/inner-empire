/**
 * Going back, and knowing where you are.
 *
 * The trail and the breadcrumb are the two things the toolbar adds that no
 * other part of the application can reconstruct: one records where the user
 * has been, the other says where they are. Both are easy to get subtly wrong
 * — a back button that grows its own history as you retrace it never reaches
 * forward, and a breadcrumb that makes the current note clickable invites a
 * click that does nothing.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { asVaultPath } from '@/types/domain';

vi.mock('@/services/api', () => ({
  api: {
    loadWorkspace: vi.fn(async () => ({ workspace: null, removed: [] })),
    saveWorkspace: vi.fn(async () => {}),
    readNote: vi.fn(async () => ({ content: '', modifiedMs: 0 })),
  },
}));

vi.mock('@/services/events', () => ({
  events: { emit: vi.fn(), on: vi.fn(() => () => {}) },
}));

const { useWorkspaceStore } = await import('@/state/workspaceStore');
const { breadcrumbFor } = await import('@/app/Toolbar');

const a = asVaultPath('A.md');
const b = asVaultPath('B.md');
const c = asVaultPath('C.md');

describe('navigation history', () => {
  beforeEach(() => {
    useWorkspaceStore.setState({ history: [], historyIndex: -1 });
  });

  it('records each note as it is opened', async () => {
    const workspace = useWorkspaceStore.getState();
    await workspace.openFile(a);
    await workspace.openFile(b);

    expect(useWorkspaceStore.getState().history).toEqual([a, b]);
    expect(useWorkspaceStore.getState().historyIndex).toBe(1);
  });

  it('does not record reopening the note already showing', async () => {
    const workspace = useWorkspaceStore.getState();
    await workspace.openFile(a);
    await workspace.openFile(a);

    expect(useWorkspaceStore.getState().history).toEqual([a]);
  });

  it('walks back and forward without growing the trail', async () => {
    const workspace = useWorkspaceStore.getState();
    await workspace.openFile(a);
    await workspace.openFile(b);
    await useWorkspaceStore.getState().goBack();

    expect(useWorkspaceStore.getState().historyIndex).toBe(0);
    // Going back must not count as a visit, or forward becomes unreachable.
    expect(useWorkspaceStore.getState().history).toEqual([a, b]);

    await useWorkspaceStore.getState().goForward();
    expect(useWorkspaceStore.getState().historyIndex).toBe(1);
    expect(useWorkspaceStore.getState().history).toEqual([a, b]);
  });

  it('discards the forward trail once you go somewhere new', async () => {
    const workspace = useWorkspaceStore.getState();
    await workspace.openFile(a);
    await workspace.openFile(b);
    await useWorkspaceStore.getState().goBack();
    await useWorkspaceStore.getState().openFile(c);

    expect(useWorkspaceStore.getState().history).toEqual([a, c]);
    expect(useWorkspaceStore.getState().historyIndex).toBe(1);
  });

  it('stops at the ends rather than running off them', async () => {
    const workspace = useWorkspaceStore.getState();
    await workspace.openFile(a);

    await useWorkspaceStore.getState().goBack();
    expect(useWorkspaceStore.getState().historyIndex).toBe(0);

    await useWorkspaceStore.getState().goForward();
    expect(useWorkspaceStore.getState().historyIndex).toBe(0);
  });

  it('keeps the trail bounded', async () => {
    const workspace = useWorkspaceStore.getState();
    for (let index = 0; index < 140; index += 1) {
      await workspace.openFile(asVaultPath(`Note ${index}.md`));
    }

    const { history, historyIndex } = useWorkspaceStore.getState();
    expect(history).toHaveLength(100);
    expect(historyIndex).toBe(99);
    // The oldest entries are the ones dropped, not the newest.
    expect(history[99]).toBe(asVaultPath('Note 139.md'));
  });
});

describe('breadcrumbFor', () => {
  const reveal = vi.fn();

  it('shows the vault alone when nothing is open', () => {
    expect(breadcrumbFor('Notes', null, null, reveal)).toEqual([
      expect.objectContaining({ label: 'Notes' }),
    ]);
  });

  it('walks the folders and ends on the note', () => {
    const segments = breadcrumbFor(
      'Notes',
      asVaultPath('Projects/2026/Launch.md'),
      'Launch plan',
      reveal,
    );

    expect(segments.map((segment) => segment.label)).toEqual([
      'Notes',
      'Projects',
      '2026',
      'Launch plan',
    ]);
    // Every folder can be shown in the explorer; the note is where you are.
    expect(segments.slice(0, 3).every((segment) => segment.onClick)).toBe(true);
    expect(segments[3]?.onClick).toBeUndefined();
  });

  it('reveals the folder a segment names, not the note', () => {
    const onReveal = vi.fn();
    const segments = breadcrumbFor(
      'Notes',
      asVaultPath('Projects/2026/Launch.md'),
      null,
      onReveal,
    );

    segments[2]?.onClick?.();
    expect(onReveal).toHaveBeenCalledWith(asVaultPath('Projects/2026'));
  });

  it('falls back to the file name when a note has no title', () => {
    const segments = breadcrumbFor('Notes', asVaultPath('Inbox/Scratch.md'), '', reveal);
    expect(segments.at(-1)?.label).toBe('Scratch.md');
  });
});
