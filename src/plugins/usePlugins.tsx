/**
 * Connecting the plugin host to the running application.
 *
 * The bridges below are the only way a plugin touches the workspace or the
 * interface. Keeping them here, rather than handing plugins the stores, means
 * the surface a plugin sees is a deliberate choice rather than whatever
 * happens to be exported.
 */

import { useEffect, useMemo, useRef, useState } from 'react';

import type { VaultPath } from '@/types/domain';

import { PluginManager, type LoadedPlugin, type UiBridge, type WorkspaceBridge } from './manager';

/** A panel a plugin added to a sidebar. */
export interface PluginPanel {
  pluginId: string;
  id: string;
  label: string;
  icon: string;
  side: 'left' | 'right';
  render: (container: HTMLElement) => void | (() => void);
}

export interface PluginStatusItem {
  pluginId: string;
  id: string;
  render: (container: HTMLElement) => void;
}

export interface PluginHost {
  manager: PluginManager;
  plugins: LoadedPlugin[];
  panels: PluginPanel[];
  statusItems: PluginStatusItem[];
  /** A confirmation a plugin asked for, awaiting an answer. */
  pendingConfirm: { title: string; message: string; resolve: (answer: boolean) => void } | null;
}

export interface UsePluginsOptions {
  enabled: boolean;
  workspace: WorkspaceBridge;
  appVersion: string;
}

export function usePlugins({ enabled, workspace, appVersion }: UsePluginsOptions): PluginHost {
  const [plugins, setPlugins] = useState<LoadedPlugin[]>([]);
  const [panels, setPanels] = useState<PluginPanel[]>([]);
  const [statusItems, setStatusItems] = useState<PluginStatusItem[]>([]);
  const [pendingConfirm, setPendingConfirm] = useState<PluginHost['pendingConfirm']>(null);

  // The bridge reads through a ref, so a plugin always sees the current
  // workspace rather than the one that existed when it loaded.
  const workspaceRef = useRef(workspace);
  workspaceRef.current = workspace;

  const manager = useMemo(() => {
    const workspaceBridge: WorkspaceBridge = {
      activeFile: () => workspaceRef.current.activeFile(),
      activeContent: () => workspaceRef.current.activeContent(),
      setActiveContent: (content) => workspaceRef.current.setActiveContent(content),
      insertAtCursor: (text) => workspaceRef.current.insertAtCursor(text),
      openFile: (path: VaultPath, options) => workspaceRef.current.openFile(path, options),
    };

    const uiBridge: UiBridge = {
      addPanel: (panel) => {
        setPanels((current) => [...current, panel]);
        return {
          dispose: () =>
            setPanels((current) =>
              current.filter(
                (candidate) =>
                  !(candidate.pluginId === panel.pluginId && candidate.id === panel.id),
              ),
            ),
        };
      },
      addStatusBarItem: (item) => {
        setStatusItems((current) => [...current, item]);
        return {
          dispose: () =>
            setStatusItems((current) =>
              current.filter(
                (candidate) => !(candidate.pluginId === item.pluginId && candidate.id === item.id),
              ),
            ),
        };
      },
      confirm: (title, message) =>
        new Promise<boolean>((resolve) => {
          setPendingConfirm({
            title,
            message,
            resolve: (answer) => {
              setPendingConfirm(null);
              resolve(answer);
            },
          });
        }),
    };

    return new PluginManager(workspaceBridge, uiBridge, appVersion);
  }, [appVersion]);

  useEffect(() => manager.subscribe(() => setPlugins(manager.list())), [manager]);

  useEffect(() => {
    if (!enabled) {
      void manager.unloadAll();
      setPanels([]);
      setStatusItems([]);
      return;
    }
    void manager.loadAll();
    return () => {
      void manager.unloadAll();
    };
  }, [enabled, manager]);

  return { manager, plugins, panels, statusItems, pendingConfirm };
}

/**
 * Mount a plugin's panel.
 *
 * The plugin owns the element's contents; React owns the element itself. The
 * cleanup a plugin returns runs before the element is emptied, so it can
 * detach anything it attached.
 */
export function PluginPanelHost({ panel }: { panel: PluginPanel }) {
  const container = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const element = container.current;
    if (!element) return;
    let cleanup: void | (() => void);
    try {
      cleanup = panel.render(element);
    } catch (error) {
      console.error(`The panel "${panel.id}" failed to render:`, error);
      element.textContent = 'This panel could not be shown.';
    }
    return () => {
      try {
        cleanup?.();
      } catch (error) {
        console.error(`The panel "${panel.id}" failed to clean up:`, error);
      }
      element.replaceChildren();
    };
  }, [panel]);

  return <div className="ie-plugin-panel" ref={container} />;
}
