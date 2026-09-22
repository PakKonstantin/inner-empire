/**
 * The graph.
 *
 * Drawn on a canvas rather than as SVG or DOM nodes: a vault with five
 * thousand notes and twice as many links is tens of thousands of elements, and
 * only an immediate-mode surface stays smooth at that size. `d3-force` runs the
 * simulation; everything drawn here is drawn by hand, which is also what makes
 * it follow the theme's custom properties.
 */

import {
  forceCenter,
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type Simulation,
  type SimulationLinkDatum,
  type SimulationNodeDatum,
} from 'd3-force';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { api } from '@/services/api';
import {
  Button,
  CollapsibleSection,
  IconButton,
  SearchInput,
  Select,
  Toggle,
  Tooltip,
} from '@/ui';

import { applyGraphFilters, foldersOf } from './filters';
import { events } from '@/services/events';
import type { GraphData, GraphNodeKind, VaultPath } from '@/types/domain';

interface Node extends SimulationNodeDatum {
  id: string;
  label: string;
  kind: GraphNodeKind;
  path: VaultPath | null;
  degree: number;
  folder: string;
}

interface Edge extends SimulationLinkDatum<Node> {
  source: string | Node;
  target: string | Node;
}

export interface GraphSettings {
  includeAttachments: boolean;
  includeUnresolved: boolean;
  includeTags: boolean;
  /** Repulsion between nodes. */
  repulsion: number;
  /** How strongly links pull. */
  linkStrength: number;
  /** Hide labels until the view is zoomed in past this scale. */
  labelThreshold: number;
  /** For the local graph: how many hops to include. */
  depth: number;
  /** Show only this folder and what is nested inside it. */
  folder: string | null;
  /** Leave out the notes nothing links to. */
  hideOrphans: boolean;
}

export const DEFAULT_GRAPH_SETTINGS: GraphSettings = {
  includeAttachments: false,
  includeUnresolved: true,
  includeTags: false,
  repulsion: 260,
  linkStrength: 0.35,
  labelThreshold: 0.75,
  depth: 1,
  folder: null,
  hideOrphans: false,
};

export interface GraphViewProps {
  /** When set, only this note's neighbourhood is shown. */
  centerPath?: VaultPath | null;
  activePath: VaultPath | null;
  settings: GraphSettings;
  onSettingsChange: (settings: GraphSettings) => void;
  onOpen: (path: VaultPath, options?: { newPane?: boolean }) => void;
  /** Hide the controls, for the small local-graph panel. */
  compact?: boolean;
}

export function GraphView(props: GraphViewProps) {
  const canvas = useRef<HTMLCanvasElement | null>(null);
  const container = useRef<HTMLDivElement | null>(null);
  const simulation = useRef<Simulation<Node, Edge> | null>(null);
  const nodes = useRef<Node[]>([]);
  const edges = useRef<Edge[]>([]);
  const transform = useRef({ x: 0, y: 0, k: 1 });
  const hovered = useRef<Node | null>(null);
  const dragging = useRef<Node | null>(null);

  const [data, setData] = useState<GraphData | null>(null);
  const [filter, setFilter] = useState('');
  // Mirrored into state only so the readout can show it; the drawing reads
  // the ref, because a number changing sixty times a second is not state.
  const [zoom, setZoom] = useState(1);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const request = {
        includeAttachments: props.settings.includeAttachments,
        includeUnresolved: props.settings.includeUnresolved,
        includeTags: props.settings.includeTags,
      };
      const result = props.centerPath
        ? await api.localGraph(props.centerPath, props.settings.depth, request)
        : await api.graph(request);
      setData(result);
    } catch {
      setData({ nodes: [], edges: [], truncated: false });
    } finally {
      setLoading(false);
    }
  }, [
    props.centerPath,
    props.settings.depth,
    props.settings.includeAttachments,
    props.settings.includeTags,
    props.settings.includeUnresolved,
  ]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const offs = [
      events.on('indexCompleted', () => void load()),
      events.on('indexUpdated', () => void load()),
    ];
    return () => offs.forEach((off) => off());
  }, [load]);

  // The folder list comes from the unfiltered data, so choosing a folder does
  // not remove every other folder from the chooser.
  const folders = useMemo(() => (data ? foldersOf(data) : []), [data]);

  const visible = useMemo(
    () =>
      data
        ? applyGraphFilters(data, {
            folder: props.settings.folder,
            hideOrphans: props.settings.hideOrphans,
          })
        : null,
    [data, props.settings.folder, props.settings.hideOrphans],
  );

  const matchesFilter = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    if (!needle) return null;
    return (node: Node) => node.label.toLowerCase().includes(needle);
  }, [filter]);

  // Build and run the simulation.
  useEffect(() => {
    if (!visible || !container.current) return;

    const width = container.current.clientWidth || 800;
    const height = container.current.clientHeight || 600;

    // Keep positions for nodes that already existed, so a refresh after an
    // edit does not scatter the whole graph.
    const previous = new Map(nodes.current.map((node) => [node.id, node]));
    nodes.current = visible.nodes.map((node) => {
      const existing = previous.get(node.id);
      return {
        id: node.id,
        label: node.label,
        kind: node.kind,
        path: node.path,
        degree: node.degree,
        folder: node.folder,
        x: existing?.x ?? (Math.random() - 0.5) * width,
        y: existing?.y ?? (Math.random() - 0.5) * height,
        vx: existing?.vx ?? 0,
        vy: existing?.vy ?? 0,
      };
    });
    edges.current = visible.edges.map((edge) => ({ source: edge.source, target: edge.target }));

    simulation.current?.stop();
    simulation.current = forceSimulation<Node, Edge>(nodes.current)
      .force(
        'link',
        forceLink<Node, Edge>(edges.current)
          .id((node) => node.id)
          .distance(60)
          .strength(props.settings.linkStrength),
      )
      .force('charge', forceManyBody().strength(-props.settings.repulsion))
      .force('center', forceCenter(0, 0))
      .force('x', forceX(0).strength(0.03))
      .force('y', forceY(0).strength(0.03))
      .force(
        'collide',
        forceCollide<Node>().radius((node) => radiusOf(node) + 3),
      )
      .alpha(1)
      .restart();

    return () => {
      simulation.current?.stop();
    };
  }, [visible, props.settings.linkStrength, props.settings.repulsion]);

  // Draw on every animation frame while the simulation is warm.
  useEffect(() => {
    let frame = 0;
    const render = () => {
      draw(canvas.current, nodes.current, edges.current, transform.current, {
        activePath: props.activePath,
        centerPath: props.centerPath ?? null,
        hovered: hovered.current,
        matchesFilter,
        labelThreshold: props.settings.labelThreshold,
      });
      frame = requestAnimationFrame(render);
    };
    frame = requestAnimationFrame(render);
    return () => cancelAnimationFrame(frame);
  }, [props.activePath, props.centerPath, matchesFilter, props.settings.labelThreshold]);

  // Keep the backing store in step with the element's size and the display's
  // pixel ratio, or the drawing is blurry on a scaled screen.
  useEffect(() => {
    const element = container.current;
    const surface = canvas.current;
    if (!element || !surface) return;

    const resize = () => {
      const ratio = window.devicePixelRatio || 1;
      surface.width = element.clientWidth * ratio;
      surface.height = element.clientHeight * ratio;
      surface.style.width = `${element.clientWidth}px`;
      surface.style.height = `${element.clientHeight}px`;
    };
    resize();

    const observer = new ResizeObserver(resize);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const toGraphSpace = useCallback((event: React.PointerEvent | React.MouseEvent) => {
    const surface = canvas.current;
    if (!surface) return { x: 0, y: 0 };
    const rect = surface.getBoundingClientRect();
    const { x, y, k } = transform.current;
    return {
      x: (event.clientX - rect.left - rect.width / 2 - x) / k,
      y: (event.clientY - rect.top - rect.height / 2 - y) / k,
    };
  }, []);

  const nodeAt = useCallback((point: { x: number; y: number }): Node | null => {
    // Reverse order, so the node drawn last — on top — is the one picked.
    for (let index = nodes.current.length - 1; index >= 0; index -= 1) {
      const node = nodes.current[index]!;
      const dx = (node.x ?? 0) - point.x;
      const dy = (node.y ?? 0) - point.y;
      const radius = radiusOf(node) + 4;
      if (dx * dx + dy * dy <= radius * radius) return node;
    }
    return null;
  }, []);

  /**
   * Zoom about the middle of the view.
   *
   * The wheel zooms towards the pointer, because that is where the eye is.
   * A button has no pointer to aim at, so the centre is the honest choice —
   * the thing you were looking at stays where it was.
   */
  const zoomBy = useCallback((factor: number) => {
    const next = Math.max(0.1, Math.min(6, transform.current.k * factor));
    const ratio = next / transform.current.k;
    transform.current.x *= ratio;
    transform.current.y *= ratio;
    transform.current.k = next;
    setZoom(next);
  }, []);

  /** Frame everything the simulation has laid out. */
  const fitToView = useCallback(() => {
    const surface = canvas.current;
    const laid = nodes.current;
    if (!surface || laid.length === 0) return;

    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const node of laid) {
      const radius = radiusOf(node) + 12;
      minX = Math.min(minX, (node.x ?? 0) - radius);
      minY = Math.min(minY, (node.y ?? 0) - radius);
      maxX = Math.max(maxX, (node.x ?? 0) + radius);
      maxY = Math.max(maxY, (node.y ?? 0) + radius);
    }

    const width = surface.clientWidth || 800;
    const height = surface.clientHeight || 600;
    const k = Math.max(
      0.1,
      Math.min(2, Math.min(width / Math.max(maxX - minX, 1), height / Math.max(maxY - minY, 1))),
    );

    // The drawing is centred on the origin, so the pan is the graph's own
    // centre moved back to the middle of the view.
    transform.current = {
      k,
      x: -((minX + maxX) / 2) * k,
      y: -((minY + maxY) / 2) * k,
    };
    setZoom(k);
  }, []);

  return (
    <div className="ie-graph" ref={container}>
      <canvas
        ref={canvas}
        className="ie-graph__canvas"
        onPointerDown={(event) => {
          const point = toGraphSpace(event);
          const node = nodeAt(point);
          if (node) {
            dragging.current = node;
            node.fx = node.x;
            node.fy = node.y;
            simulation.current?.alphaTarget(0.25).restart();
          }
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onPointerMove={(event) => {
          const point = toGraphSpace(event);
          if (dragging.current) {
            dragging.current.fx = point.x;
            dragging.current.fy = point.y;
            return;
          }
          if (event.buttons === 1) {
            // Dragging the background pans.
            transform.current.x += event.movementX;
            transform.current.y += event.movementY;
            return;
          }
          hovered.current = nodeAt(point);
          if (canvas.current) {
            canvas.current.style.cursor = hovered.current ? 'pointer' : 'grab';
          }
        }}
        onPointerUp={(event) => {
          if (dragging.current) {
            // Release the pin so the node settles back into the layout.
            dragging.current.fx = null;
            dragging.current.fy = null;
            dragging.current = null;
            simulation.current?.alphaTarget(0);
          }
          event.currentTarget.releasePointerCapture(event.pointerId);
        }}
        onClick={(event) => {
          const node = nodeAt(toGraphSpace(event));
          if (node?.path) props.onOpen(node.path, { newPane: event.ctrlKey || event.metaKey });
        }}
        onWheel={(event) => {
          const surface = canvas.current;
          if (!surface) return;
          const rect = surface.getBoundingClientRect();
          const scale = Math.exp(-event.deltaY * 0.001);
          const next = Math.max(0.1, Math.min(6, transform.current.k * scale));
          // Zoom towards the pointer rather than the centre, which is what
          // makes zooming feel like it is under your control.
          const px = event.clientX - rect.left - rect.width / 2;
          const py = event.clientY - rect.top - rect.height / 2;
          const ratio = next / transform.current.k;
          transform.current.x = px - (px - transform.current.x) * ratio;
          transform.current.y = py - (py - transform.current.y) * ratio;
          transform.current.k = next;
          setZoom(next);
        }}
      />

      {props.compact ? null : (
        <>
          <div className="ie-graph__zoom ie-chrome">
            <Tooltip content="Zoom in" placement="left">
              <IconButton icon="plus" label="Zoom in" size="sm" onClick={() => zoomBy(1.25)} />
            </Tooltip>
            <Tooltip content="Zoom out" placement="left">
              <IconButton icon="minus" label="Zoom out" size="sm" onClick={() => zoomBy(1 / 1.25)} />
            </Tooltip>
            <Tooltip content="Fit everything on screen" placement="left">
              <IconButton icon="maximize" label="Fit to view" size="sm" onClick={fitToView} />
            </Tooltip>
            <span className="ie-graph__zoom-level" aria-live="polite">
              {Math.round(zoom * 100)}%
            </span>
          </div>

          <div className="ie-graph__controls ie-chrome">
            <SearchInput
              label="Find a note in the graph"
              placeholder="Find a note"
              value={filter}
              onValueChange={setFilter}
            />

            <CollapsibleSection title="Show" defaultOpen>
              <Toggle
                label="Notes that do not exist yet"
                checked={props.settings.includeUnresolved}
                onChange={(checked) =>
                  props.onSettingsChange({ ...props.settings, includeUnresolved: checked })
                }
              />
              <Toggle
                label="Attachments"
                checked={props.settings.includeAttachments}
                onChange={(checked) =>
                  props.onSettingsChange({ ...props.settings, includeAttachments: checked })
                }
              />
              <Toggle
                label="Tags"
                checked={props.settings.includeTags}
                onChange={(checked) =>
                  props.onSettingsChange({ ...props.settings, includeTags: checked })
                }
              />
              <Toggle
                label="Only notes with links"
                description="Hides the notes nothing connects to."
                checked={props.settings.hideOrphans}
                onChange={(checked) =>
                  props.onSettingsChange({ ...props.settings, hideOrphans: checked })
                }
              />
              <Select
                label="Folder"
                value={props.settings.folder ?? ''}
                onChange={(event) =>
                  props.onSettingsChange({
                    ...props.settings,
                    folder: event.target.value || null,
                  })
                }
                options={[
                  { value: '', label: 'The whole vault' },
                  ...folders.map((folder) => ({ value: folder, label: folder })),
                ]}
              />
            </CollapsibleSection>

            <CollapsibleSection title="Layout">
              <label className="ie-graph__slider">
                Spacing
                <input
                  type="range"
                  min={60}
                  max={600}
                  value={props.settings.repulsion}
                  onChange={(event) =>
                    props.onSettingsChange({ ...props.settings, repulsion: Number(event.target.value) })
                  }
                />
              </label>
              <label className="ie-graph__slider">
                Link pull
                <input
                  type="range"
                  min={5}
                  max={100}
                  value={props.settings.linkStrength * 100}
                  onChange={(event) =>
                    props.onSettingsChange({
                      ...props.settings,
                      linkStrength: Number(event.target.value) / 100,
                    })
                  }
                />
              </label>
              {props.centerPath ? (
                <label className="ie-graph__slider">
                  Depth
                  <input
                    type="range"
                    min={1}
                    max={5}
                    value={props.settings.depth}
                    onChange={(event) =>
                      props.onSettingsChange({ ...props.settings, depth: Number(event.target.value) })
                    }
                  />
                </label>
              ) : null}
              <Button
                variant="quiet"
                size="sm"
                icon="undo"
                onClick={() => {
                  transform.current = { x: 0, y: 0, k: 1 };
                  setZoom(1);
                  simulation.current?.alpha(0.6).restart();
                }}
              >
                Reset the layout
              </Button>
            </CollapsibleSection>
          </div>
        </>
      )}

      <div className="ie-graph__status">
        {loading
          ? 'Building the graph…'
          : visible
            ? `${visible.nodes.length} notes, ${visible.edges.length} links${
                visible.truncated ? ' (showing the most connected)' : ''
              }`
            : ''}
      </div>
    </div>
  );
}

function radiusOf(node: Node): number {
  // Square root, so a note with a hundred links is visibly bigger than one with
  // ten without dwarfing the rest of the graph.
  return 4 + Math.sqrt(node.degree) * 1.8;
}

interface DrawOptions {
  activePath: VaultPath | null;
  centerPath: VaultPath | null;
  hovered: Node | null;
  matchesFilter: ((node: Node) => boolean) | null;
  labelThreshold: number;
}

function draw(
  surface: HTMLCanvasElement | null,
  nodes: Node[],
  edges: Edge[],
  transform: { x: number; y: number; k: number },
  options: DrawOptions,
): void {
  if (!surface) return;
  const context = surface.getContext('2d');
  if (!context) return;

  const ratio = window.devicePixelRatio || 1;
  const styles = getComputedStyle(document.documentElement);
  const colour = (name: string) => styles.getPropertyValue(name).trim() || '#888';

  context.save();
  context.clearRect(0, 0, surface.width, surface.height);
  context.scale(ratio, ratio);
  context.translate(
    surface.width / ratio / 2 + transform.x,
    surface.height / ratio / 2 + transform.y,
  );
  context.scale(transform.k, transform.k);

  // The neighbours of whatever is hovered, so pointing at a note shows what it
  // connects to.
  const highlighted = new Set<string>();
  if (options.hovered) {
    highlighted.add(options.hovered.id);
    for (const edge of edges) {
      const source = idOf(edge.source);
      const target = idOf(edge.target);
      if (source === options.hovered.id) highlighted.add(target);
      if (target === options.hovered.id) highlighted.add(source);
    }
  }

  context.lineWidth = 1 / transform.k;
  for (const edge of edges) {
    const source = edge.source as Node;
    const target = edge.target as Node;
    if (typeof source !== 'object' || typeof target !== 'object') continue;

    const emphasised =
      highlighted.has(source.id) && highlighted.has(target.id) && options.hovered !== null;
    context.strokeStyle = emphasised ? colour('--graph-highlight') : colour('--graph-edge');
    context.globalAlpha = options.hovered && !emphasised ? 0.25 : 1;
    context.beginPath();
    context.moveTo(source.x ?? 0, source.y ?? 0);
    context.lineTo(target.x ?? 0, target.y ?? 0);
    context.stroke();
  }
  context.globalAlpha = 1;

  for (const node of nodes) {
    const radius = radiusOf(node);
    const isActive = node.path !== null && node.path === options.activePath;
    const isCenter = node.path !== null && node.path === options.centerPath;
    const dimmed =
      (options.matchesFilter !== null && !options.matchesFilter(node)) ||
      (options.hovered !== null && !highlighted.has(node.id));

    context.globalAlpha = dimmed ? 0.2 : 1;
    context.beginPath();
    context.arc(node.x ?? 0, node.y ?? 0, radius, 0, Math.PI * 2);
    context.fillStyle =
      isActive || isCenter
        ? colour('--graph-highlight')
        : node.kind === 'unresolved'
          ? colour('--graph-node-unresolved')
          : node.kind === 'attachment'
            ? colour('--graph-node-attachment')
            : node.kind === 'tag'
              ? colour('--graph-node-tag')
              : colour('--graph-node');
    context.fill();

    if (isActive || isCenter) {
      context.lineWidth = 2 / transform.k;
      context.strokeStyle = colour('--text-normal');
      context.stroke();
    }

    // Labels only once they would be legible, and only for nodes worth naming
    // at this zoom, so a dense graph does not become a wall of text.
    const showLabel =
      !dimmed &&
      (transform.k > options.labelThreshold || isActive || isCenter || options.hovered === node);
    if (showLabel) {
      context.globalAlpha = dimmed ? 0.2 : 1;
      context.fillStyle = colour('--text-muted');
      context.font = `${11 / transform.k}px var(--font-interface)`;
      context.textAlign = 'center';
      context.textBaseline = 'top';
      context.fillText(truncate(node.label, 28), node.x ?? 0, (node.y ?? 0) + radius + 3 / transform.k);
    }
  }

  context.restore();
}

function idOf(endpoint: string | Node): string {
  return typeof endpoint === 'string' ? endpoint : endpoint.id;
}

function truncate(text: string, maximum: number): string {
  return text.length > maximum ? `${text.slice(0, maximum - 1)}…` : text;
}
