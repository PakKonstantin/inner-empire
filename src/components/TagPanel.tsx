/**
 * The tag explorer.
 *
 * Nested tags form a tree: `#AI/LLM` sits under `#AI`, and a parent's count
 * includes everything beneath it, so the numbers add up the way a reader
 * expects.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';

import { api } from '@/services/api';
import { events } from '@/services/events';
import type { TagSummary } from '@/types/domain';

interface TagNode {
  name: string;
  segment: string;
  depth: number;
  count: number;
  totalCount: number;
  children: TagNode[];
}

export interface TagPanelProps {
  onSelectTag: (tag: string) => void;
}

export function TagPanel({ onSelectTag }: TagPanelProps) {
  const [tags, setTags] = useState<TagSummary[]>([]);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const [filter, setFilter] = useState('');

  const refresh = useCallback(async () => {
    try {
      setTags(await api.allTags());
    } catch {
      setTags([]);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const offs = [
      events.on('indexCompleted', () => void refresh()),
      events.on('indexUpdated', () => void refresh()),
    ];
    return () => offs.forEach((off) => off());
  }, [refresh]);

  const roots = useMemo(() => buildTree(tags), [tags]);
  const visible = useMemo(() => flatten(roots, collapsed, filter.trim().toLowerCase()), [
    roots,
    collapsed,
    filter,
  ]);

  return (
    <div className="ie-panel ie-tags">
      <div className="ie-panel-header">
        <span>Tags</span>
        <span className="ie-count-badge">{tags.length}</span>
      </div>

      <div className="ie-explorer__filter">
        <input
          className="ie-input"
          type="search"
          placeholder="Filter tags"
          value={filter}
          onChange={(event) => setFilter(event.target.value)}
        />
      </div>

      <div className="ie-panel__body">
        {visible.length === 0 ? (
          <div className="ie-empty">
            {tags.length === 0 ? 'No tags in this vault yet.' : 'Nothing matches that filter.'}
          </div>
        ) : (
          visible.map((node) => {
            const isCollapsed = collapsed.has(node.name);
            return (
              <div
                key={node.name}
                className="ie-tag-row"
                style={{ paddingLeft: `${8 + node.depth * 14}px` }}
              >
                {node.children.length > 0 ? (
                  <button
                    type="button"
                    className={`ie-tree-chevron${isCollapsed ? '' : ' is-open'}`}
                    aria-label={isCollapsed ? `Expand ${node.name}` : `Collapse ${node.name}`}
                    onClick={() =>
                      setCollapsed((current) => {
                        const next = new Set(current);
                        if (next.has(node.name)) next.delete(node.name);
                        else next.add(node.name);
                        return next;
                      })
                    }
                  >
                    ▸
                  </button>
                ) : (
                  <span className="ie-tree-chevron ie-tree-chevron--placeholder" />
                )}
                <button
                  type="button"
                  className="ie-tag-row__name"
                  onClick={() => onSelectTag(node.name)}
                  title={`Search for #${node.name}`}
                >
                  {node.segment}
                </button>
                <span className="ie-count-badge">{node.totalCount}</span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

/** Build the nesting from the flat list the backend returns. */
function buildTree(tags: TagSummary[]): TagNode[] {
  const byName = new Map<string, TagNode>();
  const roots: TagNode[] = [];

  const ensure = (name: string, depth: number): TagNode => {
    const existing = byName.get(name);
    if (existing) return existing;

    const segments = name.split('/');
    const node: TagNode = {
      name,
      segment: segments[segments.length - 1] ?? name,
      depth,
      count: 0,
      totalCount: 0,
      children: [],
    };
    byName.set(name, node);

    if (segments.length === 1) {
      roots.push(node);
    } else {
      // A tag may exist without its parent ever being used on its own, so the
      // parent is created as an empty container rather than dropped.
      const parentName = segments.slice(0, -1).join('/');
      ensure(parentName, depth - 1).children.push(node);
    }
    return node;
  };

  for (const tag of [...tags].sort((a, b) => a.name.localeCompare(b.name))) {
    const node = ensure(tag.name, tag.name.split('/').length - 1);
    node.count = tag.count;
    node.totalCount = tag.totalCount;
  }

  return roots;
}

function flatten(nodes: TagNode[], collapsed: Set<string>, filter: string): TagNode[] {
  const out: TagNode[] = [];
  const walk = (list: TagNode[]): void => {
    for (const node of list) {
      const matches = !filter || node.name.toLowerCase().includes(filter);
      const childMatches = filter ? hasMatch(node.children, filter) : false;
      if (matches || childMatches) out.push(node);
      // A filter expands everything it matches, so a hit is never hidden
      // behind a collapsed parent.
      if ((!collapsed.has(node.name) || filter) && node.children.length > 0) {
        walk(node.children);
      }
    }
  };
  walk(nodes);
  return out;
}

function hasMatch(nodes: TagNode[], filter: string): boolean {
  return nodes.some(
    (node) => node.name.toLowerCase().includes(filter) || hasMatch(node.children, filter),
  );
}
