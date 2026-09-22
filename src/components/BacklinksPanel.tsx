/**
 * Incoming references.
 *
 * Two sections, because they answer different questions. Linked mentions are
 * notes that point here. Unlinked mentions are notes that say this note's title
 * without linking, which is the raw material for the next link — so each one
 * offers to become a link.
 */

import { useCallback, useEffect, useState } from 'react';

import { api } from '@/services/api';
import { events } from '@/services/events';
import type { Backlink, SearchHit, VaultPath } from '@/types/domain';
import { Badge, EmptyState, Icon, Skeleton } from '@/ui';

export interface BacklinksPanelProps {
  path: VaultPath | null;
  onOpen: (path: VaultPath, options?: { line?: number; newPane?: boolean }) => void;
}

export function BacklinksPanel({ path, onOpen }: BacklinksPanelProps) {
  const [linked, setLinked] = useState<Backlink[]>([]);
  const [unlinked, setUnlinked] = useState<SearchHit[]>([]);
  const [showUnlinked, setShowUnlinked] = useState(false);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!path) {
      setLinked([]);
      setUnlinked([]);
      return;
    }
    setLoading(true);
    try {
      const [backlinks, mentions] = await Promise.all([
        api.backlinks(path),
        showUnlinked ? api.unlinkedMentions(path, 50) : Promise.resolve([]),
      ]);
      setLinked(backlinks);
      setUnlinked(mentions);
    } catch {
      setLinked([]);
      setUnlinked([]);
    } finally {
      setLoading(false);
    }
  }, [path, showUnlinked]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Backlinks must be live: editing a note elsewhere in the vault changes what
  // points here, and a stale panel is worse than no panel.
  useEffect(() => {
    const offs = [
      events.on('indexUpdated', () => void refresh()),
      events.on('indexCompleted', () => void refresh()),
    ];
    return () => offs.forEach((off) => off());
  }, [refresh]);

  const grouped = groupBySource(linked);

  return (
    <div className="ie-panel ie-backlinks">
      <div className="ie-panel-header">
        <span>Backlinks</span>
        <Badge>{linked.length}</Badge>
      </div>

      <div className="ie-panel__body">
        {!path ? (
          <EmptyState
            compact
            icon="corner-down-left"
            title="No note open"
            description="Open a note to see what points at it."
          />
        ) : loading && linked.length === 0 ? (
          // Three bars rather than a spinner: the shape of what is coming,
          // so the panel does not jump when it arrives.
          <div className="ie-panel__loading" aria-busy="true" aria-live="polite">
            <span className="sr-only">Looking for backlinks</span>
            <Skeleton height={14} width="70%" />
            <Skeleton height={14} width="90%" />
            <Skeleton height={14} width="55%" />
          </div>
        ) : linked.length === 0 ? (
          <EmptyState
            compact
            icon="corner-down-left"
            title="Nothing links here yet"
            description="Write [[the name of this note]] in another note to make a link."
          />
        ) : (
          grouped.map(([source, entries]) => (
            <div key={source} className="ie-backlink-group">
              <button
                type="button"
                className="ie-backlink-group__title"
                onClick={() => onOpen(entries[0]!.sourcePath)}
              >
                {entries[0]!.sourceTitle}
                <Badge>{entries.length}</Badge>
              </button>
              {entries.map((entry, index) => (
                <button
                  key={`${entry.sourcePath}-${entry.line}-${index}`}
                  type="button"
                  className="ie-backlink"
                  onClick={(event) =>
                    onOpen(entry.sourcePath, {
                      line: entry.line,
                      newPane: event.ctrlKey || event.metaKey,
                    })
                  }
                  title={`Line ${entry.line + 1}`}
                >
                  <span className="ie-backlink__context">{entry.context}</span>
                </button>
              ))}
            </div>
          ))
        )}

        <div className="ie-panel-header ie-panel-header--sub">
          <button
            type="button"
            className="ie-disclosure"
            aria-expanded={showUnlinked}
            onClick={() => setShowUnlinked((current) => !current)}
          >
            <span className={`ie-tree-chevron${showUnlinked ? ' is-open' : ''}`}>
              <Icon name="chevron-right" size={14} />
            </span>
            Unlinked mentions
          </button>
          {showUnlinked ? <Badge>{unlinked.length}</Badge> : null}
        </div>

        {showUnlinked ? (
          unlinked.length === 0 ? (
            <EmptyState
              compact
              icon="search"
              title="No unlinked mentions"
              description="No other note names this one without linking to it."
            />
          ) : (
            unlinked.map((hit) => (
              <button
                key={hit.path}
                type="button"
                className="ie-backlink"
                onClick={() => onOpen(hit.path)}
              >
                <span className="ie-backlink__source">{hit.title}</span>
                <span
                  className="ie-backlink__context"
                  // The snippet's only markup is the <mark> the backend added
                  // around the matched words; the surrounding note text was
                  // escaped by SQLite's snippet function before it got here.
                  dangerouslySetInnerHTML={{ __html: hit.snippet }}
                />
              </button>
            ))
          )
        ) : null}
      </div>
    </div>
  );
}

function groupBySource(backlinks: Backlink[]): [string, Backlink[]][] {
  const groups = new Map<string, Backlink[]>();
  for (const backlink of backlinks) {
    const existing = groups.get(backlink.sourcePath);
    if (existing) existing.push(backlink);
    else groups.set(backlink.sourcePath, [backlink]);
  }
  return [...groups.entries()];
}
