/**
 * The canvas file format.
 *
 * A canvas is a JSON file in the vault, so it travels with the notes, can be
 * read by anything, and is diffable in version control. Nothing about it lives
 * in the index except the file's existence, which is what keeps it a document
 * the user owns rather than app state.
 */

import type { VaultPath } from '@/types/domain';

export interface CanvasPoint {
  x: number;
  y: number;
}

export interface CanvasSize {
  width: number;
  height: number;
}

interface NodeBase extends CanvasPoint, CanvasSize {
  id: string;
  /** An index into the palette rather than a literal colour, so a canvas looks
   *  right in both themes. */
  color?: number;
}

export type CanvasNode =
  | (NodeBase & { type: 'text'; text: string })
  | (NodeBase & { type: 'file'; file: VaultPath; subpath?: string })
  | (NodeBase & { type: 'link'; url: string })
  | (NodeBase & { type: 'group'; label?: string });

/** Which side of a card an edge leaves from. */
export type CanvasSide = 'top' | 'right' | 'bottom' | 'left';

export interface CanvasEdge {
  id: string;
  fromNode: string;
  fromSide?: CanvasSide;
  toNode: string;
  toSide?: CanvasSide;
  /** Arrowheads. Defaults to an arrow at the destination only. */
  fromEnd?: 'none' | 'arrow';
  toEnd?: 'none' | 'arrow';
  label?: string;
  color?: number;
}

export interface CanvasDocument {
  nodes: CanvasNode[];
  edges: CanvasEdge[];
}

export const EMPTY_CANVAS: CanvasDocument = { nodes: [], edges: [] };

export const DEFAULT_NODE_SIZE: CanvasSize = { width: 260, height: 160 };
export const DEFAULT_GROUP_SIZE: CanvasSize = { width: 420, height: 320 };

/** The palette a `color` index refers to, resolved from the theme. */
export const CANVAS_COLORS = [
  '--accent',
  '--text-error',
  '--text-warning',
  '--text-success',
  '--graph-node-attachment',
  '--graph-node-tag',
] as const;

export function colorVariable(index: number | undefined): string {
  if (index === undefined) return 'var(--border-strong)';
  return `var(${CANVAS_COLORS[index % CANVAS_COLORS.length]})`;
}

/**
 * Parse a canvas file.
 *
 * A damaged or partly-hand-edited file yields whatever nodes are valid rather
 * than nothing: losing one malformed card is better than losing the board.
 */
export function parseCanvas(source: string): CanvasDocument {
  let raw: unknown;
  try {
    raw = JSON.parse(source);
  } catch {
    return { ...EMPTY_CANVAS };
  }
  if (typeof raw !== 'object' || raw === null) return { ...EMPTY_CANVAS };

  const record = raw as { nodes?: unknown; edges?: unknown };
  const nodes = Array.isArray(record.nodes)
    ? record.nodes.filter(isValidNode)
    : [];
  const known = new Set(nodes.map((node) => node.id));
  const edges = Array.isArray(record.edges)
    ? record.edges.filter(
        (edge): edge is CanvasEdge =>
          isValidEdge(edge) && known.has(edge.fromNode) && known.has(edge.toNode),
      )
    : [];

  return { nodes, edges };
}

function isValidNode(value: unknown): value is CanvasNode {
  if (typeof value !== 'object' || value === null) return false;
  const node = value as Partial<CanvasNode>;
  if (typeof node.id !== 'string' || node.id === '') return false;
  if (!['text', 'file', 'link', 'group'].includes(String(node.type))) return false;
  return (
    typeof node.x === 'number' &&
    typeof node.y === 'number' &&
    typeof node.width === 'number' &&
    typeof node.height === 'number'
  );
}

function isValidEdge(value: unknown): value is CanvasEdge {
  if (typeof value !== 'object' || value === null) return false;
  const edge = value as Partial<CanvasEdge>;
  return (
    typeof edge.id === 'string' &&
    typeof edge.fromNode === 'string' &&
    typeof edge.toNode === 'string'
  );
}

/** Serialise, with stable key order so a saved canvas diffs cleanly. */
export function serializeCanvas(document: CanvasDocument): string {
  return `${JSON.stringify(
    {
      nodes: document.nodes.map(orderNodeKeys),
      edges: document.edges.map(orderEdgeKeys),
    },
    null,
    2,
  )}\n`;
}

function orderNodeKeys(node: CanvasNode): Record<string, unknown> {
  const base: Record<string, unknown> = {
    id: node.id,
    type: node.type,
    x: Math.round(node.x),
    y: Math.round(node.y),
    width: Math.round(node.width),
    height: Math.round(node.height),
  };
  if (node.color !== undefined) base.color = node.color;
  switch (node.type) {
    case 'text':
      base.text = node.text;
      break;
    case 'file':
      base.file = node.file;
      if (node.subpath) base.subpath = node.subpath;
      break;
    case 'link':
      base.url = node.url;
      break;
    case 'group':
      if (node.label) base.label = node.label;
      break;
  }
  return base;
}

function orderEdgeKeys(edge: CanvasEdge): Record<string, unknown> {
  const base: Record<string, unknown> = {
    id: edge.id,
    fromNode: edge.fromNode,
    toNode: edge.toNode,
  };
  if (edge.fromSide) base.fromSide = edge.fromSide;
  if (edge.toSide) base.toSide = edge.toSide;
  if (edge.fromEnd) base.fromEnd = edge.fromEnd;
  if (edge.toEnd) base.toEnd = edge.toEnd;
  if (edge.label) base.label = edge.label;
  if (edge.color !== undefined) base.color = edge.color;
  return base;
}

let counter = 0;

export function newId(prefix: string): string {
  counter += 1;
  return `${prefix}${Date.now().toString(36)}${counter.toString(36)}`;
}

/** The anchor point on one side of a node. */
export function anchorOf(node: CanvasNode, side: CanvasSide): CanvasPoint {
  switch (side) {
    case 'top':
      return { x: node.x + node.width / 2, y: node.y };
    case 'bottom':
      return { x: node.x + node.width / 2, y: node.y + node.height };
    case 'left':
      return { x: node.x, y: node.y + node.height / 2 };
    default:
      return { x: node.x + node.width, y: node.y + node.height / 2 };
  }
}

/**
 * Pick the sides an edge should leave and arrive on, when the file does not
 * say. Choosing by relative position is what makes an arrow look deliberate
 * rather than crossing the card it starts from.
 */
export function inferSides(from: CanvasNode, to: CanvasNode): [CanvasSide, CanvasSide] {
  const dx = to.x + to.width / 2 - (from.x + from.width / 2);
  const dy = to.y + to.height / 2 - (from.y + from.height / 2);

  if (Math.abs(dx) > Math.abs(dy)) {
    return dx > 0 ? ['right', 'left'] : ['left', 'right'];
  }
  return dy > 0 ? ['bottom', 'top'] : ['top', 'bottom'];
}

/** Nodes fully inside a rectangle, for rubber-band selection. */
export function nodesInRectangle(
  nodes: CanvasNode[],
  rectangle: { x: number; y: number; width: number; height: number },
): CanvasNode[] {
  const left = Math.min(rectangle.x, rectangle.x + rectangle.width);
  const right = Math.max(rectangle.x, rectangle.x + rectangle.width);
  const top = Math.min(rectangle.y, rectangle.y + rectangle.height);
  const bottom = Math.max(rectangle.y, rectangle.y + rectangle.height);

  return nodes.filter(
    (node) =>
      node.x >= left &&
      node.y >= top &&
      node.x + node.width <= right &&
      node.y + node.height <= bottom,
  );
}

/** The smallest rectangle containing every node, for "fit to content". */
export function boundingBox(nodes: CanvasNode[]): {
  x: number;
  y: number;
  width: number;
  height: number;
} | null {
  if (nodes.length === 0) return null;
  let left = Infinity;
  let top = Infinity;
  let right = -Infinity;
  let bottom = -Infinity;

  for (const node of nodes) {
    left = Math.min(left, node.x);
    top = Math.min(top, node.y);
    right = Math.max(right, node.x + node.width);
    bottom = Math.max(bottom, node.y + node.height);
  }
  return { x: left, y: top, width: right - left, height: bottom - top };
}

/** Which group, if any, a node sits inside — the smallest one containing it. */
export function groupContaining(
  node: CanvasNode,
  nodes: CanvasNode[],
): CanvasNode | null {
  const candidates = nodes.filter(
    (candidate) =>
      candidate.type === 'group' &&
      candidate.id !== node.id &&
      node.x >= candidate.x &&
      node.y >= candidate.y &&
      node.x + node.width <= candidate.x + candidate.width &&
      node.y + node.height <= candidate.y + candidate.height,
  );
  if (candidates.length === 0) return null;
  return candidates.reduce((smallest, candidate) =>
    candidate.width * candidate.height < smallest.width * smallest.height ? candidate : smallest,
  );
}
