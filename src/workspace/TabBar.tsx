/**
 * The tab strip for one pane.
 *
 * Tabs can be dragged to reorder within a pane and dragged onto another pane
 * to move there. Middle click closes, as it does in a browser. A tab with
 * unsaved changes shows a dot in place of its close button, so the state is
 * visible without hovering.
 */

import { useCallback, useState } from 'react';

import { useContextMenu, type MenuEntry } from '@/components/ContextMenu';
import { notify } from '@/components/Notifications';
import type { TabState, VaultPath } from '@/types/domain';
import { pathFileName, pathStem } from '@/types/domain';
import { Icon, IconButton, Tooltip, useRovingFocus } from '@/ui';

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
  onCloseToTheRight: (tabId: string) => void;
  onCloseAll: () => void;
  /** Whether anything is on the reopen stack. */
  canReopen: boolean;
  onReopenClosed: () => void;
  onOpenInNewPane: (tabId: string) => void;
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
  // A tab strip is a tablist, and a tablist moves between its tabs with the
  // arrow keys rather than making the user tab through every one.
  const roving = useRovingFocus('horizontal');

  const onTabContextMenu = useCallback(
    (event: React.MouseEvent, tab: TabState) => {
      event.preventDefault();
      const index = props.tabs.findIndex((candidate) => candidate.id === tab.id);
      // Nothing to the right means nothing to close; a disabled entry says so
      // more clearly than one that quietly does nothing.
      const hasTabsToTheRight = props.tabs.slice(index + 1).some((candidate) => !candidate.pinned);

      const entries: MenuEntry[] = [
        { id: 'close', label: 'Close', hint: 'Ctrl+W', run: () => props.onClose(tab.id) },
        { id: 'close-others', label: 'Close others', run: () => props.onCloseOthers(tab.id) },
        {
          id: 'close-right',
          label: 'Close to the right',
          disabled: !hasTabsToTheRight,
          run: () => props.onCloseToTheRight(tab.id),
        },
        { id: 'close-all', label: 'Close all', run: props.onCloseAll },
        {
          id: 'reopen',
          label: 'Reopen closed tab',
          hint: 'Ctrl+Shift+W',
          disabled: !props.canReopen,
          run: props.onReopenClosed,
        },
        { id: 'sep-1', separator: true },
        {
          id: 'pin',
          label: tab.pinned ? 'Unpin' : 'Pin',
          run: () => props.onTogglePin(tab.id),
        },
        { id: 'duplicate', label: 'Duplicate', run: () => props.onDuplicate(tab.id) },
        {
          id: 'open-new-pane',
          label: 'Open in a new pane',
          run: () => props.onOpenInNewPane(tab.id),
        },
        { id: 'sep-2', separator: true },
        { id: 'copy-path', label: 'Copy path', run: () => copyToClipboard(tab.path) },
        {
          id: 'copy-link',
          label: 'Copy link',
          run: () => copyToClipboard(`[[${tab.path.replace(/\.(md|markdown)$/i, '')}]]`),
        },
        { id: 'sep-3', separator: true },
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
      ref={roving.container}
      role="tablist"
      aria-label="Open notes"
      onMouseDown={props.onFocusPane}
      onKeyDown={roving.onKeyDown}
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
              data-roving=""
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
                if (event.key === 'Enter' || event.key === ' ') {
                  props.onSelect(tab.id);
                } else if (event.key === 'Delete' || event.key === 'Backspace') {
                  props.onClose(tab.id);
                } else {
                  return;
                }
                event.preventDefault();
              }}
            >
              {tab.pinned ? (
                <Icon name="pin" size={12} className="ie-tab__pin" label="Pinned" />
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
                {/* The dot doubles as the close button: it says "unsaved"
                    until you point at it, at which moment it becomes the cross
                    it has always also been. Both are drawn and the CSS decides,
                    so the swap costs no re-render on hover. */}
                {dirty ? <Icon name="dot" size={10} className="ie-tab__dirty" /> : null}
                <Icon name="close" size={12} className="ie-tab__cross" />
              </button>
            </div>
          );
        })}
      </div>

      <div className="ie-tabbar__actions">
        <Tooltip content="Split right">
          <IconButton
            icon="split-vertical"
            label="Split right"
            size="sm"
            onClick={() => props.onSplit('vertical')}
          />
        </Tooltip>
        <Tooltip content="Split down">
          <IconButton
            icon="split-horizontal"
            label="Split down"
            size="sm"
            onClick={() => props.onSplit('horizontal')}
          />
        </Tooltip>
      </div>
    </div>
  );
}

async function copyToClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    notify('success', 'Copied.');
  } catch {
    notify('error', 'Could not reach the clipboard.');
  }
}
