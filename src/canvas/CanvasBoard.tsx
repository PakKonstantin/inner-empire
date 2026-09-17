/**
 * The canvas board.
 *
 * Cards are DOM elements rather than a canvas drawing, because a card holds
 * editable text and a rendered note, and those want to be real elements. The
 * edges behind them are SVG. At the sizes a board reaches — tens of cards, not
 * thousands — this is the right trade, and it is why a text card can simply be
 * a textarea.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useContextMenu, type MenuEntry } from '@/components/ContextMenu';
import { notify } from '@/components/Notifications';
import { renderMarkdown } from '@/markdown/renderer';
import { api } from '@/services/api';
import type { VaultPath } from '@/types/domain';
import { pathStem } from '@/types/domain';

import {
  anchorOf,
  boundingBox,
  colorVariable,
  DEFAULT_GROUP_SIZE,
  DEFAULT_NODE_SIZE,
  EMPTY_CANVAS,
  groupContaining,
  inferSides,
  newId,
  nodesInRectangle,
  parseCanvas,
  serializeCanvas,
  type CanvasDocument,
  type CanvasEdge,
  type CanvasNode,
  type CanvasSide,
} from './model';

const GRID = 8;
const SAVE_DELAY_MS = 600;

export interface CanvasBoardProps {
  path: VaultPath;
  onOpenNote: (path: VaultPath, options?: { newPane?: boolean }) => void;
  assetUrl: (target: string) => string | null;
}

interface Viewport {
  x: number;
  y: number;
  k: number;
}

type Interaction =
  | { kind: 'none' }
  | { kind: 'pan'; startX: number; startY: number; origin: Viewport }
  | { kind: 'move'; ids: string[]; startX: number; startY: number; origins: Map<string, { x: number; y: number }> }
  | { kind: 'resize'; id: string; startX: number; startY: number; origin: { width: number; height: number } }
  | { kind: 'select'; startX: number; startY: number; currentX: number; currentY: number }
  | { kind: 'connect'; fromId: string; fromSide: CanvasSide; toX: number; toY: number };

export function CanvasBoard({ path, onOpenNote, assetUrl }: CanvasBoardProps) {
  const [document, setDocument] = useState<CanvasDocument>(EMPTY_CANVAS);
  const [selection, setSelection] = useState<Set<string>>(() => new Set());
  const [editing, setEditing] = useState<string | null>(null);
  const [viewport, setViewport] = useState<Viewport>({ x: 0, y: 0, k: 1 });
  const [interaction, setInteraction] = useState<Interaction>({ kind: 'none' });
  const [loaded, setLoaded] = useState(false);

  const surface = useRef<HTMLDivElement | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const menu = useContextMenu();

  useEffect(() => {
    let cancelled = false;
    api
      .readNote(path)
      .then((note) => {
        if (cancelled) return;
        setDocument(parseCanvas(note.content));
        setLoaded(true);
      })
      .catch(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, [path]);

  /** Write the board, debounced, the same way a note is written. */
  const persist = useCallback(
    (next: CanvasDocument) => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      saveTimer.current = setTimeout(() => {
        saveTimer.current = null;
        api.saveNote(path, serializeCanvas(next)).catch((error) => {
          notify('error', `Could not save the canvas: ${message(error)}`);
        });
      }, SAVE_DELAY_MS);
    },
    [path],
  );

  const update = useCallback(
    (change: (current: CanvasDocument) => CanvasDocument) => {
      setDocument((current) => {
        const next = change(current);
        persist(next);
        return next;
      });
    },
    [persist],
  );

  // Flush on unmount, so switching away from a canvas does not lose the last
  // move.
  useEffect(
    () => () => {
      if (saveTimer.current) {
        clearTimeout(saveTimer.current);
        saveTimer.current = null;
      }
    },
    [],
  );

  const toBoard = useCallback(
    (clientX: number, clientY: number) => {
      const rect = surface.current?.getBoundingClientRect();
      if (!rect) return { x: 0, y: 0 };
      return {
        x: (clientX - rect.left - viewport.x) / viewport.k,
        y: (clientY - rect.top - viewport.y) / viewport.k,
      };
    },
    [viewport],
  );

  const addNode = useCallback(
    (node: CanvasNode) => {
      update((current) => ({ ...current, nodes: [...current.nodes, node] }));
      setSelection(new Set([node.id]));
      if (node.type === 'text') setEditing(node.id);
    },
    [update],
  );

  const addAt = useCallback(
    (clientX: number, clientY: number, type: 'text' | 'group') => {
      const point = toBoard(clientX, clientY);
      const size = type === 'group' ? DEFAULT_GROUP_SIZE : DEFAULT_NODE_SIZE;
      addNode(
        type === 'group'
          ? { id: newId('g'), type: 'group', label: 'Group', ...snapPoint(point), ...size }
          : { id: newId('n'), type: 'text', text: '', ...snapPoint(point), ...size },
      );
    },
    [addNode, toBoard],
  );

  const removeSelected = useCallback(() => {
    if (selection.size === 0) return;
    update((current) => ({
      nodes: current.nodes.filter((node) => !selection.has(node.id)),
      // An edge with a missing endpoint would be invisible and confusing, so
      // it goes with the card.
      edges: current.edges.filter(
        (edge) => !selection.has(edge.fromNode) && !selection.has(edge.toNode),
      ),
    }));
    setSelection(new Set());
  }, [selection, update]);

  const setColor = useCallback(
    (color: number | undefined) => {
      update((current) => ({
        ...current,
        nodes: current.nodes.map((node) =>
          selection.has(node.id) ? ({ ...node, color } as CanvasNode) : node,
        ),
      }));
    },
    [selection, update],
  );

  // Keyboard: delete, select all, escape, and nudging with the arrows.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (editing !== null) return;
      const target = event.target;
      if (target instanceof HTMLElement && ['INPUT', 'TEXTAREA'].includes(target.tagName)) return;
      if (!surface.current?.contains(globalThis.document.activeElement)) return;

      if (event.key === 'Delete' || event.key === 'Backspace') {
        event.preventDefault();
        removeSelected();
      } else if (event.key === 'Escape') {
        setSelection(new Set());
      } else if (event.key === 'a' && (event.ctrlKey || event.metaKey)) {
        event.preventDefault();
        setSelection(new Set(document.nodes.map((node) => node.id)));
      } else if (event.key.startsWith('Arrow') && selection.size > 0) {
        event.preventDefault();
        const step = event.shiftKey ? GRID * 4 : GRID;
        const dx = event.key === 'ArrowRight' ? step : event.key === 'ArrowLeft' ? -step : 0;
        const dy = event.key === 'ArrowDown' ? step : event.key === 'ArrowUp' ? -step : 0;
        update((current) => ({
          ...current,
          nodes: current.nodes.map((node) =>
            selection.has(node.id) ? { ...node, x: node.x + dx, y: node.y + dy } : node,
          ),
        }));
      }
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [document.nodes, editing, removeSelected, selection, update]);

  const onPointerMove = useCallback(
    (event: React.PointerEvent) => {
      switch (interaction.kind) {
        case 'pan':
          setViewport({
            ...interaction.origin,
            x: interaction.origin.x + (event.clientX - interaction.startX),
            y: interaction.origin.y + (event.clientY - interaction.startY),
          });
          break;

        case 'move': {
          const dx = (event.clientX - interaction.startX) / viewport.k;
          const dy = (event.clientY - interaction.startY) / viewport.k;
          setDocument((current) => ({
            ...current,
            nodes: current.nodes.map((node) => {
              const origin = interaction.origins.get(node.id);
              if (!origin) return node;
              return { ...node, x: snap(origin.x + dx), y: snap(origin.y + dy) };
            }),
          }));
          break;
        }

        case 'resize': {
          const dx = (event.clientX - interaction.startX) / viewport.k;
          const dy = (event.clientY - interaction.startY) / viewport.k;
          setDocument((current) => ({
            ...current,
            nodes: current.nodes.map((node) =>
              node.id === interaction.id
                ? {
                    ...node,
                    width: Math.max(80, snap(interaction.origin.width + dx)),
                    height: Math.max(60, snap(interaction.origin.height + dy)),
                  }
                : node,
            ),
          }));
          break;
        }

        case 'select':
          setInteraction({ ...interaction, currentX: event.clientX, currentY: event.clientY });
          break;

        case 'connect': {
          const point = toBoard(event.clientX, event.clientY);
          setInteraction({ ...interaction, toX: point.x, toY: point.y });
          break;
        }

        default:
          break;
      }
    },
    [interaction, toBoard, viewport.k],
  );

  const onPointerUp = useCallback(
    (event: React.PointerEvent) => {
      switch (interaction.kind) {
        case 'move':
        case 'resize':
          // Persist once at the end of the gesture rather than on every frame.
          persist(document);
          break;

        case 'select': {
          const from = toBoard(interaction.startX, interaction.startY);
          const to = toBoard(event.clientX, event.clientY);
          const inside = nodesInRectangle(document.nodes, {
            x: from.x,
            y: from.y,
            width: to.x - from.x,
            height: to.y - from.y,
          });
          setSelection(new Set(inside.map((node) => node.id)));
          break;
        }

        case 'connect': {
          const point = toBoard(event.clientX, event.clientY);
          const target = document.nodes.find(
            (node) =>
              node.id !== interaction.fromId &&
              point.x >= node.x &&
              point.x <= node.x + node.width &&
              point.y >= node.y &&
              point.y <= node.y + node.height,
          );
          if (target) {
            const source = document.nodes.find((node) => node.id === interaction.fromId);
            const [, toSide] = source ? inferSides(source, target) : ['right', 'left'];
            const edge: CanvasEdge = {
              id: newId('e'),
              fromNode: interaction.fromId,
              fromSide: interaction.fromSide,
              toNode: target.id,
              toSide: toSide as CanvasSide,
              toEnd: 'arrow',
            };
            update((current) => ({ ...current, edges: [...current.edges, edge] }));
          }
          break;
        }

        default:
          break;
      }
      setInteraction({ kind: 'none' });
    },
    [document, interaction, persist, toBoard, update],
  );

  const selectionRectangle = useMemo(() => {
    if (interaction.kind !== 'select') return null;
    const rect = surface.current?.getBoundingClientRect();
    if (!rect) return null;
    return {
      left: Math.min(interaction.startX, interaction.currentX) - rect.left,
      top: Math.min(interaction.startY, interaction.currentY) - rect.top,
      width: Math.abs(interaction.currentX - interaction.startX),
      height: Math.abs(interaction.currentY - interaction.startY),
    };
  }, [interaction]);

  if (!loaded) return <div className="ie-pane__loading">Opening the canvas…</div>;

  return (
    <div className="ie-canvas">
      <div className="ie-canvas__toolbar ie-chrome">
        <button
          type="button"
          className="ie-button ie-button--quiet"
          onClick={() =>
            addNode({
              id: newId('n'),
              type: 'text',
              text: '',
              ...snapPoint(centreOf(surface.current, viewport)),
              ...DEFAULT_NODE_SIZE,
            })
          }
        >
          Add card
        </button>
        <button
          type="button"
          className="ie-button ie-button--quiet"
          onClick={() =>
            addNode({
              id: newId('g'),
              type: 'group',
              label: 'Group',
              ...snapPoint(centreOf(surface.current, viewport)),
              ...DEFAULT_GROUP_SIZE,
            })
          }
        >
          Add group
        </button>
        <span className="ie-viewer__spacer" />
        <button
          type="button"
          className="ie-button ie-button--quiet"
          onClick={() => setViewport((current) => ({ ...current, k: current.k / 1.25 }))}
        >
          Zoom out
        </button>
        <span>{Math.round(viewport.k * 100)}%</span>
        <button
          type="button"
          className="ie-button ie-button--quiet"
          onClick={() => setViewport((current) => ({ ...current, k: current.k * 1.25 }))}
        >
          Zoom in
        </button>
        <button
          type="button"
          className="ie-button ie-button--quiet"
          onClick={() => setViewport(fitToContent(document.nodes, surface.current))}
        >
          Fit
        </button>
      </div>

      <div
        className="ie-canvas__surface"
        ref={surface}
        tabIndex={0}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerDown={(event) => {
          if (event.target !== event.currentTarget && !(event.target as HTMLElement).classList.contains('ie-canvas__grid')) {
            return;
          }
          event.currentTarget.focus();
          // Middle button or space-drag pans; a plain drag selects.
          if (event.button === 1 || event.altKey) {
            setInteraction({
              kind: 'pan',
              startX: event.clientX,
              startY: event.clientY,
              origin: viewport,
            });
          } else {
            setSelection(new Set());
            setEditing(null);
            setInteraction({
              kind: 'select',
              startX: event.clientX,
              startY: event.clientY,
              currentX: event.clientX,
              currentY: event.clientY,
            });
          }
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onWheel={(event) => {
          if (event.ctrlKey || event.metaKey) {
            const rect = event.currentTarget.getBoundingClientRect();
            const scale = Math.exp(-event.deltaY * 0.002);
            const next = Math.max(0.15, Math.min(4, viewport.k * scale));
            const px = event.clientX - rect.left;
            const py = event.clientY - rect.top;
            const ratio = next / viewport.k;
            setViewport({
              k: next,
              x: px - (px - viewport.x) * ratio,
              y: py - (py - viewport.y) * ratio,
            });
          } else {
            setViewport((current) => ({
              ...current,
              x: current.x - event.deltaX,
              y: current.y - event.deltaY,
            }));
          }
        }}
        onContextMenu={(event) => {
          if (event.target !== event.currentTarget) return;
          event.preventDefault();
          const entries: MenuEntry[] = [
            { id: 'card', label: 'Add a card here', run: () => addAt(event.clientX, event.clientY, 'text') },
            { id: 'group', label: 'Add a group here', run: () => addAt(event.clientX, event.clientY, 'group') },
            { id: 'sep', separator: true },
            {
              id: 'fit',
              label: 'Fit everything on screen',
              run: () => setViewport(fitToContent(document.nodes, surface.current)),
            },
          ];
          menu.open(event, entries);
        }}
        onDragOver={(event) => event.preventDefault()}
        onDrop={(event) => {
          // A note dragged from the explorer becomes a card showing that note.
          const dropped = event.dataTransfer.getData('text/plain');
          if (!dropped) return;
          event.preventDefault();
          const point = toBoard(event.clientX, event.clientY);
          addNode({
            id: newId('n'),
            type: 'file',
            file: dropped as VaultPath,
            ...snapPoint(point),
            ...DEFAULT_NODE_SIZE,
          });
        }}
      >
        <div
          className="ie-canvas__grid"
          style={{
            backgroundSize: `${GRID * 4 * viewport.k}px ${GRID * 4 * viewport.k}px`,
            backgroundPosition: `${viewport.x}px ${viewport.y}px`,
          }}
        />

        <div
          className="ie-canvas__world"
          style={{
            transform: `translate(${viewport.x}px, ${viewport.y}px) scale(${viewport.k})`,
          }}
        >
          <Edges
            document={document}
            pending={interaction.kind === 'connect' ? interaction : null}
            onRemove={(id) =>
              update((current) => ({
                ...current,
                edges: current.edges.filter((edge) => edge.id !== id),
              }))
            }
          />

          {/* Groups render first so cards sit above them. */}
          {[...document.nodes]
            .sort((a, b) => (a.type === 'group' ? -1 : 0) - (b.type === 'group' ? -1 : 0))
            .map((node) => (
              <Card
                key={node.id}
                node={node}
                selected={selection.has(node.id)}
                editing={editing === node.id}
                assetUrl={assetUrl}
                onOpenNote={onOpenNote}
                onStartEdit={() => setEditing(node.id)}
                onStopEdit={() => setEditing(null)}
                onChange={(changed) =>
                  update((current) => ({
                    ...current,
                    nodes: current.nodes.map((candidate) =>
                      candidate.id === changed.id ? changed : candidate,
                    ),
                  }))
                }
                onPointerDownCard={(event) => {
                  event.stopPropagation();
                  const additive = event.shiftKey || event.ctrlKey || event.metaKey;
                  const nextSelection = additive
                    ? new Set(selection).add(node.id)
                    : selection.has(node.id)
                      ? selection
                      : new Set([node.id]);
                  setSelection(nextSelection);

                  // Moving a group moves what it contains, which is what makes
                  // a group useful rather than decorative.
                  const moving = new Set(nextSelection);
                  for (const id of nextSelection) {
                    const candidate = document.nodes.find((n) => n.id === id);
                    if (candidate?.type !== 'group') continue;
                    for (const member of document.nodes) {
                      if (groupContaining(member, document.nodes)?.id === candidate.id) {
                        moving.add(member.id);
                      }
                    }
                  }

                  setInteraction({
                    kind: 'move',
                    ids: [...moving],
                    startX: event.clientX,
                    startY: event.clientY,
                    origins: new Map(
                      document.nodes
                        .filter((candidate) => moving.has(candidate.id))
                        .map((candidate) => [candidate.id, { x: candidate.x, y: candidate.y }]),
                    ),
                  });
                }}
                onStartResize={(event) => {
                  event.stopPropagation();
                  setInteraction({
                    kind: 'resize',
                    id: node.id,
                    startX: event.clientX,
                    startY: event.clientY,
                    origin: { width: node.width, height: node.height },
                  });
                }}
                onStartConnect={(side, event) => {
                  event.stopPropagation();
                  const point = toBoard(event.clientX, event.clientY);
                  setInteraction({
                    kind: 'connect',
                    fromId: node.id,
                    fromSide: side,
                    toX: point.x,
                    toY: point.y,
                  });
                }}
                onContextMenu={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  setSelection(new Set([node.id]));
                  menu.open(event, [
                    ...(node.type === 'file'
                      ? [
                          {
                            id: 'open',
                            label: 'Open this note',
                            run: () => onOpenNote(node.file),
                          } as MenuEntry,
                        ]
                      : []),
                    { id: 'colour-none', label: 'No colour', run: () => setColor(undefined) },
                    { id: 'colour-0', label: 'Accent', run: () => setColor(0) },
                    { id: 'colour-1', label: 'Red', run: () => setColor(1) },
                    { id: 'colour-2', label: 'Amber', run: () => setColor(2) },
                    { id: 'colour-3', label: 'Green', run: () => setColor(3) },
                    { id: 'sep', separator: true },
                    { id: 'delete', label: 'Delete', danger: true, run: removeSelected },
                  ]);
                }}
              />
            ))}
        </div>

        {selectionRectangle ? (
          <div className="ie-canvas__marquee" style={selectionRectangle} />
        ) : null}
      </div>
    </div>
  );
}

interface CardProps {
  node: CanvasNode;
  selected: boolean;
  editing: boolean;
  assetUrl: (target: string) => string | null;
  onOpenNote: (path: VaultPath, options?: { newPane?: boolean }) => void;
  onStartEdit: () => void;
  onStopEdit: () => void;
  onChange: (node: CanvasNode) => void;
  onPointerDownCard: (event: React.PointerEvent) => void;
  onStartResize: (event: React.PointerEvent) => void;
  onStartConnect: (side: CanvasSide, event: React.PointerEvent) => void;
  onContextMenu: (event: React.MouseEvent) => void;
}

function Card(props: CardProps) {
  const { node } = props;
  const accent = colorVariable(node.color);

  return (
    <div
      className={[
        'ie-card',
        `ie-card--${node.type}`,
        props.selected ? 'is-selected' : '',
      ]
        .filter(Boolean)
        .join(' ')}
      style={{
        left: node.x,
        top: node.y,
        width: node.width,
        height: node.height,
        borderColor: node.color !== undefined ? accent : undefined,
      }}
      onPointerDown={props.onPointerDownCard}
      onContextMenu={props.onContextMenu}
      onDoubleClick={() => {
        if (node.type === 'text' || node.type === 'group') props.onStartEdit();
        if (node.type === 'file') props.onOpenNote(node.file);
      }}
    >
      <CardBody {...props} />

      {props.selected ? (
        <>
          {(['top', 'right', 'bottom', 'left'] as CanvasSide[]).map((side) => (
            <button
              key={side}
              type="button"
              className={`ie-card__port ie-card__port--${side}`}
              aria-label={`Draw a connection from the ${side}`}
              onPointerDown={(event) => props.onStartConnect(side, event)}
            />
          ))}
          <div
            className="ie-card__resize"
            role="separator"
            aria-label="Resize"
            onPointerDown={props.onStartResize}
          />
        </>
      ) : null}
    </div>
  );
}

function CardBody(props: CardProps) {
  const { node } = props;

  switch (node.type) {
    case 'text':
      return props.editing ? (
        <textarea
          className="ie-card__editor"
          autoFocus
          value={node.text}
          onChange={(event) => props.onChange({ ...node, text: event.target.value })}
          onBlur={props.onStopEdit}
          onPointerDown={(event) => event.stopPropagation()}
          onKeyDown={(event) => {
            if (event.key === 'Escape') props.onStopEdit();
          }}
        />
      ) : (
        <div className="ie-card__text">
          {node.text ? (
            renderMarkdown(node.text, {
              path: '' as VaultPath,
              onFollowLink: (target) => props.onOpenNote(target as VaultPath),
              onFollowTag: () => {},
              onFollowExternal: (url) => void api.openExternal(url),
              resolveAsset: props.assetUrl,
            })
          ) : (
            <span className="ie-card__placeholder">Double-click to write</span>
          )}
        </div>
      );

    case 'file':
      return <FileCard node={node} onOpenNote={props.onOpenNote} assetUrl={props.assetUrl} />;

    case 'link':
      return (
        <a
          className="ie-card__link"
          href={node.url}
          onClick={(event) => {
            event.preventDefault();
            void api.openExternal(node.url);
          }}
        >
          {node.url}
        </a>
      );

    default:
      return props.editing ? (
        <input
          className="ie-card__group-label"
          autoFocus
          value={node.label ?? ''}
          onChange={(event) => props.onChange({ ...node, label: event.target.value })}
          onBlur={props.onStopEdit}
          onPointerDown={(event) => event.stopPropagation()}
          onKeyDown={(event) => {
            if (event.key === 'Enter' || event.key === 'Escape') props.onStopEdit();
          }}
        />
      ) : (
        <span className="ie-card__group-label">{node.label || 'Group'}</span>
      );
  }
}

function FileCard({
  node,
  onOpenNote,
  assetUrl,
}: {
  node: Extract<CanvasNode, { type: 'file' }>;
  onOpenNote: (path: VaultPath) => void;
  assetUrl: (target: string) => string | null;
}) {
  const [content, setContent] = useState<string | null>(null);
  const isImage = /\.(png|jpe?g|gif|webp|svg|bmp|avif)$/i.test(node.file);

  useEffect(() => {
    if (isImage) return;
    let cancelled = false;
    api
      .readNote(node.file)
      .then((note) => {
        if (!cancelled) setContent(note.content);
      })
      .catch(() => {
        if (!cancelled) setContent(null);
      });
    return () => {
      cancelled = true;
    };
  }, [node.file, isImage]);

  if (isImage) {
    const source = assetUrl(node.file);
    return source ? (
      <img className="ie-card__image" src={source} alt={node.file} />
    ) : (
      <div className="ie-card__missing">{node.file}</div>
    );
  }

  return (
    <>
      <button
        type="button"
        className="ie-card__title"
        onClick={() => onOpenNote(node.file)}
        onPointerDown={(event) => event.stopPropagation()}
      >
        {pathStem(node.file)}
      </button>
      <div className="ie-card__preview">
        {content === null ? (
          <span className="ie-card__placeholder">This note could not be read.</span>
        ) : (
          renderMarkdown(content, {
            path: node.file,
            onFollowLink: (target) => onOpenNote(target as VaultPath),
            onFollowTag: () => {},
            onFollowExternal: (url) => void api.openExternal(url),
            resolveAsset: assetUrl,
          })
        )}
      </div>
    </>
  );
}

function Edges({
  document,
  pending,
  onRemove,
}: {
  document: CanvasDocument;
  pending: { fromId: string; fromSide: CanvasSide; toX: number; toY: number } | null;
  onRemove: (id: string) => void;
}) {
  const byId = useMemo(
    () => new Map(document.nodes.map((node) => [node.id, node])),
    [document.nodes],
  );

  // The SVG covers a fixed large area rather than being sized to content, so a
  // card dragged beyond the previous bounds still has its edges drawn.
  return (
    <svg className="ie-canvas__edges" viewBox="-10000 -10000 20000 20000" width="20000" height="20000">
      <defs>
        <marker
          id="ie-arrow"
          viewBox="0 0 10 10"
          refX="9"
          refY="5"
          markerWidth="6"
          markerHeight="6"
          orient="auto-start-reverse"
        >
          <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--border-strong)" />
        </marker>
      </defs>

      {document.edges.map((edge) => {
        const from = byId.get(edge.fromNode);
        const to = byId.get(edge.toNode);
        if (!from || !to) return null;

        const [defaultFrom, defaultTo] = inferSides(from, to);
        const start = anchorOf(from, edge.fromSide ?? defaultFrom);
        const end = anchorOf(to, edge.toSide ?? defaultTo);

        return (
          <g key={edge.id} className="ie-canvas__edge">
            <path
              d={curve(start, end)}
              stroke={edge.color !== undefined ? colorVariable(edge.color) : 'var(--border-strong)'}
              fill="none"
              strokeWidth={2}
              markerEnd={edge.toEnd === 'none' ? undefined : 'url(#ie-arrow)'}
            />
            {/* A wide transparent path on top, so the line is easy to click
                without being thick. */}
            <path
              d={curve(start, end)}
              stroke="transparent"
              fill="none"
              strokeWidth={14}
              style={{ cursor: 'pointer' }}
              onDoubleClick={() => onRemove(edge.id)}
            >
              <title>Double-click to remove this connection</title>
            </path>
            {edge.label ? (
              <text
                x={(start.x + end.x) / 2}
                y={(start.y + end.y) / 2 - 6}
                textAnchor="middle"
                fill="var(--text-muted)"
                fontSize={12}
              >
                {edge.label}
              </text>
            ) : null}
          </g>
        );
      })}

      {pending
        ? (() => {
            const from = byId.get(pending.fromId);
            if (!from) return null;
            const start = anchorOf(from, pending.fromSide);
            return (
              <path
                d={curve(start, { x: pending.toX, y: pending.toY })}
                stroke="var(--accent)"
                strokeDasharray="6 4"
                fill="none"
                strokeWidth={2}
              />
            );
          })()
        : null}
    </svg>
  );
}

/** A cubic curve that leaves and arrives horizontally, like a flow diagram. */
function curve(from: { x: number; y: number }, to: { x: number; y: number }): string {
  const dx = Math.abs(to.x - from.x);
  const offset = Math.max(40, Math.min(dx * 0.6, 180));
  return `M ${from.x} ${from.y} C ${from.x + offset} ${from.y}, ${to.x - offset} ${to.y}, ${to.x} ${to.y}`;
}

function snap(value: number): number {
  return Math.round(value / GRID) * GRID;
}

function snapPoint(point: { x: number; y: number }): { x: number; y: number } {
  return { x: snap(point.x), y: snap(point.y) };
}

function centreOf(element: HTMLElement | null, viewport: Viewport): { x: number; y: number } {
  if (!element) return { x: 0, y: 0 };
  return {
    x: (element.clientWidth / 2 - viewport.x) / viewport.k - DEFAULT_NODE_SIZE.width / 2,
    y: (element.clientHeight / 2 - viewport.y) / viewport.k - DEFAULT_NODE_SIZE.height / 2,
  };
}

function fitToContent(nodes: CanvasNode[], element: HTMLElement | null): Viewport {
  const box = boundingBox(nodes);
  if (!box || !element) return { x: 0, y: 0, k: 1 };

  const padding = 60;
  const scaleX = (element.clientWidth - padding * 2) / Math.max(box.width, 1);
  const scaleY = (element.clientHeight - padding * 2) / Math.max(box.height, 1);
  const k = Math.max(0.15, Math.min(1.5, Math.min(scaleX, scaleY)));

  return {
    k,
    x: element.clientWidth / 2 - (box.x + box.width / 2) * k,
    y: element.clientHeight / 2 - (box.y + box.height / 2) * k,
  };
}

function message(error: unknown): string {
  return error instanceof Object && 'message' in error ? String(error.message) : String(error);
}
