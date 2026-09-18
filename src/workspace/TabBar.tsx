/**
 * The tab strip for one pane.
 *
 * Tabs can be dragged to reorder within a pane and dragged onto another pane
 * to move there. Middle click closes, as it does in a browser. A tab with
 * unsaved changes shows a dot in place of its close button, so the state is
 * visible without hovering.
 */

import { useCallback, useRef, useState } from 'react';

import { useContextMenu, type MenuEntry } from '@/components/ContextMenu';
import type { TabState, VaultPath } from '@/types/domain';
import { pathFileName, pathStem } from '@/types/domain';

export interface TabBarProps {
  paneId: string;
  tabs: TabState[];
  activeTabId: string | null;
  isActivePane: boolean;
  /** Which open files have unsaved changes. */
  dirtyPaths: Set<string>;
  titleOf: (path: VaultPath) => string;
  onSelect: (tabId: string) => void;
  onClose: (tabId: string) => void;
  onCloseOthers: (tabId: string) => void;
  onCloseAll: () => void;
  onTogglePin: (tabId: string) => void;
  onDuplicate: (tabId: string) => void;
  onReorder: (tabId: string, index: number) => void;
  onMoveToPane: (tabId: string, paneId: string, index?: number) => void;
  onSplit: (direction: 'horizontal' | 'vertical') => void;
  onFocusPane: () => void;
}

const TAB_MIME = 'application/x-inner-empire-tab';

export function TabBar(props: TabBarProps) {
  const menu = useContextMenu();
  const [dropIndex, setDropIndex] = useState<number | null>(null);
  const strip = useRef<HTMLDivElement | null>(null);

  const onTabContextMenu = useCallback(
    (event: React.MouseEvent, tab: TabState) => {
      event.preventDefault();
      const entries: MenuEntry[] = [
        { id: 'close', label: 'Close', hint: 'Ctrl+W', run: () => props.onClose(tab.id) },
        { id: 'close-others', label: 'Close others', run: () => props.onCloseOthers(tab.id) },
        { id: 'close-all', label: 'Close all', run: props.onCloseAll },
        { id: 'sep-1', separator: true },
        {
          id: 'pin',
          label: tab.pinned ? 'Unpin' : 'Pin',
          run: () => props.onTogglePin(tab.id),
        },
        { id: 'duplicate', label: 'Duplicate', run: () => props.onDuplicate(tab.id) },
        { id: 'sep-2', separator: true },
        { id: 'split-v', label: 'Split right', run: () => props.onSplit('vertical') },
        { id: 'split-h', label: 'Split down', run: () => props.onSplit('horizontal') },
      ];
      menu.open(event, entries);
    },
    [menu, props],
  );

  return (
    <div
      className={`ie-tabbar ie-chrome${props.isActivePane ? ' is-active-pane' : ''}`}
      ref={strip}
      role="tablist"
      onMouseDown={props.onFocusPane}
      onDragOver={(event) => {
        if (!event.dataTransfer.types.includes(TAB_MIME)) return;
        event.preventDefault();
        setDropIndex(props.tabs.length);
      }}
      onDrop={(event) => {
        const payload = event.dataTransfer.getData(TAB_MIME);
        setDropIndex(null);
        if (!payload) return;
        event.preventDefault();
        const { tabId, paneId } = JSON.parse(payload) as { tabId: string; paneId: string };
        if (paneId === props.paneId) props.onReorder(tabId, props.tabs.length);
        else props.onMoveToPane(tabId, props.paneId);
      }}
    >
      <div className="ie-tabbar__tabs">
        {props.tabs.map((tab, index) => {
          const dirty = props.dirtyPaths.has(tab.path);
          const isActive = tab.id === props.activeTabId;

          return (
            <div
              key={tab.id}
              role="tab"
              aria-selected={isActive}
              tabIndex={isActive ? 0 : -1}
              title={tab.path}
              className={[
                'ie-tab',
                isActive ? 'is-active' : '',
                tab.pinned ? 'is-pinned' : '',
                dirty ? 'is-dirty' : '',
                dropIndex === index ? 'is-drop-before' : '',
              ]
                .filter(Boolean)
                .join(' ')}
              draggable
              onDragStart={(event) => {
                event.dataTransfer.effectAllowed = 'move';
                event.dataTransfer.setData(
                  TAB_MIME,
                  JSON.stringify({ tabId: tab.id, paneId: props.paneId }),
                );
              }}
              onDragOver={(event) => {
                if (!event.dataTransfer.types.includes(TAB_MIME)) return;
                event.preventDefault();
                event.stopPropagation();
                setDropIndex(index);
              }}
              onDragLeave={() => setDropIndex(null)}
              onDrop={(event) => {
                const payload = event.dataTransfer.getData(TAB_MIME);
                setDropIndex(null);
                if (!payload) return;
                event.preventDefault();
                event.stopPropagation();
                const { tabId, paneId } = JSON.parse(payload) as { tabId: string; paneId: string };
                if (paneId === props.paneId) props.onReorder(tabId, index);
                else props.onMoveToPane(tabId, props.paneId, index);
              }}
              onClick={() => props.onSelect(tab.id)}
              onAuxClick={(event) => {
                if (event.button === 1) {
                  event.preventDefault();
                  props.onClose(tab.id);
                }
              }}
              onContextMenu={(event) => onTabContextMenu(event, tab)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') props.onSelect(tab.id);
              }}
            >
              {tab.pinned ? (
                <span className="ie-tab__pin" aria-label="Pinned">
                  ◆
                </span>
              ) : null}
              <span className="ie-tab__label">{props.titleOf(tab.path) || pathStem(tab.path)}</span>
              {tab.mode !== 'edit' ? <span className="ie-tab__mode">{tab.mode}</span> : null}
              <button
                type="button"
                className="ie-tab__close"
                aria-label={dirty ? `Close ${pathFileName(tab.path)} (unsaved)` : `Close ${pathFileName(tab.path)}`}
                onClick={(event) => {
                  event.stopPropagation();
                  props.onClose(tab.id);
                }}
              >
                {/* The dot doubles as the close button: it says "unsaved" until
                    you point at it, and closes when you click. */}
                {dirty ? '●' : '✕'}
              </button>
            </div>
          );
        })}
      </div>

      <div className="ie-tabbar__actions">
        <button
          type="button"
          className="ie-icon-button"
          title="Split right"
          aria-label="Split right"
          onClick={() => props.onSplit('vertical')}
        >
          ▥
        </button>
        <button
          type="button"
          className="ie-icon-button"
          title="Split down"
          aria-label="Split down"
          onClick={() => props.onSplit('horizontal')}
        >
          ▤
        </button>
      </div>
    </div>
  );
}
