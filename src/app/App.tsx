/**
 * The application shell.
 *
 * Holds the layout, wires the stores to the panels, and owns the handful of
 * cross-cutting concerns that genuinely belong at the top: which modal is
 * open, where a link click should land, and flushing unsaved work when the
 * window closes.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { CommandPalette, type PaletteMode } from '@/commands/CommandPalette';
import { BacklinksPanel } from '@/components/BacklinksPanel';
import { ConfirmDialog, Modal } from '@/components/Modal';
import { Notifications, notify } from '@/components/Notifications';
import { OutlinePanel } from '@/components/OutlinePanel';
import { PropertiesPanel } from '@/components/PropertiesPanel';
import { TagPanel } from '@/components/TagPanel';
import { FileExplorer } from '@/explorer/FileExplorer';
import { DEFAULT_GRAPH_SETTINGS, GraphView, type GraphSettings } from '@/graph/GraphView';
import { PluginPanelHost, usePlugins } from '@/plugins/usePlugins';
import { SearchPanel } from '@/search/SearchPanel';
import { EmptyState, Rail } from '@/ui';
import { api } from '@/services/api';
import { events } from '@/services/events';
import { useSettingsStore } from '@/state/settingsStore';
import { useVaultStore } from '@/state/vaultStore';
import { flushPendingWrites, useWorkspaceStore } from '@/state/workspaceStore';
import type { Diagnostic, PaneLayout, PaneNode, Property, VaultPath } from '@/types/domain';
import { VAULT_ROOT, asVaultPath, joinPath, pathFileName, pathParent, pathStem } from '@/types/domain';
import * as tree from '@/workspace/paneTree';
import { SplitContainer } from '@/workspace/SplitContainer';
import { TabBar } from '@/workspace/TabBar';

import { PaneContent } from './PaneContent';
import { RecoveryPrompt } from './RecoveryPrompt';
import { SettingsDialog } from './SettingsDialog';
import { Sidebar } from './Sidebar';
import { StatusBar } from './StatusBar';
import { Toolbar } from './Toolbar';
import { NARROW_QUERY, useMediaQuery } from '@/hooks/useMediaQuery';
import { useAppCommands, type CommandActions } from './useAppCommands';
import { useHotkeys } from './useHotkeys';
import { VaultChooser } from './VaultChooser';

type Dialog =
  | { kind: 'none' }
  | { kind: 'palette'; mode: PaletteMode }
  | { kind: 'settings' }
  | { kind: 'rename'; path: VaultPath }
  | { kind: 'confirmDelete'; path: VaultPath }
  | { kind: 'createNote'; folder: VaultPath }
  | { kind: 'createFolder'; folder: VaultPath }
  | { kind: 'template' }
  | { kind: 'diagnostics' };

export function App() {
  const vault = useVaultStore();
  const workspace = useWorkspaceStore();
  const settings = useSettingsStore();
  const narrow = useMediaQuery(NARROW_QUERY);

  const [dialog, setDialog] = useState<Dialog>({ kind: 'none' });
  const [searchQuery, setSearchQuery] = useState<string | undefined>(undefined);
  const [cursor, setCursor] = useState({ line: 1, column: 1 });
  const [graphSettings, setGraphSettings] = useState<GraphSettings>(DEFAULT_GRAPH_SETTINGS);
  const [resolvedTargets, setResolvedTargets] = useState<Set<string>>(() => new Set());
  const [assetUrls, setAssetUrls] = useState<Record<string, string>>({});
  const [activeProperties, setActiveProperties] = useState<Property[]>([]);
  const pendingJump = useRef<number | null>(null);
  const activePathRef = useRef<VaultPath | null>(null);

  const isOpen = vault.status !== 'closed' && vault.status !== 'opening';
  const activePane = useMemo(() => findPane(workspace.layout), [workspace.layout]);
  const activeTab = activePane
    ? (activePane.tabs.find((tab) => tab.id === activePane.activeTabId) ?? null)
    : null;
  const activePath = activeTab?.path ?? null;
  const activeBuffer = activePath ? workspace.buffers[activePath] : undefined;

  activePathRef.current = activeTab?.path ?? null;

  // Plugins see the workspace through this bridge and nothing else, so what
  // they can reach is a deliberate list rather than whatever is exported.
  const pluginHost = usePlugins({
    enabled: vault.status === 'ready',
    appVersion: APP_VERSION,
    workspace: {
      activeFile: () => activePathRef.current,
      activeContent: () => {
        const path = activePathRef.current;
        return path ? (useWorkspaceStore.getState().buffers[path]?.content ?? null) : null;
      },
      setActiveContent: (content) => {
        const path = activePathRef.current;
        if (path) useWorkspaceStore.getState().editBuffer(path, content);
      },
      insertAtCursor: (text) => {
        // Dispatched rather than reaching into the editor, so the pane tree
        // stays free of editor references.
        window.dispatchEvent(new CustomEvent('ie:insert-text', { detail: { text } }));
      },
      openFile: async (path, options) => {
        await useWorkspaceStore.getState().openFile(path);
        if (options?.newPane) useWorkspaceStore.getState().splitActivePane('vertical');
      },
    },
  });

  const dirtyPaths = useMemo(
    () =>
      new Set(
        Object.values(workspace.buffers)
          .filter((buffer) => buffer.dirty)
          .map((buffer) => buffer.path),
      ),
    [workspace.buffers],
  );

  // Titles for the tab strip, kept in a map so a tab does not fetch per render.
  const [titles, setTitles] = useState<Record<string, string>>({});
  useEffect(() => {
    const paths = tree.allTabs(workspace.layout.root).map((tab) => tab.path);
    let cancelled = false;
    Promise.all(
      paths.map(async (path) => {
        const note = await api.readNote(path).catch(() => null);
        return [path, note?.title ?? pathStem(path)] as const;
      }),
    ).then((entries) => {
      if (!cancelled) setTitles(Object.fromEntries(entries));
    });
    return () => {
      cancelled = true;
    };
  }, [workspace.layout]);

  /** Which link targets in the active note resolve, for the editor's styling. */
  const refreshLinkState = useCallback(async () => {
    if (!activePath) {
      setResolvedTargets(new Set());
      setAssetUrls({});
      setActiveProperties([]);
      return;
    }
    try {
      const [links, note] = await Promise.all([
        api.outgoingLinks(activePath),
        api.readNote(activePath),
      ]);
      setResolvedTargets(
        new Set(links.filter((link) => link.targetPath).map((link) => link.target.toLowerCase())),
      );
      setActiveProperties(note.metadata.properties);

      // Resolve attachments to absolute paths the webview can load. Only the
      // ones this note references, so opening a note does not resolve the
      // whole vault.
      const attachments = links.filter(
        (link) => link.targetPath && !/\.(md|markdown)$/i.test(link.targetPath),
      );
      const resolved = await Promise.all(
        attachments.map(async (link) => {
          const absolute = await api.resolveAssetPath(link.targetPath!).catch(() => null);
          return absolute ? ([link.target.toLowerCase(), assetUrl(absolute)] as const) : null;
        }),
      );
      setAssetUrls(Object.fromEntries(resolved.filter((entry) => entry !== null)));
    } catch {
      setResolvedTargets(new Set());
    }
  }, [activePath]);

  useEffect(() => {
    void refreshLinkState();
  }, [refreshLinkState]);

  useEffect(() => {
    const offs = [
      events.on('indexUpdated', () => void refreshLinkState()),
      events.on('indexCompleted', () => void refreshLinkState()),
    ];
    return () => offs.forEach((off) => off());
  }, [refreshLinkState]);

  /** Open a file, optionally in a new split and at a line. */
  const openFile = useCallback(
    async (path: VaultPath, options: { newPane?: boolean; line?: number } = {}) => {
      if (options.newPane) workspace.splitActivePane('vertical');
      await workspace.openFile(path);
      if (options.line !== undefined) pendingJump.current = options.line;
    },
    [workspace],
  );

  /** Follow a wiki link, offering to create the note when it does not exist. */
  const followLink = useCallback(
    async (target: string, newPane: boolean) => {
      if (!activePath) return;
      try {
        const resolution = await api.resolveLink(activePath, target);
        if (resolution.outcome === 'external') {
          await api.openExternal(resolution.url);
          return;
        }
        if (resolution.outcome === 'resolved') {
          await openFile(resolution.path, {
            newPane,
            ...(resolution.line !== null ? { line: resolution.line } : {}),
          });
          if (resolution.anchorMissing) {
            notify('warning', `${pathFileName(resolution.path)} has no such heading or block.`);
          }
          if (resolution.ambiguous) {
            notify('warning', `More than one note answers to "${target}".`);
          }
          return;
        }

        // Unresolved: create it, which is the whole point of writing a link to
        // a note that does not exist yet.
        const created = await api.createNoteFromLink(target, pathParent(activePath));
        await openFile(created, { newPane });
        notify('success', `Created ${pathFileName(created)}.`);
      } catch (error) {
        notify('error', message(error));
      }
    },
    [activePath, openFile],
  );

  const followTag = useCallback(
    (tag: string) => {
      setSearchQuery(`tag:${tag}`);
      workspace.setSidebar('left', { visible: true, activePanel: 'search' });
    },
    [workspace],
  );

  /**
   * Show a folder in the explorer.
   *
   * The breadcrumb needs to reach into a panel that owns its own tree state.
   * It asks through an event, the way the editor is asked to jump to a line,
   * so the shell does not hold a reference to the explorer's internals.
   */
  const revealFolder = useCallback(
    (folder: VaultPath) => {
      workspace.setSidebar('left', { visible: true, activePanel: 'files' });
      window.dispatchEvent(
        new CustomEvent('ie:reveal-path', { detail: { path: folder, folder: true } }),
      );
    },
    [workspace],
  );

  const assetUrlFor = useCallback(
    (target: string) => assetUrls[target.toLowerCase()] ?? null,
    [assetUrls],
  );

  /**
   * File a dropped or pasted attachment in the vault and return the link.
   *
   * The bytes go through the backend rather than being written from here,
   * because that is where the attachment-folder setting, name sanitising and
   * the atomic write live.
   */
  const importFile = useCallback(
    async (file: File, note: VaultPath): Promise<string | null> => {
      try {
        const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
        const stored = await api.importAttachment(file.name, bytes, note);
        await refreshLinkState();
        const isImage = /\.(png|jpe?g|gif|webp|svg|bmp|avif)$/i.test(stored);
        // An image is embedded so it shows; anything else is linked so the
        // note stays readable.
        return isImage ? `![[${pathFileName(stored)}]]` : `[[${pathFileName(stored)}]]`;
      } catch (error) {
        notify('error', `Could not file that attachment: ${message(error)}`);
        return null;
      }
    },
    [refreshLinkState],
  );

  const actions = useMemo<CommandActions>(
    () => ({
      openPalette: (mode) => setDialog({ kind: 'palette', mode }),
      openSearch: (query) => {
        setSearchQuery(query ?? '');
        workspace.setSidebar('left', { visible: true, activePanel: 'search' });
      },
      openSettings: () => setDialog({ kind: 'settings' }),
      openGraph: () => {
        void workspace.openFile(asVaultPath('graph:vault'), { mode: 'graph' });
      },
      createNote: () => setDialog({ kind: 'createNote', folder: folderOf(activePath) }),
      createFolder: () => setDialog({ kind: 'createFolder', folder: folderOf(activePath) }),
      renameActive: () => activePath && setDialog({ kind: 'rename', path: activePath }),
      deleteActive: () => activePath && setDialog({ kind: 'confirmDelete', path: activePath }),
      insertTemplate: () => setDialog({ kind: 'template' }),
      activePath: () => activePath,
      focusEditor: () => {
        document.querySelector<HTMLElement>('.ie-editor .cm-content')?.focus();
      },
      exportActive: (format) => activePath && void exportNote(activePath, format ?? 'html'),
      printActive: () => activePath && void printNote(activePath, titles[activePath] ?? ''),
      importFiles: () => void importIntoVault(folderOf(activePath)),
    }),
    [activePath, titles, workspace],
  );

  useAppCommands(actions);
  useHotkeys(settings.hotkeys, dialog.kind !== 'settings');

  // Flush unsaved work before the window goes away. The backend also writes
  // atomically, so the worst case here is losing the last few keystrokes
  // rather than a damaged file.
  useEffect(() => {
    const onBeforeUnload = () => {
      void flushPendingWrites();
    };
    window.addEventListener('beforeunload', onBeforeUnload);
    return () => window.removeEventListener('beforeunload', onBeforeUnload);
  }, []);

  if (!isOpen) {
    return (
      <>
        <VaultChooser />
        <Notifications />
      </>
    );
  }

  const renderLeaf = (leaf: Extract<PaneNode, { type: 'leaf' }>) => {
    const tab = leaf.tabs.find((candidate) => candidate.id === leaf.activeTabId) ?? leaf.tabs[0];
    const buffer = tab ? workspace.buffers[tab.path] : undefined;

    return (
      <section
        className={`ie-pane${leaf.id === workspace.layout.activePaneId ? ' is-active' : ''}`}
        key={leaf.id}
        onFocus={() => workspace.setActivePane(leaf.id)}
      >
        {settings.appearance.showTabBar ? (
          <TabBar
            paneId={leaf.id}
            tabs={leaf.tabs}
            activeTabId={leaf.activeTabId}
            isActivePane={leaf.id === workspace.layout.activePaneId}
            dirtyPaths={dirtyPaths}
            titleOf={(path) => titles[path] ?? pathStem(path)}
            onSelect={workspace.setActiveTab}
            onClose={(tabId) => void workspace.closeTab(tabId)}
            onCloseOthers={workspace.closeOthers}
            onCloseAll={() => workspace.closeAllInPane(leaf.id)}
            onTogglePin={workspace.togglePin}
            onDuplicate={workspace.duplicateTab}
            onReorder={workspace.reorderTab}
            onMoveToPane={workspace.moveTabToPane}
            onSplit={(direction) => {
              workspace.setActivePane(leaf.id);
              workspace.splitActivePane(direction);
            }}
            onFocusPane={() => workspace.setActivePane(leaf.id)}
          />
        ) : null}

        <div className="ie-pane__body">
          {tab ? (
            <PaneContent
              tab={tab}
              buffer={buffer}
              loading={Boolean(workspace.loading[tab.path])}
              preferences={settings.editor}
              graphSettings={graphSettings}
              onGraphSettingsChange={setGraphSettings}
              activePath={activePath}
              onEdit={(content) => workspace.editBuffer(tab.path, content)}
              onSave={() => void workspace.saveBuffer(tab.path)}
              onCursorChange={(position) => {
                setCursor({ line: position.line, column: position.column });
                workspace.rememberPosition(tab.id, {
                  cursorOffset: position.offset,
                  scrollLine: position.line,
                });
              }}
              onFollowLink={(target, newPane) => void followLink(target, newPane)}
              onFollowTag={followTag}
              onOpen={(path, options) => void openFile(path, options ?? {})}
              resolvedTargets={resolvedTargets}
              assetUrl={assetUrlFor}
              onImportFile={importFile}
            />
          ) : (
            <div className="ie-empty">
              Nothing open in this pane. Press Ctrl+O to find a note.
            </div>
          )}
        </div>
      </section>
    );
  };

  /** Panels for the left rail, in the order they appear. */
  const leftPanels = [
    {
      id: 'files',
      label: 'Files',
      icon: 'folder' as const,
      render: () => (
        <FileExplorer
          activePath={activePath}
          onOpen={(path, options) => void openFile(path, options ?? {})}
          onRename={(path) => setDialog({ kind: 'rename', path })}
          onDelete={(path) => setDialog({ kind: 'confirmDelete', path })}
          onCreateNote={(folder) => setDialog({ kind: 'createNote', folder })}
          onCreateFolder={(folder) => setDialog({ kind: 'createFolder', folder })}
        />
      ),
    },
    {
      id: 'search',
      label: 'Search',
      icon: 'search' as const,
      render: () => (
        <SearchPanel
          initialQuery={searchQuery}
          onOpen={(path, options) => void openFile(path, options ?? {})}
        />
      ),
    },
    {
      id: 'tags',
      label: 'Tags',
      icon: 'hash' as const,
      render: () => <TagPanel onSelectTag={followTag} />,
    },
    ...pluginHost.panels
      .filter((panel) => panel.side === 'left')
      .map((panel) => ({
        id: `${panel.pluginId}:${panel.id}`,
        label: panel.label,
        // A plugin's icon is a string from its manifest, not one of ours, so
        // it gets the generic mark rather than a broken glyph.
        icon: 'grid' as const,
        render: () => <PluginPanelHost panel={panel} />,
      })),
  ];

  const rightPanels = [
    {
      id: 'backlinks',
      label: 'Backlinks',
      icon: 'corner-down-left' as const,
      render: () => (
        <BacklinksPanel
          path={activePath}
          onOpen={(path, options) => void openFile(path, options ?? {})}
        />
      ),
    },
    {
      id: 'outline',
      label: 'Outline',
      icon: 'list' as const,
      render: () => (
        <OutlinePanel
          path={activePath}
          currentLine={cursor.line - 1}
          onJump={(line) => {
            pendingJump.current = line;
            jumpToLine(line);
          }}
        />
      ),
    },
    {
      id: 'properties',
      label: 'Properties',
      icon: 'settings' as const,
      render: () => (
        <PropertiesPanel
          path={activePath}
          properties={activeProperties}
          onChanged={() => {
            if (activePath) void workspace.reloadBuffer(activePath);
            void refreshLinkState();
          }}
        />
      ),
    },
    {
      id: 'localGraph',
      label: 'Local graph',
      icon: 'graph' as const,
      render: () =>
        activePath ? (
          <GraphView
            compact
            centerPath={activePath}
            activePath={activePath}
            settings={graphSettings}
            onSettingsChange={setGraphSettings}
            onOpen={(path, options) => void openFile(path, options ?? {})}
          />
        ) : (
          <EmptyState
            compact
            icon="graph"
            title="Nothing to draw"
            description="Open a note to see what links to it and where it leads."
          />
        ),
    },
    ...pluginHost.panels
      .filter((panel) => panel.side === 'right')
      .map((panel) => ({
        id: `${panel.pluginId}:${panel.id}`,
        label: panel.label,
        icon: 'grid' as const,
        render: () => <PluginPanelHost panel={panel} />,
      })),
  ];

  /**
   * Choosing from a rail.
   *
   * Choosing the panel that is already showing collapses the sidebar, which
   * is what a rail does everywhere else and saves a separate toggle button.
   */
  const selectPanel = (side: 'left' | 'right', id: string) => {
    const sidebar = side === 'left' ? workspace.leftSidebar : workspace.rightSidebar;
    if (sidebar.visible && sidebar.activePanel === id) workspace.setSidebar(side, { visible: false });
    else workspace.setSidebar(side, { visible: true, activePanel: id });
    // On a narrow window the sidebars overlay the editor, so two open at once
    // would cover it entirely.
    if (narrow) {
      const other = side === 'left' ? 'right' : 'left';
      workspace.setSidebar(other, { visible: false });
    }
  };

  return (
    <div className="ie-app">
      <Toolbar
        vaultName={vault.info?.settings.name ?? null}
        activePath={activePath}
        activeTitle={activePath ? (titles[activePath] ?? null) : null}
        leftSidebarVisible={workspace.leftSidebar.visible}
        rightSidebarVisible={workspace.rightSidebar.visible}
        onToggleSidebar={(side) => workspace.toggleSidebar(side)}
        canGoBack={workspace.historyIndex > 0}
        canGoForward={workspace.historyIndex < workspace.history.length - 1}
        onBack={() => void workspace.goBack()}
        onForward={() => void workspace.goForward()}
        onRevealFolder={revealFolder}
        onSearch={(query) => {
          setSearchQuery(query);
          workspace.setSidebar('left', { visible: true, activePanel: 'search' });
        }}
        onNewNote={() => setDialog({ kind: 'createNote', folder: folderOf(activePath) })}
        onQuickSwitch={() => setDialog({ kind: 'palette', mode: 'files' })}
        onCommandPalette={() => setDialog({ kind: 'palette', mode: 'commands' })}
        onSettings={() => setDialog({ kind: 'settings' })}
      />

      <div className="ie-app__main">
        <Rail
          side="left"
          items={leftPanels.map(({ id, label, icon }) => ({ id, label, icon }))}
          activeId={workspace.leftSidebar.visible ? workspace.leftSidebar.activePanel : null}
          onSelect={(id) => selectPanel('left', id)}
        />

        {workspace.leftSidebar.visible ? (
          <Sidebar
            side="left"
            width={workspace.leftSidebar.width}
            activePanel={workspace.leftSidebar.activePanel}
            panels={leftPanels}
            onResize={(width) => workspace.setSidebar('left', { width })}
          />
        ) : null}

        <main className="ie-app__panes">
          <SplitContainer
            node={workspace.layout.root}
            renderLeaf={renderLeaf}
            onResize={workspace.resizeSplit}
          />
        </main>

        {/* Only a drawer needs putting away, and only the CSS knows when a
            sidebar is one — so this is rendered whenever a sidebar is open and
            the narrow breakpoint hides it the rest of the time. */}
        {workspace.leftSidebar.visible || workspace.rightSidebar.visible ? (
          <button
            type="button"
            className="ie-app__scrim"
            aria-label="Close the sidebar"
            tabIndex={narrow ? 0 : -1}
            onClick={() => {
              workspace.setSidebar('left', { visible: false });
              workspace.setSidebar('right', { visible: false });
            }}
          />
        ) : null}

        {workspace.rightSidebar.visible ? (
          <Sidebar
            side="right"
            width={workspace.rightSidebar.width}
            activePanel={workspace.rightSidebar.activePanel}
            panels={rightPanels}
            onResize={(width) => workspace.setSidebar('right', { width })}
          />
        ) : null}

        <Rail
          side="right"
          items={rightPanels.map(({ id, label, icon }) => ({ id, label, icon }))}
          activeId={workspace.rightSidebar.visible ? workspace.rightSidebar.activePanel : null}
          onSelect={(id) => selectPanel('right', id)}
        />
      </div>

      {settings.appearance.showStatusBar ? (
        <StatusBar
          path={activePath}
          buffer={activeBuffer}
          cursor={cursor}
          indexing={vault.index}
          vaultName={vault.info?.settings.name ?? null}
          fileCount={vault.info?.fileCount ?? 0}
          diagnosticCount={vault.diagnostics.length}
          onOpenDiagnostics={() => setDialog({ kind: 'diagnostics' })}
        />
      ) : null}

      <Dialogs
        dialog={dialog}
        onClose={() => setDialog({ kind: 'none' })}
        activePath={activePath}
        openFile={(path, options) => void openFile(path, options ?? {})}
        hotkeys={settings.hotkeys}
      />

      {pluginHost.pendingConfirm ? (
        <ConfirmDialog
          title={pluginHost.pendingConfirm.title}
          message={pluginHost.pendingConfirm.message}
          onCancel={() => pluginHost.pendingConfirm?.resolve(false)}
          onConfirm={() => pluginHost.pendingConfirm?.resolve(true)}
        />
      ) : null}

      <RecoveryPrompt vaultReady={vault.status === 'ready'} />

      <Notifications />
    </div>
  );
}

/** Reported to plugins so they can check compatibility. */
const APP_VERSION = '0.1.0';

/** Every modal, kept out of the shell so the shell reads as a layout. */
function Dialogs({
  dialog,
  onClose,
  activePath,
  openFile,
  hotkeys,
}: {
  dialog: Dialog;
  onClose: () => void;
  activePath: VaultPath | null;
  openFile: (path: VaultPath, options?: { newPane?: boolean }) => void;
  hotkeys: Record<string, string>;
}) {
  const vault = useVaultStore();
  const [name, setName] = useState('');

  useEffect(() => {
    if (dialog.kind === 'rename') setName(pathFileName(dialog.path));
    else setName('');
  }, [dialog]);

  switch (dialog.kind) {
    case 'palette':
      return (
        <CommandPalette
          mode={dialog.mode}
          hotkeys={hotkeys}
          onClose={onClose}
          onOpenFile={openFile}
        />
      );

    case 'settings':
      return <SettingsDialog onClose={onClose} />;

    case 'rename':
      return (
        <Modal
          title="Rename"
          description="Every link pointing at this note will be updated."
          onClose={onClose}
          size="small"
          footer={
            <>
              <button type="button" className="ie-button" onClick={onClose}>
                Cancel
              </button>
              <button
                type="button"
                className="ie-button ie-button--primary"
                onClick={async () => {
                  const target = joinPath(pathParent(dialog.path), name.trim());
                  onClose();
                  try {
                    await api.renameEntry(dialog.path, target);
                  } catch (error) {
                    notify('error', message(error));
                  }
                }}
              >
                Rename
              </button>
            </>
          }
        >
          <input
            className="ie-input"
            autoFocus
            value={name}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && event.currentTarget.blur()}
          />
        </Modal>
      );

    case 'confirmDelete':
      return (
        <ConfirmDialog
          title="Move to trash"
          message={`${pathFileName(dialog.path)} will move to the vault's trash folder. You can restore it from Settings.`}
          confirmLabel="Move to trash"
          danger
          onCancel={onClose}
          onConfirm={async () => {
            onClose();
            try {
              await api.deleteEntry(dialog.path);
              notify('info', `Moved ${pathFileName(dialog.path)} to the trash.`);
            } catch (error) {
              notify('error', message(error));
            }
          }}
        />
      );

    case 'createNote':
    case 'createFolder': {
      const isNote = dialog.kind === 'createNote';
      return (
        <Modal
          title={isNote ? 'New note' : 'New folder'}
          description={dialog.folder ? `In ${dialog.folder}` : 'At the top of the vault'}
          onClose={onClose}
          size="small"
          footer={
            <>
              <button type="button" className="ie-button" onClick={onClose}>
                Cancel
              </button>
              <button
                type="button"
                className="ie-button ie-button--primary"
                onClick={async () => {
                  const trimmed = name.trim();
                  if (!trimmed) return;
                  onClose();
                  try {
                    if (isNote) {
                      const path = joinPath(
                        dialog.folder,
                        trimmed.endsWith('.md') ? trimmed : `${trimmed}.md`,
                      );
                      const created = await api.createNote(path, `# ${trimmed}\n\n`);
                      openFile(created);
                    } else {
                      await api.createFolder(joinPath(dialog.folder, trimmed));
                    }
                  } catch (error) {
                    notify('error', message(error));
                  }
                }}
              >
                Create
              </button>
            </>
          }
        >
          <input
            className="ie-input"
            autoFocus
            placeholder={isNote ? 'Note name' : 'Folder name'}
            value={name}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && event.currentTarget.blur()}
          />
        </Modal>
      );
    }

    case 'template':
      return <TemplatePicker activePath={activePath} onClose={onClose} />;

    case 'diagnostics':
      return (
        <Modal title="Scan notices" onClose={onClose} size="large">
          {vault.diagnostics.length === 0 ? (
            <p>Nothing to report. The last scan found no problems.</p>
          ) : (
            <ul className="ie-diagnostics">
              {vault.diagnostics.map((diagnostic, index) => (
                <li key={index} className={`ie-diagnostic ie-diagnostic--${diagnostic.kind}`}>
                  {describeDiagnostic(diagnostic)}
                </li>
              ))}
            </ul>
          )}
        </Modal>
      );

    default:
      return null;
  }
}

function TemplatePicker({
  activePath,
  onClose,
}: {
  activePath: VaultPath | null;
  onClose: () => void;
}) {
  const workspace = useWorkspaceStore();
  const [templates, setTemplates] = useState<{ path: VaultPath; name: string }[]>([]);

  useEffect(() => {
    api
      .listTemplates()
      .then(setTemplates)
      .catch(() => setTemplates([]));
  }, []);

  return (
    <Modal title="Insert a template" onClose={onClose} size="small">
      {templates.length === 0 ? (
        <p>
          No templates yet. Put Markdown files in the vault&apos;s template folder and they will
          appear here.
        </p>
      ) : (
        <ul className="ie-template-list">
          {templates.map((template) => (
            <li key={template.path}>
              <button
                type="button"
                className="ie-button ie-button--quiet"
                onClick={async () => {
                  if (!activePath) return;
                  onClose();
                  try {
                    const rendered = await api.renderTemplate(template.path, activePath);
                    const buffer = workspace.buffers[activePath];
                    workspace.editBuffer(activePath, `${buffer?.content ?? ''}${rendered}`);
                  } catch (error) {
                    notify('error', message(error));
                  }
                }}
              >
                {template.name}
              </button>
            </li>
          ))}
        </ul>
      )}
    </Modal>
  );
}

function findPane(layout: PaneLayout) {
  return tree.findLeaf(layout.root, layout.activePaneId) ?? tree.leaves(layout.root)[0] ?? null;
}

function folderOf(path: VaultPath | null): VaultPath {
  return path ? pathParent(path) : VAULT_ROOT;
}

/** Turn an absolute path into a URL the webview may load. */
function assetUrl(absolute: string): string {
  // Tauri's asset protocol; the scope was granted for this vault when it
  // opened, so nothing outside it can be addressed this way.
  return `asset://localhost/${encodeURIComponent(absolute)}`;
}

function jumpToLine(line: number): void {
  // The editor listens for this rather than being reached into directly, which
  // keeps the pane tree free of editor refs.
  window.dispatchEvent(new CustomEvent('ie:jump-to-line', { detail: { line } }));
}

async function exportNote(
  path: VaultPath,
  format: 'html' | 'markdown',
): Promise<void> {
  const { pickFolder } = await import('@/services/dialogs');
  const destination = await pickFolder({ title: 'Export to' });
  if (!destination) return;
  try {
    const result = await api.exportNotes([path], destination, { format });
    notify('success', `Exported ${result.files.length} ${result.files.length === 1 ? 'file' : 'files'}.`);
    if (result.unresolvedLinks.length > 0) {
      notify(
        'warning',
        `${result.unresolvedLinks.length} link${result.unresolvedLinks.length === 1 ? '' : 's'} pointed at notes that do not exist and were left as plain text.`,
      );
    }
  } catch (error) {
    notify('error', message(error));
  }
}

/**
 * Print the note, which is also how a PDF is produced.
 *
 * The webview's own print dialogue offers "save as PDF" on both platforms, so
 * this reuses the renderer the reading view already uses rather than shipping
 * a second PDF engine. What the user gets is what they were reading.
 */
async function printNote(path: VaultPath, title: string): Promise<void> {
  try {
    const note = await api.readNote(path);
    const frame = document.createElement('iframe');
    frame.style.position = 'fixed';
    frame.style.right = '100%';
    frame.style.width = '0';
    frame.style.height = '0';
    frame.setAttribute('aria-hidden', 'true');
    document.body.appendChild(frame);

    const view = frame.contentDocument;
    if (!view) {
      frame.remove();
      notify('error', 'Could not prepare the note for printing.');
      return;
    }

    // Render through the same pipeline the reading view uses, then take the
    // resulting markup. Links become plain text: a printed page has nowhere
    // for them to go.
    const holder = document.createElement('div');
    holder.className = 'ie-reading__body';
    document.body.appendChild(holder);
    const { createRoot } = await import('react-dom/client');
    const root = createRoot(holder);
    const { renderMarkdown } = await import('@/markdown/renderer');
    root.render(
      renderMarkdown(note.content, {
        path,
        onFollowLink: () => {},
        onFollowTag: () => {},
        onFollowExternal: () => {},
        resolveAsset: () => null,
      }),
    );

    // One frame for React to commit before the markup is read.
    await new Promise((resolve) => requestAnimationFrame(resolve));
    const body = holder.innerHTML;
    root.unmount();
    holder.remove();

    view.open();
    view.write(
      `<!doctype html><html><head><meta charset="utf-8"><title>${escapeHtml(
        title || pathFileName(path),
      )}</title><style>
        body { font: 12pt/1.6 Georgia, "Times New Roman", serif; margin: 2cm; color: #111; }
        h1, h2, h3 { line-height: 1.25; }
        pre, code { font-family: "SFMono-Regular", Consolas, monospace; font-size: 10pt; }
        pre { background: #f4f4f4; padding: 0.6em; border-radius: 4px; overflow-x: auto; }
        blockquote { border-left: 3px solid #ccc; margin-left: 0; padding-left: 1em; color: #444; }
        table { border-collapse: collapse; width: 100%; }
        th, td { border: 1px solid #ccc; padding: 4px 8px; }
        img { max-width: 100%; }
        button { all: unset; }
      </style></head><body>${body}</body></html>`,
    );
    view.close();

    frame.contentWindow?.focus();
    frame.contentWindow?.print();
    // Give the dialogue time to take the document before it is discarded.
    setTimeout(() => frame.remove(), 60_000);
  } catch (error) {
    notify('error', `Could not print: ${message(error)}`);
  }
}

async function importIntoVault(folder: VaultPath): Promise<void> {
  const { pickFiles } = await import('@/services/dialogs');
  const chosen = await pickFiles({ title: 'Import into the vault' });
  if (chosen.length === 0) return;

  try {
    const result = await api.importFiles(folder, chosen);
    notify(
      'success',
      `Imported ${result.imported.length} ${result.imported.length === 1 ? 'file' : 'files'}.`,
    );
    if (result.skipped.length > 0) {
      notify('warning', `Skipped ${result.skipped.length}: ${result.skipped.join(', ')}`);
    }
  } catch (error) {
    notify('error', `Could not import: ${message(error)}`);
  }
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function describeDiagnostic(diagnostic: Diagnostic): string {
  switch (diagnostic.kind) {
    case 'caseConflict':
      return `These names differ only by capitalisation and would collide on Windows: ${diagnostic.paths.join(', ')}`;
    case 'unportableName':
      return `${diagnostic.path} is not a portable name: ${diagnostic.reason}`;
    case 'interruptedWrite':
      return `${diagnostic.path} looks like a write that was interrupted. Your note is intact; this leftover can be deleted.`;
    case 'unreadableFile':
      return `${diagnostic.path} could not be read: ${diagnostic.message}`;
    case 'malformedFrontmatter':
      return `${diagnostic.path} has frontmatter that is not valid YAML: ${diagnostic.message}. The note is still indexed.`;
    case 'escapingSymlink':
      return `${diagnostic.path} is a link pointing outside the vault and was not followed.`;
    default:
      return 'Unrecognised notice.';
  }
}

function message(error: unknown): string {
  return error instanceof Object && 'message' in error ? String(error.message) : String(error);
}
