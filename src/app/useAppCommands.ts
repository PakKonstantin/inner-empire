/**
 * Registering the built-in commands.
 *
 * Every action the user can take is registered here, which means the palette,
 * the hotkey editor and any plugin all see one list. Adding an action is one
 * entry rather than a menu item, a shortcut and a palette registration.
 */

import { useEffect } from 'react';

import { notify } from '@/components/Notifications';
import { commands, type Command } from '@/commands/registry';
import { api } from '@/services/api';
import { pickFolder } from '@/services/dialogs';
import { useSettingsStore } from '@/state/settingsStore';
import { useVaultStore } from '@/state/vaultStore';
import { useWorkspaceStore } from '@/state/workspaceStore';
import type { PaneLayout, PaneNode, VaultPath } from '@/types/domain';

export interface CommandActions {
  openPalette: (mode: 'commands' | 'files') => void;
  openSearch: (query?: string) => void;
  openSettings: () => void;
  openGraph: () => void;
  createNote: () => void;
  createFolder: () => void;
  renameActive: () => void;
  deleteActive: () => void;
  insertTemplate: () => void;
  activePath: () => VaultPath | null;
  focusEditor: () => void;
  exportActive: (format?: 'html' | 'markdown') => void;
  /** Render the note through the browser's print dialogue, which can save a PDF. */
  printActive: () => void;
  importFiles: () => void;
}

export function useAppCommands(actions: CommandActions): void {
  useEffect(() => {
    const workspace = useWorkspaceStore.getState;
    const settings = useSettingsStore.getState;
    const vault = useVaultStore.getState;

    const hasVault = () => vault().status !== 'closed';
    const hasNote = () => actions.activePath() !== null;

    const list: Command[] = [
      {
        id: 'palette.commands',
        name: 'Command palette',
        category: 'General',
        defaultHotkey: 'Mod+P',
        run: () => actions.openPalette('commands'),
      },
      {
        id: 'palette.files',
        name: 'Open note',
        category: 'General',
        defaultHotkey: 'Mod+O',
        isAvailable: hasVault,
        run: () => actions.openPalette('files'),
      },
      {
        id: 'search.open',
        name: 'Search in vault',
        category: 'General',
        defaultHotkey: 'Mod+Shift+F',
        isAvailable: hasVault,
        run: () => actions.openSearch(),
      },
      {
        id: 'note.create',
        name: 'New note',
        category: 'Notes',
        defaultHotkey: 'Mod+N',
        isAvailable: hasVault,
        run: () => actions.createNote(),
      },
      {
        id: 'folder.create',
        name: 'New folder',
        category: 'Notes',
        isAvailable: hasVault,
        run: () => actions.createFolder(),
      },
      {
        id: 'note.save',
        name: 'Save',
        category: 'Notes',
        defaultHotkey: 'Mod+S',
        isAvailable: hasNote,
        run: async () => {
          const path = actions.activePath();
          if (path) await workspace().saveBuffer(path);
        },
      },
      {
        id: 'note.saveAll',
        name: 'Save everything',
        category: 'Notes',
        defaultHotkey: 'Mod+Alt+S',
        isAvailable: hasVault,
        run: () => workspace().saveAll(),
      },
      {
        id: 'note.rename',
        name: 'Rename this note',
        category: 'Notes',
        defaultHotkey: 'F2',
        isAvailable: hasNote,
        run: () => actions.renameActive(),
      },
      {
        id: 'note.delete',
        name: 'Move this note to the trash',
        category: 'Notes',
        isAvailable: hasNote,
        run: () => actions.deleteActive(),
      },
      {
        id: 'note.export',
        name: 'Export this note as HTML',
        category: 'Notes',
        isAvailable: hasNote,
        run: () => actions.exportActive(),
      },
      {
        id: 'note.exportMarkdown',
        name: 'Export this note as Markdown',
        category: 'Notes',
        isAvailable: hasNote,
        run: () => actions.exportActive('markdown'),
      },
      {
        id: 'note.print',
        name: 'Print, or save as PDF',
        category: 'Notes',
        defaultHotkey: 'Mod+Shift+P',
        isAvailable: hasNote,
        run: () => actions.printActive(),
      },
      {
        id: 'vault.import',
        name: 'Import files into this vault',
        category: 'Vault',
        isAvailable: hasVault,
        run: () => actions.importFiles(),
      },
      {
        id: 'note.dailyNote',
        name: "Open today's note",
        category: 'Notes',
        defaultHotkey: 'Mod+Shift+D',
        isAvailable: hasVault,
        run: async () => {
          const { path, created } = await api.openDailyNote();
          await workspace().openFile(path);
          if (created) notify('success', `Created ${path}.`);
        },
      },
      {
        id: 'note.yesterday',
        name: "Open yesterday's note",
        category: 'Notes',
        isAvailable: hasVault,
        run: async () => {
          const { path } = await api.openDailyNote(-1);
          await workspace().openFile(path);
        },
      },
      {
        id: 'note.insertTemplate',
        name: 'Insert a template',
        category: 'Notes',
        defaultHotkey: 'Mod+Shift+T',
        isAvailable: hasNote,
        run: () => actions.insertTemplate(),
      },
      {
        id: 'view.toggleLeftSidebar',
        name: 'Toggle the left sidebar',
        category: 'View',
        defaultHotkey: 'Mod+B',
        run: () => workspace().toggleSidebar('left'),
      },
      {
        id: 'view.toggleRightSidebar',
        name: 'Toggle the right sidebar',
        category: 'View',
        defaultHotkey: 'Mod+Shift+B',
        run: () => workspace().toggleSidebar('right'),
      },
      {
        id: 'view.graph',
        name: 'Open the graph',
        category: 'View',
        defaultHotkey: 'Mod+G',
        isAvailable: hasVault,
        run: () => actions.openGraph(),
      },
      {
        id: 'view.toggleTheme',
        name: 'Switch between light and dark',
        category: 'View',
        run: () => {
          const current = settings().appearance.theme;
          const next = current === 'dark' ? 'light' : current === 'light' ? 'system' : 'dark';
          void settings().update('appearance', { theme: next });
          notify('info', `Theme: ${next}.`);
        },
      },
      {
        id: 'view.readingMode',
        name: 'Switch between editing and reading',
        category: 'View',
        defaultHotkey: 'Mod+Shift+R',
        isAvailable: hasNote,
        run: () => {
          const state = workspace();
          const pane = findActivePane(state.layout);
          const tab = pane?.tabs.find((candidate) => candidate.id === pane.activeTabId);
          if (tab) state.setTabMode(tab.id, tab.mode === 'read' ? 'edit' : 'read');
        },
      },
      {
        id: 'pane.splitVertical',
        name: 'Split right',
        category: 'Panes',
        defaultHotkey: 'Mod+\\',
        isAvailable: hasVault,
        run: () => workspace().splitActivePane('vertical'),
      },
      {
        id: 'pane.splitHorizontal',
        name: 'Split down',
        category: 'Panes',
        defaultHotkey: 'Mod+Shift+\\',
        isAvailable: hasVault,
        run: () => workspace().splitActivePane('horizontal'),
      },
      {
        id: 'pane.nextTab',
        name: 'Next tab',
        category: 'Panes',
        defaultHotkey: 'Mod+Tab',
        run: () => workspace().cycleTab(1),
      },
      {
        id: 'pane.previousTab',
        name: 'Previous tab',
        category: 'Panes',
        defaultHotkey: 'Mod+Shift+Tab',
        run: () => workspace().cycleTab(-1),
      },
      {
        id: 'pane.nextPane',
        name: 'Next pane',
        category: 'Panes',
        defaultHotkey: 'Mod+Alt+ArrowRight',
        run: () => workspace().cyclePane(1),
      },
      {
        id: 'pane.closeTab',
        name: 'Close this tab',
        category: 'Panes',
        defaultHotkey: 'Mod+W',
        isAvailable: hasNote,
        run: () => {
          const pane = findActivePane(workspace().layout);
          if (pane?.activeTabId) void workspace().closeTab(pane.activeTabId);
        },
      },
      {
        id: 'vault.open',
        name: 'Open another vault',
        category: 'Vault',
        run: async () => {
          const folder = await pickFolder({ title: 'Open a vault' });
          if (folder) await vault().open(folder);
        },
      },
      {
        id: 'vault.close',
        name: 'Close this vault',
        category: 'Vault',
        isAvailable: hasVault,
        run: () => vault().close(),
      },
      {
        id: 'vault.rebuildIndex',
        name: 'Rebuild the search index',
        category: 'Vault',
        isAvailable: hasVault,
        run: async () => {
          await vault().rebuildIndex();
          notify('info', 'Rebuilding the index from your notes.');
        },
      },
      {
        id: 'vault.settings',
        name: 'Settings',
        category: 'General',
        defaultHotkey: 'Mod+,',
        run: () => actions.openSettings(),
      },
      {
        id: 'workspace.reset',
        name: 'Reset the layout',
        category: 'Panes',
        isAvailable: hasVault,
        run: async () => {
          await workspace().reset();
          notify('info', 'Layout reset.');
        },
      },
      {
        id: 'editor.focus',
        name: 'Focus the editor',
        category: 'View',
        defaultHotkey: 'Escape',
        isAvailable: hasNote,
        run: () => actions.focusEditor(),
      },
    ];

    return commands.registerAll(list);
  }, [actions]);
}

function findActivePane(layout: PaneLayout) {
  const walk = (node: PaneNode): Extract<PaneNode, { type: 'leaf' }> | null => {
    if (node.type === 'leaf') return node.id === layout.activePaneId ? node : null;
    return walk(node.first) ?? walk(node.second);
  };
  return walk(layout.root);
}
