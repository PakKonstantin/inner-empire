/**
 * A windowed list.
 *
 * The explorer, the search results and the backlinks panel all render lists
 * that can hold tens of thousands of rows. Rendering them all would cost a DOM
 * node per note; this renders only what fits on screen plus a small overscan,
 * so the cost is constant however large the vault is.
 *
 * Rows are a fixed height, which is what lets the visible window be computed
 * arithmetically rather than measured.
 */

import type { ReactNode } from 'react';
import { useCallback, useEffect, useRef, useState } from 'react';

export interface VirtualListProps<T> {
  items: T[];
  rowHeight: number;
  renderRow: (item: T, index: number) => ReactNode;
  keyOf: (item: T, index: number) => string;
  /** Extra rows above and below, so a fast scroll does not show blank space. */
  overscan?: number;
  className?: string;
  emptyState?: ReactNode;
  /** Scroll this index into view when it changes. */
  scrollToIndex?: number;
}

export function VirtualList<T>({
  items,
  rowHeight,
  renderRow,
  keyOf,
  overscan = 8,
  className,
  emptyState,
  scrollToIndex,
}: VirtualListProps<T>) {
  const viewport = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(0);

  useEffect(() => {
    const element = viewport.current;
    if (!element) return;

    setHeight(element.clientHeight);
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) setHeight(entry.contentRect.height);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const element = viewport.current;
    if (!element || scrollToIndex === undefined || scrollToIndex < 0) return;

    const top = scrollToIndex * rowHeight;
    const bottom = top + rowHeight;
    // Only scroll when the row is actually out of view, so arrowing through a
    // list does not jerk it around.
    if (top < element.scrollTop) {
      element.scrollTop = top;
    } else if (bottom > element.scrollTop + element.clientHeight) {
      element.scrollTop = bottom - element.clientHeight;
    }
  }, [scrollToIndex, rowHeight]);

  const onScroll = useCallback((event: React.UIEvent<HTMLDivElement>) => {
    setScrollTop(event.currentTarget.scrollTop);
  }, []);

  if (items.length === 0 && emptyState) {
    return <div className={className}>{emptyState}</div>;
  }

  const first = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
  const visibleCount = Math.ceil((height || 400) / rowHeight) + overscan * 2;
  const last = Math.min(items.length, first + visibleCount);
  const visible = items.slice(first, last);

  return (
    <div className={className} ref={viewport} onScroll={onScroll} style={{ overflowY: 'auto' }}>
      {/* A spacer of the full height gives the scrollbar the right size while
          only the visible rows exist in the DOM. */}
      <div style={{ height: items.length * rowHeight, position: 'relative' }}>
        <div style={{ transform: `translateY(${first * rowHeight}px)` }}>
          {visible.map((item, offset) => (
            <div key={keyOf(item, first + offset)} style={{ height: rowHeight }}>
              {renderRow(item, first + offset)}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
