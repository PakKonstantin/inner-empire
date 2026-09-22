/**
 * Taking a clause back out of a query.
 *
 * The chips themselves are described by the backend's parser, so what is left
 * to get right here is the cut: a clause's `source` is exactly the text the
 * user typed, and removing it has to leave a query that still says what the
 * remaining chips claim it says.
 */

import { describe, expect, it } from 'vitest';

import { withoutClause } from './SearchPanel';

describe('withoutClause', () => {
  it('removes a clause and closes the gap', () => {
    expect(withoutClause('gradient tag:AI path:Projects', 'tag:AI')).toBe(
      'gradient path:Projects',
    );
  });

  it('removes the first clause without leaving leading space', () => {
    expect(withoutClause('gradient tag:AI', 'gradient')).toBe('tag:AI');
  });

  it('removes the last clause without leaving trailing space', () => {
    expect(withoutClause('gradient tag:AI', 'tag:AI')).toBe('gradient');
  });

  it('removes the only clause, leaving nothing', () => {
    expect(withoutClause('tag:AI', 'tag:AI')).toBe('');
  });

  it('keeps a quoted phrase intact when removing something else', () => {
    expect(withoutClause('"design notes" tag:AI', 'tag:AI')).toBe('"design notes"');
    expect(withoutClause('"design notes" tag:AI', '"design notes"')).toBe('tag:AI');
  });

  it('removes a negated clause with its dash', () => {
    expect(withoutClause('gradient -draft', '-draft')).toBe('gradient');
  });

  it('removes only the first occurrence', () => {
    // Two identical clauses are unusual but legal; removing the chip should
    // take one away rather than all of them.
    expect(withoutClause('tag:AI tag:AI', 'tag:AI')).toBe('tag:AI');
  });

  it('leaves the query alone when the clause is not in it', () => {
    // The query can change between the describe call and the click.
    expect(withoutClause('gradient', 'tag:AI')).toBe('gradient');
  });

  it('collapses the extra whitespace a removal leaves behind', () => {
    expect(withoutClause('a   tag:AI   b', 'tag:AI')).toBe('a b');
  });
});
