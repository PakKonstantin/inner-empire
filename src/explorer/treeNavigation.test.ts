/**
 * Moving around the tree with the arrow keys.
 *
 * The tree the user sees is a flat array with a depth on each row, so every
 * relationship a tree normally has — parent, sibling, first child — has to be
 * derived from the order and the depths. These are the derivations, and the
 * edges are where they go wrong.
 */

import { describe, expect, it } from 'vitest';

import { asVaultPath } from '@/types/domain';

import { parentIndex, verticalTarget } from './treeNavigation';
import type { TreeRow } from './useFileTree';

/**
 * A small vault:
 *
 *   Projects/            depth 0
 *     2026/              depth 1
 *       Launch.md        depth 2
 *     Notes.md           depth 1
 *   Inbox/               depth 0
 *   Readme.md            depth 0
 */
const rows: TreeRow[] = [
  { kind: 'folder', path: asVaultPath('Projects'), name: 'Projects', depth: 0, expanded: true },
  { kind: 'folder', path: asVaultPath('Projects/2026'), name: '2026', depth: 1, expanded: true },
  { kind: 'file', path: asVaultPath('Projects/2026/Launch.md'), name: 'Launch.md', depth: 2 },
  { kind: 'file', path: asVaultPath('Projects/Notes.md'), name: 'Notes.md', depth: 1 },
  { kind: 'folder', path: asVaultPath('Inbox'), name: 'Inbox', depth: 0, expanded: false },
  { kind: 'file', path: asVaultPath('Readme.md'), name: 'Readme.md', depth: 0 },
];

describe('verticalTarget', () => {
  it('steps down and up one row at a time', () => {
    expect(verticalTarget(rows, 1, 'ArrowDown')).toBe(2);
    expect(verticalTarget(rows, 1, 'ArrowUp')).toBe(0);
  });

  it('stops at the ends instead of wrapping', () => {
    expect(verticalTarget(rows, rows.length - 1, 'ArrowDown')).toBe(rows.length - 1);
    expect(verticalTarget(rows, 0, 'ArrowUp')).toBe(0);
  });

  it('starts at the top going down and at the bottom going up', () => {
    // Nothing focused yet: the first key press should land somewhere useful
    // rather than doing nothing.
    expect(verticalTarget(rows, -1, 'ArrowDown')).toBe(0);
    expect(verticalTarget(rows, -1, 'ArrowUp')).toBe(rows.length - 1);
  });

  it('jumps to either end', () => {
    expect(verticalTarget(rows, 3, 'Home')).toBe(0);
    expect(verticalTarget(rows, 3, 'End')).toBe(rows.length - 1);
  });

  it('does nothing in an empty tree', () => {
    expect(verticalTarget([], -1, 'ArrowDown')).toBeNull();
    expect(verticalTarget([], -1, 'Home')).toBeNull();
  });
});

describe('parentIndex', () => {
  it('finds the folder a row sits in', () => {
    expect(parentIndex(rows, 2)).toBe(1);
    expect(parentIndex(rows, 3)).toBe(0);
  });

  it('skips past deeper rows in between', () => {
    // Notes.md is at depth 1; the rows above it include a depth-2 file that
    // is not its parent.
    expect(rows[2]?.depth).toBe(2);
    expect(parentIndex(rows, 3)).toBe(0);
  });

  it('has no parent at the top level', () => {
    expect(parentIndex(rows, 0)).toBeNull();
    expect(parentIndex(rows, 4)).toBeNull();
    expect(parentIndex(rows, 5)).toBeNull();
  });

  it('has no parent for a row that is not there', () => {
    expect(parentIndex(rows, -1)).toBeNull();
    expect(parentIndex(rows, 99)).toBeNull();
  });
});
