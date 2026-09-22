/**
 * The file tree.
 *
 * Rows are windowed, so a fifty-thousand-file vault costs the same number of
 * DOM nodes as a small one. Drag and drop moves files between folders, and
 * every move goes through the rename command so links are rewritten.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useContextMenu, type MenuEntry } from '@/components/ContextMenu';
import { VirtualList } from '@/components/VirtualList';
import { notify } from '@/components/Notifications';
import { api } from '@/services/api';
import type { VaultPath } from '@/types/domain';
import { VAULT_ROOT, joinPath, pathFileName, pathParent } from '@/types/domain';
import { EmptyState, Icon, IconButton, SearchInput, Select, Tooltip, iconForFile } from '@/ui';

import { useFileTree, type FileTreeOptions, type SortOrder, type TreeRow } from './useFileTree';

const ROW_HEIGHT = 26;

export interface FileExplorerProps {
  activePath: VaultPath | null;
  onOpen: (path: VaultPath, options?: { newPane?: boolean }) => void;
  onRename: (path: VaultPath) => void;
  onDelete: (path: VaultPath) => void;
  onCreateNote: (folder: VaultPath) => void;
  onCreateFolder: (folder: VaultPath) => void;
}

export function FileExplorer(props: FileExplorerProps) {
  const [options, setOptions] = useState<FileTreeOptions>({
    sort: 'name',
    filter: '',
    showAttachments: true,
  });
  const tree = useFileTree(options);
  const menu = useContextMenu();
  const [dragging, setDragging] = useState<VaultPath | null>(null);
  const [dropTarget, setDropTarget] = useState<VaultPath | null>(null);
  const [revealed, setRevealed] = useState<VaultPath | null>(null);
  const filterInput = useRef<HTMLInputElement | null>(null);

  // The hook hands back a fresh object every render, so the listener below
  // reads it through a ref instead of re-subscribing each time.
  const treeRef = useRef(tree);
  treeRef.current = tree;

  /**
   * Someone elsewhere — the breadcrumb, for one — asked for a path to be
   * shown here. The tree's own state lives in this component, so the request
   * arrives as an event rather than as a ref reaching in from the shell.
   */
  useEffect(() => {
    const onReveal = (event: Event) => {
      const detail = (event as CustomEvent<{ path?: VaultPath; folder?: boolean }>).detail;
      if (!detail?.path) return;
      treeRef.current.revealPath(detail.path);
      // A folder also opens itself; a file only needs its ancestors open.
      if (detail.folder) treeRef.current.expand(detail.path);
      // A filter would hide whatever was just revealed.
      setOptions((current) => (current.filter ? { ...current, filter: '' } : current));
      setRevealed(detail.path);
    };
    window.addEventListener('ie:reveal-path', onReveal);
    return () => window.removeEventListener('ie:reveal-path', onReveal);
  }, []);

  // A reveal holds the list's attention only until the user opens something
  // else; after that the open note is what should stay in view.
  useEffect(() => {
    setRevealed(null);
  }, [props.activePath]);

  const activeIndex = useMemo(
    () => tree.rows.findIndex((row) => row.path === props.activePath),
    [tree.rows, props.activePath],
  );

  /**
   * Where the list should scroll.
   *
   * A reveal wins over the open note until its row actually exists — folders
   * load a level at a time, so the row a reveal is waiting for often appears
   * a tick or two later.
   */
  const revealedIndex = useMemo(
    () => (revealed ? tree.rows.findIndex((row) => row.path === revealed) : -1),
    [tree.rows, revealed],
  );
  const scrollToIndex = revealedIndex >= 0 ? revealedIndex : activeIndex;

  const onRowContextMenu = useCallback(
    (event: React.MouseEvent, row: TreeRow) => {
      event.preventDefault();
      event.stopPropagation();

      const isFolder = row.kind === 'folder';
      const entries: MenuEntry[] = isFolder
        ? [
            { id: 'new-note', label: 'New note here', run: () => props.onCreateNote(row.path) },
            { id: 'new-folder', label: 'New folder here', run: () => props.onCreateFolder(row.path) },
            { id: 'sep-1', separator: true },
            { id: 'rename', label: 'Rename', run: () => props.onRename(row.path) },
            { id: 'delete', label: 'Move to trash', danger: true, run: () => props.onDelete(row.path) },
            { id: 'sep-2', separator: true },
            { id: 'copy-path', label: 'Copy path', run: () => copyToClipboard(row.path) },
            { id: 'reveal', label: 'Show in file manager', run: () => void api.revealInFileManager(row.path) },
          ]
        : [
            { id: 'open', label: 'Open', run: () => props.onOpen(row.path) },
            {
              id: 'open-split',
              label: 'Open in a split',
              run: () => props.onOpen(row.path, { newPane: true }),
            },
            { id: 'sep-1', separator: true },
            { id: 'rename', label: 'Rename', hint: 'F2', run: () => props.onRename(row.path) },
            {
              id: 'duplicate',
              label: 'Duplicate',
              run: async () => {
                const copy = await api.duplicateEntry(row.path);
                notify('success', `Created ${pathFileName(copy)}.`);
              },
            },
            { id: 'delete', label: 'Move to trash', danger: true, run: () => props.onDelete(row.path) },
            { id: 'sep-2', separator: true },
            { id: 'copy-path', label: 'Copy path', run: () => copyToClipboard(row.path) },
            {
              id: 'copy-link',
              label: 'Copy link',
              run: () => copyToClipboard(`[[${row.path.replace(/\.(md|markdown)$/i, '')}]]`),
            },
            { id: 'reveal', label: 'Show in file manager', run: () => void api.revealInFileManager(row.path) },
          ];

      menu.open(event, entries);
    },
    [menu, props],
  );

  const onBackgroundContextMenu = useCallback(
    (event: React.MouseEvent) => {
      event.preventDefault();
      menu.open(event, [
        { id: 'new-note', label: 'New note', run: () => props.onCreateNote(VAULT_ROOT) },
        { id: 'new-folder', label: 'New folder', run: () => props.onCreateFolder(VAULT_ROOT) },
        { id: 'sep', separator: true },
        { id: 'collapse', label: 'Collapse all', run: () => tree.collapseAll() },
      ]);
    },
    [menu, props, tree],
  );

  /** Move a file or folder, letting the backend rewrite the links. */
  const moveInto = useCallback(
    async (source: VaultPath, targetFolder: VaultPath) => {
      if (source === targetFolder) return;
      const destination = joinPath(targetFolder, pathFileName(source));
      if (destination === source) return;
      // Dropping a folder into its own subtree would destroy it.
      if (destination.startsWith(`${source}/`)) {
        notify('warning', 'A folder cannot be moved inside itself.');
        return;
      }

      try {
        const outcome = await api.renameEntry(source, destination);
        if (outcome.linksUpdated === 0) {
          notify('success', `Moved ${pathFileName(source)}.`);
        }
      } catch (error) {
        notify('error', errorMessage(error));
      }
    },
    [],
  );

  const renderRow = useCallback(
    (row: TreeRow) => {
      const isActive = row.path === props.activePath;
      const isDropTarget = dropTarget === row.path;
      const isRevealed = revealed === row.path && !isActive;

      return (
        <div
          className={[
            'ie-tree-row',
            `ie-tree-row--${row.kind}`,
            isActive ? 'is-active' : '',
            isRevealed ? 'is-revealed' : '',
            isDropTarget ? 'is-drop-target' : '',
          ]
            .filter(Boolean)
            .join(' ')}
          style={{ paddingLeft: `${8 + row.depth * 14}px` }}
          role="treeitem"
          aria-expanded={row.kind === 'folder' ? row.expanded : undefined}
          aria-selected={isActive}
          tabIndex={-1}
          draggable
          onDragStart={(event) => {
            setDragging(row.path);
            event.dataTransfer.effectAllowed = 'move';
            event.dataTransfer.setData('text/plain', row.path);
          }}
          onDragEnd={() => {
            setDragging(null);
            setDropTarget(null);
          }}
          onDragOver={(event) => {
            const folder = row.kind === 'folder' ? row.path : pathParent(row.path);
            if (!dragging || dragging === folder) return;
            event.preventDefault();
            event.dataTransfer.dropEffect = 'move';
            setDropTarget(row.kind === 'folder' ? row.path : null);
          }}
          onDragLeave={() => setDropTarget(null)}
          onDrop={(event) => {
            event.preventDefault();
            const source = (event.dataTransfer.getData('text/plain') || dragging) as VaultPath | null;
            setDragging(null);
            setDropTarget(null);
            if (!source) return;
            const folder = row.kind === 'folder' ? row.path : pathParent(row.path);
            void moveInto(source, folder);
          }}
          onClick={() => {
            if (row.kind === 'folder') tree.toggle(row.path);
            else props.onOpen(row.path);
          }}
          onAuxClick={(event) => {
            // Middle click opens in a split, matching the tab bar.
            if (event.button === 1 && row.kind === 'file') {
              event.preventDefault();
              props.onOpen(row.path, { newPane: true });
            }
          }}
          onContextMenu={(event) => onRowContextMenu(event, row)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              if (row.kind === 'folder') tree.toggle(row.path);
              else props.onOpen(row.path);
            }
            if (event.key === 'F2') props.onRename(row.path);
          }}
        >
          {row.kind === 'folder' ? (
            <span className={`ie-tree-chevron${row.expanded ? ' is-open' : ''}`}>
              <Icon name="chevron-right" size={14} />
            </span>
          ) : (
            <span className="ie-tree-chevron ie-tree-chevron--placeholder" aria-hidden="true" />
          )}
          <Icon
            className="ie-tree-icon"
            name={row.kind === 'folder' ? (row.expanded ? 'folder-open' : 'folder') : iconForFile(row.path)}
            size={15}
          />
          <span className="ie-tree-label" title={row.path}>
            {row.kind === 'file' ? displayName(row) : row.name}
          </span>
          {row.kind === 'folder' && row.folder && row.folder.childFileCount > 0 ? (
            <span className="ie-tree-count">{row.folder.childFileCount}</span>
          ) : null}
        </div>
      );
    },
    [dragging, dropTarget, moveInto, onRowContextMenu, props, revealed, tree],
  );

  return (
    <div className="ie-explorer ie-chrome" onContextMenu={onBackgroundContextMenu}>
      <div className="ie-panel-header">
        <span>Files</span>
        <div className="ie-explorer__actions">
          <Tooltip content="New note">
            <IconButton
              icon="plus"
              label="New note"
              size="sm"
              onClick={() => props.onCreateNote(VAULT_ROOT)}
            />
          </Tooltip>
          <Tooltip content="New folder">
            <IconButton
              icon="folder-plus"
              label="New folder"
              size="sm"
              onClick={() => props.onCreateFolder(VAULT_ROOT)}
            />
          </Tooltip>
          <Tooltip content="Collapse all">
            <IconButton
              icon="chevrons-up"
              label="Collapse all"
              size="sm"
              onClick={() => tree.collapseAll()}
            />
          </Tooltip>
        </div>
      </div>

      <div className="ie-explorer__controls">
        <SearchInput
          ref={filterInput}
          label="Filter by name"
          placeholder="Filter by name"
          value={options.filter}
          onValueChange={(value) => setOptions((current) => ({ ...current, filter: value }))}
        />
        <Select
          label="Sort order"
          value={options.sort}
          onChange={(event) =>
            setOptions((current) => ({ ...current, sort: event.target.value as SortOrder }))
          }
          options={[
            { value: 'name', label: 'Name A–Z' },
            { value: 'nameDescending', label: 'Name Z–A' },
            { value: 'modified', label: 'Recently changed' },
            { value: 'created', label: 'Oldest first' },
          ]}
        />
      </div>

      <VirtualList
        className="ie-explorer__list"
        items={tree.rows}
        rowHeight={ROW_HEIGHT}
        keyOf={(row) => row.path}
        renderRow={renderRow}
        scrollToIndex={scrollToIndex}
        emptyState={
          options.filter ? (
            <EmptyState
              compact
              icon="search"
              title="Nothing matches"
              description="No file or folder here has that in its name."
              action={{
                label: 'Clear the filter',
                onClick: () => setOptions((current) => ({ ...current, filter: '' })),
              }}
            />
          ) : (
            <EmptyState
              compact
              icon="file-text"
              title="This vault is empty"
              description="Notes you create will appear here."
              action={{ label: 'New note', onClick: () => props.onCreateNote(VAULT_ROOT) }}
            />
          )
        }
      />
    </div>
  );
}

/** Prefer the note's title over its filename, when they differ. */
function displayName(row: TreeRow): string {
  if (!row.file) return row.name;
  const stem = row.name.replace(/\.(md|markdown)$/i, '');
  return row.file.kind === 'note' ? row.file.title || stem : row.name;
}

async function copyToClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    notify('success', 'Copied.');
  } catch {
    notify('error', 'Could not reach the clipboard.');
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Object && 'message' in error ? String(error.message) : String(error);
}
