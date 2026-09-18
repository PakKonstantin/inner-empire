import { describe, expect, it } from 'vitest';

import { asVaultPath } from '@/types/domain';

import {
  anchorOf,
  boundingBox,
  groupContaining,
  inferSides,
  nodesInRectangle,
  parseCanvas,
  serializeCanvas,
  type CanvasDocument,
  type CanvasNode,
} from './model';

function node(partial: Partial<CanvasNode> & { id: string }): CanvasNode {
  return {
    type: 'text',
    text: '',
    x: 0,
    y: 0,
    width: 100,
    height: 100,
    ...partial,
  } as CanvasNode;
}

describe('parsing', () => {
  it('reads nodes and edges', () => {
    const source = JSON.stringify({
      nodes: [
        { id: 'a', type: 'text', x: 0, y: 0, width: 200, height: 100, text: 'hello' },
        { id: 'b', type: 'file', x: 300, y: 0, width: 200, height: 100, file: 'Note.md' },
      ],
      edges: [{ id: 'e1', fromNode: 'a', toNode: 'b' }],
    });

    const parsed = parseCanvas(source);
    expect(parsed.nodes).toHaveLength(2);
    expect(parsed.edges).toHaveLength(1);
  });

  it('returns an empty canvas for a file that is not JSON', () => {
    expect(parseCanvas('{ truncated')).toEqual({ nodes: [], edges: [] });
    expect(parseCanvas('')).toEqual({ nodes: [], edges: [] });
  });

  it('keeps the valid cards when one is malformed', () => {
    const source = JSON.stringify({
      nodes: [
        { id: 'good', type: 'text', x: 0, y: 0, width: 1, height: 1, text: 'kept' },
        { id: 'bad', type: 'text' },
        { type: 'text', x: 0, y: 0, width: 1, height: 1 },
      ],
    });
    const parsed = parseCanvas(source);
    expect(parsed.nodes.map((n) => n.id)).toEqual(['good']);
  });

  it('drops an edge whose endpoints are missing', () => {
    const source = JSON.stringify({
      nodes: [{ id: 'a', type: 'text', x: 0, y: 0, width: 1, height: 1, text: '' }],
      edges: [
        { id: 'e1', fromNode: 'a', toNode: 'ghost' },
        { id: 'e2', fromNode: 'a', toNode: 'a' },
      ],
    });
    expect(parseCanvas(source).edges.map((e) => e.id)).toEqual(['e2']);
  });
});

describe('serialising', () => {
  it('round trips a document', () => {
    const original: CanvasDocument = {
      nodes: [
        { id: 'a', type: 'text', text: 'hello', x: 10, y: 20, width: 200, height: 100 },
        {
          id: 'b',
          type: 'file',
          file: asVaultPath('Notes/Other.md'),
          x: 300,
          y: 20,
          width: 200,
          height: 100,
        },
      ],
      edges: [{ id: 'e1', fromNode: 'a', toNode: 'b', toEnd: 'arrow' }],
    };

    expect(parseCanvas(serializeCanvas(original))).toEqual(original);
  });

  it('rounds coordinates so a drag does not produce noisy diffs', () => {
    const document: CanvasDocument = {
      nodes: [node({ id: 'a', x: 10.7381, y: 20.2, width: 100.9, height: 50.4 })],
      edges: [],
    };
    const text = serializeCanvas(document);
    expect(text).toContain('"x": 11');
    expect(text).toContain('"y": 20');
    expect(text).not.toContain('10.7381');
  });

  it('omits optional keys that are not set', () => {
    const text = serializeCanvas({ nodes: [node({ id: 'a' })], edges: [] });
    expect(text).not.toContain('color');
    expect(text).not.toContain('subpath');
  });

  it('ends with a newline, like every other file in the vault', () => {
    expect(serializeCanvas({ nodes: [], edges: [] }).endsWith('\n')).toBe(true);
  });
});

describe('geometry', () => {
  it('anchors on each side of a card', () => {
    const card = node({ id: 'a', x: 100, y: 100, width: 200, height: 100 });
    expect(anchorOf(card, 'top')).toEqual({ x: 200, y: 100 });
    expect(anchorOf(card, 'bottom')).toEqual({ x: 200, y: 200 });
    expect(anchorOf(card, 'left')).toEqual({ x: 100, y: 150 });
    expect(anchorOf(card, 'right')).toEqual({ x: 300, y: 150 });
  });

  it('picks sides from relative position', () => {
    const origin = node({ id: 'a', x: 0, y: 0, width: 100, height: 100 });
    expect(inferSides(origin, node({ id: 'b', x: 400, y: 0, width: 100, height: 100 }))).toEqual([
      'right',
      'left',
    ]);
    expect(inferSides(origin, node({ id: 'b', x: -400, y: 0, width: 100, height: 100 }))).toEqual([
      'left',
      'right',
    ]);
    expect(inferSides(origin, node({ id: 'b', x: 0, y: 400, width: 100, height: 100 }))).toEqual([
      'bottom',
      'top',
    ]);
    expect(inferSides(origin, node({ id: 'b', x: 0, y: -400, width: 100, height: 100 }))).toEqual([
      'top',
      'bottom',
    ]);
  });

  it('selects only cards fully inside the rubber band', () => {
    const nodes = [
      node({ id: 'inside', x: 10, y: 10, width: 50, height: 50 }),
      node({ id: 'straddling', x: 90, y: 10, width: 50, height: 50 }),
      node({ id: 'outside', x: 500, y: 500, width: 50, height: 50 }),
    ];
    const selected = nodesInRectangle(nodes, { x: 0, y: 0, width: 100, height: 100 });
    expect(selected.map((n) => n.id)).toEqual(['inside']);
  });

  it('handles a rubber band dragged up and to the left', () => {
    const nodes = [node({ id: 'a', x: 10, y: 10, width: 50, height: 50 })];
    const selected = nodesInRectangle(nodes, { x: 100, y: 100, width: -100, height: -100 });
    expect(selected.map((n) => n.id)).toEqual(['a']);
  });

  it('computes a bounding box', () => {
    const box = boundingBox([
      node({ id: 'a', x: 0, y: 0, width: 100, height: 100 }),
      node({ id: 'b', x: 200, y: 50, width: 100, height: 100 }),
    ]);
    expect(box).toEqual({ x: 0, y: 0, width: 300, height: 150 });
  });

  it('has no bounding box for an empty canvas', () => {
    expect(boundingBox([])).toBeNull();
  });

  it('finds the smallest group containing a card', () => {
    const card = node({ id: 'card', x: 50, y: 50, width: 50, height: 50 });
    const nodes = [
      card,
      node({ id: 'big', type: 'group', x: 0, y: 0, width: 400, height: 400 }),
      node({ id: 'small', type: 'group', x: 20, y: 20, width: 200, height: 200 }),
      node({ id: 'elsewhere', type: 'group', x: 500, y: 500, width: 100, height: 100 }),
    ];
    expect(groupContaining(card, nodes)?.id).toBe('small');
  });

  it('returns nothing when a card is in no group', () => {
    const card = node({ id: 'card', x: 900, y: 900, width: 50, height: 50 });
    expect(groupContaining(card, [card])).toBeNull();
  });
});
