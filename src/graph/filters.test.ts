/**
 * Which nodes the graph keeps.
 *
 * The rule that matters is not which nodes go but which edges go with them:
 * an edge left pointing at a node that is no longer there is handed to the
 * force simulation, and the layout breaks in a way that looks like a physics
 * bug rather than a filtering one.
 */

import { describe, expect, it } from 'vitest';

import { asVaultPath } from '@/types/domain';
import type { GraphData, GraphEdge, GraphNode } from '@/types/domain';

import { applyGraphFilters, foldersOf } from './filters';

function node(id: string, folder: string): GraphNode {
  return {
    id,
    path: asVaultPath(folder ? `${folder}/${id}.md` : `${id}.md`),
    label: id,
    kind: 'note',
    degree: 0,
    tags: [],
    folder,
  };
}

function edge(source: string, target: string): GraphEdge {
  return { source, target, kind: 'wikiLink' };
}

/**
 *   Projects/a ─ Projects/b        (a pair inside one folder)
 *   Projects/Sub/c ─ Archive/d     (a link that crosses folders)
 *   Notes/e                        (linked to nothing at all)
 */
const data: GraphData = {
  nodes: [
    node('a', 'Projects'),
    node('b', 'Projects'),
    node('c', 'Projects/Sub'),
    node('d', 'Archive'),
    node('e', 'Notes'),
  ],
  edges: [edge('a', 'b'), edge('c', 'd')],
  truncated: false,
};

const idsOf = (result: GraphData): string[] => result.nodes.map((n) => n.id);

describe('applyGraphFilters', () => {
  it('keeps everything when nothing is asked for', () => {
    const result = applyGraphFilters(data, { folder: null, hideOrphans: false });
    expect(idsOf(result)).toEqual(['a', 'b', 'c', 'd', 'e']);
    expect(result.edges).toHaveLength(2);
  });

  it('includes what is nested inside the chosen folder', () => {
    // "Show me Projects" means the subfolders too.
    const result = applyGraphFilters(data, { folder: 'Projects', hideOrphans: false });
    expect(idsOf(result)).toEqual(['a', 'b', 'c']);
  });

  it('drops an edge whose other end was filtered away', () => {
    const result = applyGraphFilters(data, { folder: 'Projects', hideOrphans: false });
    // c→d crossed out of Projects, so the link goes with d.
    expect(result.edges).toEqual([edge('a', 'b')]);
  });

  it('does not match a folder whose name is only a prefix', () => {
    const tricky: GraphData = {
      nodes: [node('x', 'Project'), node('y', 'Projects')],
      edges: [],
      truncated: false,
    };
    expect(idsOf(applyGraphFilters(tricky, { folder: 'Project', hideOrphans: false }))).toEqual([
      'x',
    ]);
  });

  it('hides the notes nothing links to', () => {
    const result = applyGraphFilters(data, { folder: null, hideOrphans: true });
    expect(idsOf(result)).toEqual(['a', 'b', 'c', 'd']);
  });

  it('judges orphans within the folder being looked at', () => {
    // c's only link leaves Projects, so inside Projects it is an orphan.
    const result = applyGraphFilters(data, { folder: 'Projects', hideOrphans: true });
    expect(idsOf(result)).toEqual(['a', 'b']);
  });

  it('never leaves an edge pointing at a node it removed', () => {
    for (const folder of [null, 'Projects', 'Archive', 'Notes']) {
      for (const hideOrphans of [false, true]) {
        const result = applyGraphFilters(data, { folder, hideOrphans });
        const present = new Set(result.nodes.map((n) => n.id));
        for (const link of result.edges) {
          expect(present.has(link.source)).toBe(true);
          expect(present.has(link.target)).toBe(true);
        }
      }
    }
  });

  it('carries the truncation flag through', () => {
    const truncated = { ...data, truncated: true };
    expect(applyGraphFilters(truncated, { folder: null, hideOrphans: false }).truncated).toBe(true);
  });
});

describe('foldersOf', () => {
  it('lists each folder once, in reading order', () => {
    expect(foldersOf(data)).toEqual(['Archive', 'Notes', 'Projects', 'Projects/Sub']);
  });

  it('leaves out the vault root, which is not a folder to choose', () => {
    const rooted: GraphData = { nodes: [node('top', '')], edges: [], truncated: false };
    expect(foldersOf(rooted)).toEqual([]);
  });
});
