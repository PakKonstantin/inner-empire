/**
 * Startup.
 *
 * The order matters: settings first so the theme is right before anything
 * paints, then the event bridge so no backend message is missed, then the
 * vault if one should reopen.
 */

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from '@/app/App';
import { ContextMenuProvider } from '@/components/ContextMenu';
import { events } from '@/services/events';
import { api } from '@/services/api';
import {
  applyAppearance,
  useSettingsStore,
  watchSystemTheme,
} from '@/state/settingsStore';
import { subscribeVaultEvents, useVaultStore } from '@/state/vaultStore';
import {
  startRecoveryJournal,
  subscribeWorkspaceEvents,
  useWorkspaceStore,
} from '@/state/workspaceStore';

// Order matters: the themeable values, then the scale layer that refers to
// them, then the reset, then the design system, then the application's own
// layout — so a later rule can override an earlier one without `!important`.
import '@/styles/theme.css';
import '@/styles/tokens.css';
import '@/styles/base.css';
import '@/styles/components.css';
import '@/styles/layout.css';

async function start(): Promise<void> {
  const settings = useSettingsStore.getState();
  await settings.load();
  applyAppearance(useSettingsStore.getState());
  watchSystemTheme();

  await events.bridge();
  subscribeVaultEvents();
  subscribeWorkspaceEvents();
  // Unsaved text is noted down periodically, so a crash costs seconds rather
  // than everything since the last autosave.
  startRecoveryJournal();

  // The workspace is loaded whenever a vault opens, including one reopened at
  // startup, so the same path covers both.
  events.on('vaultOpened', () => {
    void useWorkspaceStore.getState().hydrate();
  });

  const vault = useVaultStore.getState();
  await vault.loadRecents();

  if (settings.general.reopenLastVault) {
    // The backend may already hold an open vault after a reload in development.
    const alreadyOpen = await api.isVaultOpen().catch(() => false);
    if (alreadyOpen) {
      await vault.refreshInfo();
      useVaultStore.setState({ status: 'ready' });
      await useWorkspaceStore.getState().hydrate();
    } else {
      const [mostRecent] = useVaultStore.getState().recents;
      if (mostRecent) await vault.open(mostRecent.path);
    }
  }

  const root = document.getElementById('root');
  if (!root) throw new Error('The application root element is missing from index.html.');

  createRoot(root).render(
    <StrictMode>
      <ContextMenuProvider>
        <App />
      </ContextMenuProvider>
    </StrictMode>,
  );
}

void start();
