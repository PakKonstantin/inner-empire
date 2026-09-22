/**
 * Loading, running and unloading plugins.
 *
 * Three layers of containment, in order of strength:
 *
 * 1. A plugin only ever receives the object graph passed to `onload`, and
 *    every file it can name is a `VaultPath`, so the vault is the boundary.
 * 2. Its manifest declares what it needs; an undeclared call throws.
 * 3. Every entry point is wrapped, so a plugin that throws is disabled with a
 *    notice rather than taking the window down.
 *
 * A separate realm — a Worker — is the next step, and the API is asynchronous
 * throughout so that becomes a transport change rather than a redesign.
 */

import { notify } from '@/components/Notifications';
import { commands as commandRegistry } from '@/commands/registry';
import { api } from '@/services/api';
import { events as appEvents, type AppEventName } from '@/services/events';
import type { VaultPath } from '@/types/domain';
import { asVaultPath } from '@/types/domain';

import {
  PermissionError,
  type AppApi,
  type Disposable,
  type Permission,
  type Plugin,
  type PluginContext,
  type PluginManifest,
} from './api';

export interface LoadedPlugin {
  manifest: PluginManifest;
  instance: Plugin;
  disposables: Disposable[];
  enabled: boolean;
  /** Set when the plugin was disabled because it threw. */
  error: string | null;
}

export interface WorkspaceBridge {
  activeFile: () => VaultPath | null;
  activeContent: () => string | null;
  setActiveContent: (content: string) => void;
  insertAtCursor: (text: string) => void;
  openFile: (path: VaultPath, options?: { newPane?: boolean }) => Promise<void>;
}

export interface UiBridge {
  addPanel: (panel: {
    pluginId: string;
    id: string;
    label: string;
    icon: string;
    side: 'left' | 'right';
    render: (container: HTMLElement) => void | (() => void);
  }) => Disposable;
  addStatusBarItem: (item: {
    pluginId: string;
    id: string;
    render: (container: HTMLElement) => void;
  }) => Disposable;
  confirm: (title: string, message: string) => Promise<boolean>;
}

export const PLUGIN_FOLDER = '.inner-empire/plugins';

export class PluginManager {
  private plugins = new Map<string, LoadedPlugin>();
  private listeners = new Set<() => void>();
  private settings = new Map<string, Record<string, unknown>>();

  constructor(
    private readonly workspace: WorkspaceBridge,
    private readonly ui: UiBridge,
    private readonly appVersion: string,
  ) {}

  list(): LoadedPlugin[] {
    return [...this.plugins.values()].sort((a, b) =>
      a.manifest.name.localeCompare(b.manifest.name),
    );
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /**
   * Find and load every plugin in the vault.
   *
   * One failing plugin does not stop the others: each is loaded in isolation
   * and a failure is recorded against that plugin alone.
   */
  async loadAll(): Promise<void> {
    let listing;
    try {
      listing = await api.listFolder(asVaultPath(PLUGIN_FOLDER));
    } catch {
      // No plugin folder is the normal case, not an error.
      return;
    }

    for (const folder of listing.folders) {
      try {
        await this.load(folder.path);
      } catch (error) {
        notify('error', `Could not load the plugin in ${folder.name}: ${describe(error)}`);
      }
    }
    this.notify();
  }

  async load(folder: VaultPath): Promise<void> {
    const manifestPath = asVaultPath(`${folder}/manifest.json`);
    const manifestNote = await api.readNote(manifestPath);
    const manifest = parseManifest(manifestNote.content);

    if (this.plugins.has(manifest.id)) {
      await this.unload(manifest.id);
    }

    const source = await api.readNote(asVaultPath(`${folder}/main.js`));
    const instance = await evaluateModule(source.content, manifest.id);

    const loaded: LoadedPlugin = {
      manifest,
      instance,
      disposables: [],
      enabled: false,
      error: null,
    };
    this.plugins.set(manifest.id, loaded);
    await this.enable(manifest.id);
  }

  async enable(id: string): Promise<void> {
    const plugin = this.plugins.get(id);
    if (!plugin || plugin.enabled) return;

    const context: PluginContext = {
      manifest: plugin.manifest,
      app: this.buildApi(plugin),
      register: (disposable) => plugin.disposables.push(disposable),
    };

    try {
      await plugin.instance.onload(context);
      plugin.enabled = true;
      plugin.error = null;
    } catch (error) {
      plugin.error = describe(error);
      await this.unload(id);
      notify('error', `The plugin "${plugin.manifest.name}" failed to start: ${plugin.error}`);
    }
    this.notify();
  }

  async unload(id: string): Promise<void> {
    const plugin = this.plugins.get(id);
    if (!plugin) return;

    // Everything registered through `register` is disposed, and a disposer
    // that throws must not prevent the rest from running.
    for (const disposable of plugin.disposables.reverse()) {
      try {
        disposable.dispose();
      } catch (error) {
        console.error(`A disposer in "${id}" threw:`, error);
      }
    }
    plugin.disposables = [];
    commandRegistry.unregisterSource(id);

    if (plugin.enabled) {
      try {
        await plugin.instance.onunload?.();
      } catch (error) {
        console.error(`"${id}" threw while unloading:`, error);
      }
    }
    plugin.enabled = false;
    this.notify();
  }

  async unloadAll(): Promise<void> {
    for (const id of [...this.plugins.keys()]) {
      await this.unload(id);
    }
    this.plugins.clear();
    this.notify();
  }

  /** Build the API object a single plugin sees, bound to its permissions. */
  private buildApi(plugin: LoadedPlugin): AppApi {
    const { manifest } = plugin;
    const require = (permission: Permission, method: string) => {
      if (!manifest.permissions.includes(permission)) {
        throw new PermissionError(manifest.id, permission, method);
      }
    };

    const settingsFor = () => this.settings.get(manifest.id) ?? {};

    return {
      version: this.appVersion,

      vault: {
        list: async (limit) => {
          require('vault:read', 'vault.list');
          return api.recentFiles(limit ?? 500);
        },
        listFolder: async (path) => {
          require('vault:read', 'vault.listFolder');
          return api.listFolder(path);
        },
        read: async (path) => {
          require('vault:read', 'vault.read');
          return api.readNote(path);
        },
        exists: async (path) => {
          require('vault:read', 'vault.exists');
          return api
            .readNote(path)
            .then(() => true)
            .catch(() => false);
        },
        write: async (path, content) => {
          require('vault:write', 'vault.write');
          return api.saveNote(path, content);
        },
        create: async (path, content) => {
          require('vault:write', 'vault.create');
          return api.createNote(path, content);
        },
        createFolder: async (path) => {
          require('vault:write', 'vault.createFolder');
          return api.createFolder(path);
        },
        trash: async (path) => {
          require('vault:write', 'vault.trash');
          await api.deleteEntry(path);
        },
        rename: async (from, to) => {
          require('vault:write', 'vault.rename');
          const outcome = await api.renameEntry(from, to);
          return outcome.to;
        },
        setProperties: async (path, properties) => {
          require('vault:write', 'vault.setProperties');
          return api.setProperties(path, properties);
        },
      },

      metadata: {
        backlinks: async (path) => {
          require('metadata:read', 'metadata.backlinks');
          return api.backlinks(path);
        },
        outgoingLinks: async (path) => {
          require('metadata:read', 'metadata.outgoingLinks');
          return api.outgoingLinks(path);
        },
        headings: async (path) => {
          require('metadata:read', 'metadata.headings');
          return api.outline(path);
        },
        tags: async () => {
          require('metadata:read', 'metadata.tags');
          return api.allTags();
        },
        filesWithTag: async (tag) => {
          require('metadata:read', 'metadata.filesWithTag');
          return api.filesWithTag(tag);
        },
        search: async (query, limit) => {
          require('metadata:read', 'metadata.search');
          return api.searchVault(query, limit ?? 100);
        },
        resolveLink: async (from, target) => {
          require('metadata:read', 'metadata.resolveLink');
          const resolution = await api.resolveLink(from, target);
          return resolution.outcome === 'resolved' ? resolution.path : null;
        },
      },

      workspace: {
        activeFile: () => {
          require('workspace', 'workspace.activeFile');
          return this.workspace.activeFile();
        },
        activeContent: () => {
          require('vault:read', 'workspace.activeContent');
          return this.workspace.activeContent();
        },
        setActiveContent: (content) => {
          require('vault:write', 'workspace.setActiveContent');
          this.workspace.setActiveContent(content);
        },
        insertAtCursor: (text) => {
          require('vault:write', 'workspace.insertAtCursor');
          this.workspace.insertAtCursor(text);
        },
        openFile: async (path, options) => {
          require('workspace', 'workspace.openFile');
          return this.workspace.openFile(path, options);
        },
      },

      ui: {
        addPanel: (panel) => {
          require('ui', 'ui.addPanel');
          const disposable = this.ui.addPanel({ ...panel, pluginId: manifest.id });
          plugin.disposables.push(disposable);
          return disposable;
        },
        addStatusBarItem: (item) => {
          require('ui', 'ui.addStatusBarItem');
          const disposable = this.ui.addStatusBarItem({ ...item, pluginId: manifest.id });
          plugin.disposables.push(disposable);
          return disposable;
        },
        notify: (level, message) => {
          require('ui', 'ui.notify');
          // Prefixed, so a notice is attributable to the plugin that raised it.
          notify(level, `${manifest.name}: ${message}`);
        },
        confirm: async (title, message) => {
          require('ui', 'ui.confirm');
          return this.ui.confirm(title, message);
        },
      },

      events: {
        on: (event, handler) => {
          require('metadata:read', 'events.on');
          const off = appEvents.on(event as AppEventName, (payload) => {
            try {
              handler(payload);
            } catch (error) {
              console.error(`An event handler in "${manifest.id}" threw:`, error);
            }
          });
          const disposable = { dispose: off };
          plugin.disposables.push(disposable);
          return disposable;
        },
      },

      commands: {
        add: (command) => {
          require('commands', 'commands.add');
          const id = `plugin:${manifest.id}:${command.id}`;
          const off = commandRegistry.register({
            id,
            name: command.name,
            category: manifest.name,
            defaultHotkey: command.hotkey,
            source: manifest.id,
            run: command.run,
          });
          const disposable = { dispose: off };
          plugin.disposables.push(disposable);
          return disposable;
        },
        run: (id) => {
          require('commands', 'commands.run');
          return commandRegistry.execute(id);
        },
      },

      settings: {
        get: <T,>(key: string, fallback: T): T => {
          require('settings', 'settings.get');
          const value = settingsFor()[key];
          return value === undefined ? fallback : (value as T);
        },
        set: async (key, value) => {
          require('settings', 'settings.set');
          const current = { ...settingsFor(), [key]: value };
          this.settings.set(manifest.id, current);
          await api.createNote(
            asVaultPath(`${PLUGIN_FOLDER}/${manifest.id}/data.json`),
            JSON.stringify(current, null, 2),
            true,
          );
        },
        all: () => {
          require('settings', 'settings.all');
          return { ...settingsFor() };
        },
      },
    };
  }

  private notify(): void {
    for (const listener of [...this.listeners]) {
      try {
        listener();
      } catch (error) {
        console.error('A plugin-manager listener threw:', error);
      }
    }
  }
}

/** Parse and check a manifest, rejecting anything the loader cannot trust. */
export function parseManifest(source: string): PluginManifest {
  let raw: unknown;
  try {
    raw = JSON.parse(source);
  } catch {
    throw new Error('manifest.json is not valid JSON.');
  }
  if (typeof raw !== 'object' || raw === null) {
    throw new Error('manifest.json must contain an object.');
  }

  const record = raw as Record<string, unknown>;
  const id = record.id;
  const name = record.name;
  const version = record.version;

  if (typeof id !== 'string' || !/^[a-z0-9][a-z0-9-]*$/.test(id)) {
    throw new Error('A plugin id must be lowercase letters, digits and hyphens.');
  }
  if (typeof name !== 'string' || name.trim() === '') {
    throw new Error('A plugin needs a name.');
  }
  if (typeof version !== 'string') {
    throw new Error('A plugin needs a version.');
  }

  const known: Permission[] = [
    'vault:read',
    'vault:write',
    'metadata:read',
    'workspace',
    'ui',
    'commands',
    'settings',
    'network',
  ];
  const requested = Array.isArray(record.permissions) ? record.permissions : [];
  const permissions = requested.filter((permission): permission is Permission =>
    known.includes(permission as Permission),
  );

  const unknown = requested.filter((permission) => !known.includes(permission as Permission));
  if (unknown.length > 0) {
    throw new Error(`Unrecognised permissions: ${unknown.join(', ')}.`);
  }

  return {
    id,
    name,
    version,
    description: typeof record.description === 'string' ? record.description : undefined,
    author: typeof record.author === 'string' ? record.author : undefined,
    minAppVersion:
      typeof record.minAppVersion === 'string' ? record.minAppVersion : undefined,
    permissions,
  };
}

/**
 * Turn a plugin's source into a module.
 *
 * A blob URL and a dynamic import, so the plugin is a real ES module with its
 * own scope: its top-level declarations do not touch the application's, and it
 * has no access to anything the application did not pass into `onload`.
 */
async function evaluateModule(source: string, id: string): Promise<Plugin> {
  const blob = new Blob([source], { type: 'text/javascript' });
  const url = URL.createObjectURL(blob);
  try {
    const module = (await import(/* @vite-ignore */ url)) as { default?: unknown };
    const candidate = module.default;
    if (
      typeof candidate !== 'object' ||
      candidate === null ||
      typeof (candidate as Plugin).onload !== 'function'
    ) {
      throw new Error(`"${id}" must export a default object with an onload function.`);
    }
    return candidate as Plugin;
  } finally {
    URL.revokeObjectURL(url);
  }
}

function describe(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}
