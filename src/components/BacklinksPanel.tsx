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
        <span className="ie-count-badge">{linked.length}</span>
      </div>

      <div className="ie-panel__body">
        {!path ? (
          <div className="ie-empty">Open a note to see what links to it.</div>
        ) : linked.length === 0 && !loading ? (
          <div className="ie-empty">Nothing links here yet.</div>
        ) : (
          grouped.map(([source, entries]) => (
            <div key={source} className="ie-backlink-group">
              <button
                type="button"
                className="ie-backlink-group__title"
                onClick={() => onOpen(entries[0]!.sourcePath)}
              >
                {entries[0]!.sourceTitle}
                <span className="ie-count-badge">{entries.length}</span>
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
            <span className={`ie-tree-chevron${showUnlinked ? ' is-open' : ''}`}>▸</span>
            Unlinked mentions
          </button>
          {showUnlinked ? <span className="ie-count-badge">{unlinked.length}</span> : null}
        </div>

        {showUnlinked ? (
          unlinked.length === 0 ? (
            <div className="ie-empty">No unlinked mentions.</div>
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
