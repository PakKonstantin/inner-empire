/**
 * The current note's headings.
 *
 * Clicking one moves the cursor there rather than just scrolling, so the
 * outline is a way to navigate the document, not only to look at it.
 */

import { useCallback, useEffect, useState } from 'react';

import { api } from '@/services/api';
import { events } from '@/services/events';
import type { Heading, VaultPath } from '@/types/domain';

export interface OutlinePanelProps {
  path: VaultPath | null;
  /** Line the cursor is on, to highlight the section being edited. */
  currentLine: number;
  onJump: (line: number) => void;
}

export function OutlinePanel({ path, currentLine, onJump }: OutlinePanelProps) {
  const [headings, setHeadings] = useState<Heading[]>([]);

  const refresh = useCallback(async () => {
    if (!path) {
      setHeadings([]);
      return;
    }
    try {
      setHeadings(await api.outline(path));
    } catch {
      setHeadings([]);
    }
  }, [path]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const off = events.on('fileModified', (payload) => {
      if (payload.path === path) void refresh();
    });
    return off;
  }, [path, refresh]);

  // The heading whose section contains the cursor: the last one at or before
  // the current line.
  const activeIndex = headings.reduce(
    (best, heading, index) => (heading.line <= currentLine ? index : best),
    -1,
  );

  // Indent relative to the shallowest heading present, so a note whose top
  // level is H2 is not pushed off to the right.
  const minimumLevel = headings.reduce((min, heading) => Math.min(min, heading.level), 6);

  return (
    <div className="ie-panel ie-outline">
      <div className="ie-panel-header">
        <span>Outline</span>
        <span className="ie-count-badge">{headings.length}</span>
      </div>
      <div className="ie-panel__body">
        {!path ? (
          <div className="ie-empty">Open a note to see its outline.</div>
        ) : headings.length === 0 ? (
          <div className="ie-empty">This note has no headings.</div>
        ) : (
          headings.map((heading, index) => (
            <button
              key={`${heading.line}-${heading.slug}`}
              type="button"
              className={`ie-outline__item${index === activeIndex ? ' is-active' : ''}`}
              style={{ paddingLeft: `${10 + (heading.level - minimumLevel) * 12}px` }}
              onClick={() => onJump(heading.line)}
              title={heading.text}
            >
              {heading.text || '(untitled heading)'}
            </button>
          ))
        )}
      </div>
    </div>
  );
}
