/**
 * The plugin API.
 *
 * A plugin receives exactly the object graph handed to `onload` and nothing
 * else. That is the isolation model: there is no global to reach for, no
 * import of the app's internals, and every path a plugin can name is a
 * `VaultPath`, which cannot address anything outside the vault.
 *
 * On top of that, a manifest declares what the plugin needs. A call outside
 * the declaration throws rather than being quietly allowed, so a plugin that
 * never asked for write access cannot modify a note even by accident.
 */

import type {
  Backlink,
  DirectoryListing,
  FileEntry,
  Heading,
  Note,
  Property,
  ResolvedLink,
  SearchResults,
  TagSummary,
  VaultPath,
} from '@/types/domain';

/** What a plugin may ask for. Anything not listed is refused. */
export type Permission =
  | 'vault:read'
  | 'vault:write'
  | 'metadata:read'
  | 'workspace'
  | 'ui'
  | 'commands'
  | 'settings'
  /** Off unless the user grants it explicitly. Nothing else reaches the network. */
  | 'network';

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  description?: string;
  author?: string;
  /** Minimum app version the plugin needs. */
  minAppVersion?: string;
  permissions: Permission[];
}

/** Anything a plugin registers that must be undone when it unloads. */
export interface Disposable {
  dispose: () => void;
}

export interface PluginCommand {
  id: string;
  name: string;
  /** A suggested shortcut. The user's own binding always wins. */
  hotkey?: string;
  run: () => void | Promise<void>;
}

/** Reading and writing files. */
export interface VaultApi {
  /** Every indexed file. */
  list: (limit?: number) => Promise<FileEntry[]>;
  listFolder: (path: VaultPath) => Promise<DirectoryListing>;
  read: (path: VaultPath) => Promise<Note>;
  exists: (path: VaultPath) => Promise<boolean>;
  /** Requires `vault:write`. */
  write: (path: VaultPath, content: string) => Promise<void>;
  create: (path: VaultPath, content: string) => Promise<VaultPath>;
  createFolder: (path: VaultPath) => Promise<void>;
  /** Moves the file to the vault's trash; it is always recoverable. */
  trash: (path: VaultPath) => Promise<void>;
  rename: (from: VaultPath, to: VaultPath) => Promise<VaultPath>;
  setProperties: (path: VaultPath, properties: Property[]) => Promise<void>;
}

/** Everything derived from the notes. */
export interface MetadataApi {
  backlinks: (path: VaultPath) => Promise<Backlink[]>;
  outgoingLinks: (path: VaultPath) => Promise<ResolvedLink[]>;
  headings: (path: VaultPath) => Promise<Heading[]>;
  tags: () => Promise<TagSummary[]>;
  filesWithTag: (tag: string) => Promise<FileEntry[]>;
  search: (query: string, limit?: number) => Promise<SearchResults>;
  resolveLink: (from: VaultPath, target: string) => Promise<VaultPath | null>;
}

/** Tabs and panes. */
export interface WorkspaceApi {
  activeFile: () => VaultPath | null;
  openFile: (path: VaultPath, options?: { newPane?: boolean }) => Promise<void>;
  /** The text of the note being edited, as it currently stands. */
  activeContent: () => string | null;
  /** Replace the active note's text. Requires `vault:write`. */
  setActiveContent: (content: string) => void;
  /** Insert at the cursor. Requires `vault:write`. */
  insertAtCursor: (text: string) => void;
}

/** Panels, notices and views. */
export interface UiApi {
  /** Add a panel to a sidebar. */
  addPanel: (panel: {
    id: string;
    label: string;
    icon: string;
    side: 'left' | 'right';
    render: (container: HTMLElement) => void | (() => void);
  }) => Disposable;
  /** Add a button to the status bar. */
  addStatusBarItem: (item: { id: string; render: (container: HTMLElement) => void }) => Disposable;
  notify: (level: 'info' | 'success' | 'warning' | 'error', message: string) => void;
  /** Ask a yes/no question. */
  confirm: (title: string, message: string) => Promise<boolean>;
}

/** Subscribing to what happens in the vault. */
export interface EventsApi {
  on: (
    event:
      | 'fileCreated'
      | 'fileModified'
      | 'fileDeleted'
      | 'fileRenamed'
      | 'activeFileChanged'
      | 'vaultOpened'
      | 'vaultClosed'
      | 'workspaceChanged',
    handler: (payload: unknown) => void,
  ) => Disposable;
}

/** Per-plugin storage, kept beside the plugin in the vault. */
export interface SettingsApi {
  get: <T>(key: string, fallback: T) => T;
  set: (key: string, value: unknown) => Promise<void>;
  all: () => Record<string, unknown>;
}

export interface AppApi {
  vault: VaultApi;
  metadata: MetadataApi;
  workspace: WorkspaceApi;
  ui: UiApi;
  events: EventsApi;
  commands: {
    add: (command: PluginCommand) => Disposable;
    run: (id: string) => Promise<boolean>;
  };
  settings: SettingsApi;
  /** The running application's version, for compatibility checks. */
  version: string;
}

export interface PluginContext {
  app: AppApi;
  manifest: PluginManifest;
  /** Register something to be cleaned up when the plugin unloads. */
  register: (disposable: Disposable) => void;
}

/** What a plugin module must export. */
export interface Plugin {
  onload: (context: PluginContext) => void | Promise<void>;
  onunload?: () => void | Promise<void>;
}

/** Thrown when a plugin calls something its manifest did not ask for. */
export class PermissionError extends Error {
  constructor(
    public readonly pluginId: string,
    public readonly permission: Permission,
    method: string,
  ) {
    super(
      `The plugin "${pluginId}" called ${method}, which needs the "${permission}" permission. ` +
        `Add it to the plugin's manifest.json.`,
    );
    this.name = 'PermissionError';
  }
}
