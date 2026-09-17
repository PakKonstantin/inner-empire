/**
 * The explorer's data.
 *
 * Folders are fetched one level at a time, when they are expanded. A vault
 * with fifty thousand files across a deep hierarchy therefore costs one
 * directory read to open, not a full walk — which is also why the tree stays
 * responsive while the initial scan is still running.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';

import { api } from '@/services/api';
import { events } from '@/services/events';
import type { DirectoryListing, FileEntry, FolderEntry, VaultPath } from '@/types/domain';
import { VAULT_ROOT, pathParent } from '@/types/domain';

/** One row in the flattened tree the list renders. */
export interface TreeRow {
  kind: 'folder' | 'file';
  path: VaultPath;
  name: string;
  depth: number;
  expanded?: boolean;
  file?: FileEntry;
  folder?: FolderEntry;
}

export type SortOrder = 'name' | 'nameDescending' | 'modified' | 'created';

export interface FileTreeOptions {
  sort: SortOrder;
  /** Substring filter over names, applied to loaded levels. */
  filter: string;
  /** Show attachments as well as notes. */
  showAttachments: boolean;
}

export function useFileTree(options: FileTreeOptions) {
  const [listings, setListings] = useState<Record<string, DirectoryListing>>({});
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set([VAULT_ROOT]));
  const [loading, setLoading] = useState<Set<string>>(() => new Set());

  const load = useCallback(async (path: VaultPath) => {
    setLoading((current) => new Set(current).add(path));
    try {
      const listing = await api.listFolder(path);
      setListings((current) => ({ ...current, [path]: listing }));
    } catch {
      // A folder that cannot be read is left collapsed rather than crashing
      // the tree; the diagnostics panel reports why.
      setListings((current) => ({
        ...current,
        [path]: { path, folders: [], files: [] },
      }));
    } finally {
      setLoading((current) => {
        const next = new Set(current);
        next.delete(path);
        return next;
      });
    }
  }, []);

  useEffect(() => {
    void load(VAULT_ROOT);
  }, [load]);

  /** Reload the folder a changed file lives in, and nothing else. */
  const refreshFolderOf = useCallback(
    (path: VaultPath) => {
      const parent = pathParent(path);
      if (listings[parent] || parent === VAULT_ROOT) void load(parent);
    },
    [listings, load],
  );

  useEffect(() => {
    const offs = [
      events.on('fileCreated', ({ path }) => refreshFolderOf(path)),
      events.on('fileDeleted', ({ path }) => refreshFolderOf(path)),
      events.on('fileRenamed', ({ from, to }) => {
        refreshFolderOf(from);
        refreshFolderOf(to);
      }),
      events.on('vaultOpened', () => {
        setListings({});
        setExpanded(new Set([VAULT_ROOT]));
        void load(VAULT_ROOT);
      }),
      events.on('indexCompleted', () => {
        // Titles come from frontmatter, so a finished scan can change what the
        // tree should display even when no file was created or deleted.
        void load(VAULT_ROOT);
      }),
    ];
    return () => offs.forEach((off) => off());
  }, [refreshFolderOf, load]);

  const toggle = useCallback(
    (path: VaultPath) => {
      setExpanded((current) => {
        const next = new Set(current);
        if (next.has(path)) {
          next.delete(path);
        } else {
          next.add(path);
          if (!listings[path]) void load(path);
        }
        return next;
      });
    },
    [listings, load],
  );

  const expand = useCallback(
    (path: VaultPath) => {
      setExpanded((current) => {
        if (current.has(path)) return current;
        const next = new Set(current).add(path);
        if (!listings[path]) void load(path);
        return next;
      });
    },
    [listings, load],
  );

  /** Expand every ancestor so a path becomes visible. */
  const revealPath = useCallback(
    (path: VaultPath) => {
      const ancestors: VaultPath[] = [];
      let current = pathParent(path);
      while (current) {
        ancestors.unshift(current);
        current = pathParent(current);
      }
      ancestors.forEach(expand);
    },
    [expand],
  );

  const rows = useMemo(
    () => flatten(listings, expanded, options),
    [listings, expanded, options],
  );

  return {
    rows,
    expanded,
    loading,
    toggle,
    expand,
    revealPath,
    refresh: load,
    collapseAll: () => setExpanded(new Set([VAULT_ROOT])),
  };
}

/** Walk the loaded listings depth first, producing the rows to render. */
function flatten(
  listings: Record<string, DirectoryListing>,
  expanded: Set<string>,
  options: FileTreeOptions,
): TreeRow[] {
  const rows: TreeRow[] = [];
  const filter = options.filter.trim().toLowerCase();

  const walk = (path: VaultPath, depth: number): void => {
    const listing = listings[path];
    if (!listing) return;

    for (const folder of sortFolders(listing.folders, options.sort)) {
      const isExpanded = expanded.has(folder.path);
      // A filter hides folders that contain nothing matching, but only where
      // the contents have been loaded; an unloaded folder stays visible so the
      // user can open it and look.
      const matches = !filter || folder.name.toLowerCase().includes(filter);
      const childRows: TreeRow[] = [];
      if (isExpanded) {
        const before = rows.length;
        walk(folder.path, depth + 1);
        childRows.push(...rows.splice(before));
      }

      if (matches || childRows.length > 0 || (filter && !listings[folder.path])) {
        rows.push({
          kind: 'folder',
          path: folder.path,
          name: folder.name,
          depth,
          expanded: isExpanded,
          folder,
        });
        rows.push(...childRows);
      }
    }

    for (const file of sortFiles(listing.files, options.sort)) {
      if (!options.showAttachments && file.kind !== 'note' && file.kind !== 'canvas') continue;
      if (filter && !file.name.toLowerCase().includes(filter) && !file.title.toLowerCase().includes(filter)) {
        continue;
      }
      rows.push({ kind: 'file', path: file.path, name: file.name, depth, file });
    }
  };

  walk(VAULT_ROOT, 0);
  return rows;
}

function sortFolders(folders: FolderEntry[], order: SortOrder): FolderEntry[] {
  const sorted = [...folders];
  sorted.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
  if (order === 'nameDescending') sorted.reverse();
  return sorted;
}

function sortFiles(files: FileEntry[], order: SortOrder): FileEntry[] {
  const sorted = [...files];
  switch (order) {
    case 'modified':
      sorted.sort((a, b) => b.modifiedMs - a.modifiedMs);
      break;
    case 'created':
      // Creation time is not tracked separately; modification time is the
      // closest honest answer, and saying so beats a field that lies.
      sorted.sort((a, b) => a.modifiedMs - b.modifiedMs);
      break;
    case 'nameDescending':
      sorted.sort((a, b) => b.name.localeCompare(a.name, undefined, { sensitivity: 'base' }));
      break;
    default:
      sorted.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
  }
  return sorted;
}
