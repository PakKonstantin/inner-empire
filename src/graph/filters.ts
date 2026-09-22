/**
 * Narrowing what the graph draws.
 *
 * The backend already decides whether attachments, tags and unresolved links
 * are in the data at all — that is a query, and it belongs there. What is
 * here is the part that only makes sense once you can see the picture: hide
 * the notes nothing connects to, or look at one folder at a time.
 *
 * Both work on what has already arrived, so changing them redraws rather than
 * re-queries. Kept apart from the view because the rule that decides which
 * nodes go is the part worth testing, and a canvas cannot be asserted on.
 */

import type { GraphData, GraphEdge, GraphNode } from '@/types/domain';

export interface GraphFilters {
  /** Show only notes inside this folder, or everything when null. */
  folder: string | null;
  /** Drop notes with no links at either end. */
  hideOrphans: boolean;
}

/**
 * Apply the filters, dropping any edge left dangling.
 *
 * An edge whose other end has been filtered away has to go too, or the
 * simulation is handed a link to a node that does not exist and the layout
 * quietly breaks.
 */
export function applyGraphFilters(data: GraphData, filters: GraphFilters): GraphData {
  const inFolder = folderPredicate(filters.folder);
  let nodes = data.nodes.filter(inFolder);

  let edges = keepConnecting(data.edges, nodes);

  if (filters.hideOrphans) {
    // Orphan-ness is judged after the folder filter: looking at one folder,
    // a note whose only link leaves that folder is an orphan of this view.
    const linked = new Set<string>();
    for (const edge of edges) {
      linked.add(edge.source);
      linked.add(edge.target);
    }
    nodes = nodes.filter((node) => linked.has(node.id));
    edges = keepConnecting(edges, nodes);
  }

  return { nodes, edges, truncated: data.truncated };
}

/** The folders present in the data, for the chooser, in reading order. */
export function foldersOf(data: GraphData): string[] {
  const folders = new Set<string>();
  for (const node of data.nodes) {
    if (node.folder) folders.add(node.folder);
  }
  return [...folders].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }));
}

function folderPredicate(folder: string | null): (node: GraphNode) => boolean {
  if (!folder) return () => true;
  // A folder includes what is nested inside it, which is what a person means
  // by "show me Projects".
  return (node) => node.folder === folder || node.folder.startsWith(`${folder}/`);
}

function keepConnecting(edges: GraphEdge[], nodes: GraphNode[]): GraphEdge[] {
  const present = new Set(nodes.map((node) => node.id));
  return edges.filter((edge) => present.has(edge.source) && present.has(edge.target));
}
