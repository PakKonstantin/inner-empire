/**
 * The toolbar.
 *
 * The window had no top bar at all: no way to see where the open note lives,
 * no search without first finding the right sidebar panel, and no visible
 * sign that a sidebar could be collapsed. This is that bar, and it is
 * deliberately short — the brief is explicit that a toolbar which grows is
 * the failure mode, so what is here is what a person reaches for several
 * times an hour and nothing else.
 *
 * Left: the sidebar toggles and the trail you walked. Middle: where you are.
 * Right: search, and the four actions that start something.
 */

import { useCallback, useState } from 'react';

import type { VaultPath } from '@/types/domain';
import { VAULT_ROOT, asVaultPath, pathFileName } from '@/types/domain';
import { Breadcrumb, IconButton, SearchInput, Tooltip } from '@/ui';
import type { BreadcrumbSegment } from '@/ui';

export interface ToolbarProps {
  vaultName: string | null;
  activePath: VaultPath | null;
  /** The note's title, which is not always its file name. */
  activeTitle: string | null;

  leftSidebarVisible: boolean;
  rightSidebarVisible: boolean;
  onToggleSidebar: (side: 'left' | 'right') => void;

  canGoBack: boolean;
  canGoForward: boolean;
  onBack: () => void;
  onForward: () => void;

  /** Show this folder in the explorer. */
  onRevealFolder: (folder: VaultPath) => void;
  onSearch: (query: string) => void;
  onNewNote: () => void;
  onQuickSwitch: () => void;
  onCommandPalette: () => void;
  onSettings: () => void;
}

export function Toolbar(props: ToolbarProps) {
  const [query, setQuery] = useState('');

  const submitSearch = useCallback(
    (event: React.FormEvent) => {
      event.preventDefault();
      if (query.trim()) props.onSearch(query.trim());
    },
    [props, query],
  );

  return (
    <header className="ie-toolbar ie-chrome">
      <div className="ie-toolbar__group ie-toolbar__group--start">
        <Tooltip content="Toggle the left sidebar" shortcut="Ctrl+B">
          <IconButton
            icon="sidebar-left"
            label="Toggle the left sidebar"
            pressed={props.leftSidebarVisible}
            onClick={() => props.onToggleSidebar('left')}
          />
        </Tooltip>

        <div className="ie-toolbar__history">
          <Tooltip content="Back" shortcut="Ctrl+[">
            <IconButton
              icon="arrow-left"
              label="Back"
              disabled={!props.canGoBack}
              onClick={props.onBack}
            />
          </Tooltip>
          <Tooltip content="Forward" shortcut="Ctrl+]">
            <IconButton
              icon="arrow-right"
              label="Forward"
              disabled={!props.canGoForward}
              onClick={props.onForward}
            />
          </Tooltip>
        </div>
      </div>

      <div className="ie-toolbar__group ie-toolbar__group--center">
        <Breadcrumb
          segments={breadcrumbFor(props.vaultName, props.activePath, props.activeTitle, props.onRevealFolder)}
        />
      </div>

      <div className="ie-toolbar__group ie-toolbar__group--end">
        <form className="ie-toolbar__search" onSubmit={submitSearch} role="search">
          <SearchInput
            label="Search the vault"
            placeholder="Search"
            value={query}
            onValueChange={setQuery}
          />
        </form>

        <Tooltip content="New note" shortcut="Ctrl+N">
          <IconButton icon="plus" label="New note" onClick={props.onNewNote} />
        </Tooltip>
        <Tooltip content="Go to a note" shortcut="Ctrl+O">
          <IconButton icon="file-text" label="Go to a note" onClick={props.onQuickSwitch} />
        </Tooltip>
        <Tooltip content="Commands" shortcut="Ctrl+P">
          <IconButton icon="command" label="Commands" onClick={props.onCommandPalette} />
        </Tooltip>
        <Tooltip content="Settings" shortcut="Ctrl+,">
          <IconButton icon="settings" label="Settings" onClick={props.onSettings} />
        </Tooltip>

        <Tooltip content="Toggle the right sidebar" shortcut="Ctrl+Shift+B">
          <IconButton
            icon="sidebar-right"
            label="Toggle the right sidebar"
            pressed={props.rightSidebarVisible}
            onClick={() => props.onToggleSidebar('right')}
          />
        </Tooltip>
      </div>
    </header>
  );
}

/**
 * The trail from the vault to the open note.
 *
 * Every folder is clickable and shows itself in the explorer; the note itself
 * is where you already are, so it is text. When nothing is open the vault's
 * own name stands alone, which is still useful — it says which vault this
 * window belongs to.
 */
export function breadcrumbFor(
  vaultName: string | null,
  path: VaultPath | null,
  title: string | null,
  onRevealFolder: (folder: VaultPath) => void,
): BreadcrumbSegment[] {
  const segments: BreadcrumbSegment[] = [];

  if (vaultName) {
    segments.push({
      label: vaultName,
      title: 'Show the vault root',
      onClick: () => onRevealFolder(VAULT_ROOT),
    });
  }

  if (!path) return segments;

  const parts = path.split('/').filter(Boolean);
  const folders = parts.slice(0, -1);

  let walked = '';
  for (const folder of folders) {
    walked = walked ? `${walked}/${folder}` : folder;
    const target = asVaultPath(walked);
    segments.push({
      label: folder,
      title: walked,
      onClick: () => onRevealFolder(target),
    });
  }

  segments.push({ label: title || pathFileName(path), title: path });
  return segments;
}
