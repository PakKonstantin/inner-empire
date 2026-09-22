/**
 * Where an arrow key lands in the file tree.
 *
 * Kept apart from the component because this is the part that is easy to get
 * wrong and impossible to see: a flattened tree has no parent pointers, so
 * "go out to the parent" means walking up until the depth drops, and the
 * edges (nothing focused yet, the first row, the last row) each have a right
 * answer that is not obvious.
 *
 * Everything here works on the flattened, currently visible rows — the same
 * array the list renders — so a collapsed folder's children simply are not
 * there to move onto.
 */

import type { TreeRow } from './useFileTree';

/**
 * The row a vertical move lands on, or `null` when the move does nothing.
 *
 * `from` is the focused row's index, or -1 when nothing is focused.
 */
export function verticalTarget(
  rows: TreeRow[],
  from: number,
  key: 'ArrowDown' | 'ArrowUp' | 'Home' | 'End',
): number | null {
  if (rows.length === 0) return null;

  switch (key) {
    case 'Home':
      return 0;
    case 'End':
      return rows.length - 1;
    case 'ArrowDown':
      // From nowhere, Down starts at the top.
      return clamp(rows, from + 1);
    case 'ArrowUp':
      // From nowhere, Up starts at the bottom, which is what a list that has
      // been scrolled to its end makes you expect.
      return from < 0 ? rows.length - 1 : clamp(rows, from - 1);
  }
}

/**
 * The row containing this one, or `null` at the top level.
 *
 * The nearest row above at a shallower depth is the parent, because the rows
 * are a depth-first walk.
 */
export function parentIndex(rows: TreeRow[], from: number): number | null {
  const row = rows[from];
  if (!row) return null;
  for (let above = from - 1; above >= 0; above -= 1) {
    const candidate = rows[above];
    if (candidate && candidate.depth < row.depth) return above;
  }
  return null;
}

function clamp(rows: TreeRow[], index: number): number {
  return Math.max(0, Math.min(rows.length - 1, index));
}
