/**
 * Which vault is open, and what the index knows about it.
 *
 * Note *content* is deliberately not here. Holding fifty thousand notes in a
 * store would defeat the point of having an index; components ask for what
 * they need and the backend answers from SQLite.
 */

import { create } from 'zustand';

import { api } from '@/services/api';
import { events } from '@/services/events';
import type { Diagnostic, RecentVault, VaultInfo, VaultSettings } from '@/types/domain';

export type VaultStatus = 'closed' | 'opening' | 'indexing' | 'ready' | 'error';

interface IndexState {
  scanned: number;
  total: number | null;
  done: boolean;
}

interface VaultState {
  status: VaultStatus;
  info: VaultInfo | null;
  recents: RecentVault[];
  diagnostics: Diagnostic[];
  index: IndexState;
  error: string | null;

  loadRecents: () => Promise<void>;
  open: (path: string) => Promise<boolean>;
  create: (path: string, name: string) => Promise<boolean>;
  close: () => Promise<void>;
  refreshInfo: () => Promise<void>;
  updateSettings: (settings: VaultSettings) => Promise<void>;
  rebuildIndex: () => Promise<void>;
  forget: (path: string) => Promise<void>;
  dismissError: () => void;
}

const IDLE_INDEX: IndexState = { scanned: 0, total: null, done: true };

export const useVaultStore = create<VaultState>((set, get) => ({
  status: 'closed',
  info: null,
  recents: [],
  diagnostics: [],
  index: IDLE_INDEX,
  error: null,

  async loadRecents() {
    try {
      set({ recents: await api.recentVaults() });
    } catch {
      // The recent list is a convenience. Failing to read it must not stop the
      // welcome screen from appearing.
      set({ recents: [] });
    }
  },

  async open(path) {
    set({ status: 'opening', error: null });
    try {
      await api.openVault(path);
      set({ status: 'indexing', index: { scanned: 0, total: null, done: false } });
      await get().refreshInfo();
      await get().loadRecents();
      return true;
    } catch (error) {
      set({
        status: 'error',
        error: error instanceof Object && 'message' in error ? String(error.message) : String(error),
      });
      return false;
    }
  },

  async create(path, name) {
    set({ status: 'opening', error: null });
    try {
      await api.createVault(path, name);
      set({ status: 'indexing', index: { scanned: 0, total: null, done: false } });
      await get().refreshInfo();
      await get().loadRecents();
      return true;
    } catch (error) {
      set({
        status: 'error',
        error: error instanceof Object && 'message' in error ? String(error.message) : String(error),
      });
      return false;
    }
  },

  async close() {
    await api.closeVault();
    set({ status: 'closed', info: null, diagnostics: [], index: IDLE_INDEX });
  },

  async refreshInfo() {
    try {
      const info = await api.vaultInfo();
      set({ info });
    } catch {
      set({ info: null });
    }
  },

  async updateSettings(settings) {
    await api.updateVaultSettings(settings);
    await get().refreshInfo();
  },

  async rebuildIndex() {
    set({ status: 'indexing', index: { scanned: 0, total: null, done: false } });
    await api.rebuildIndex();
  },

  async forget(path) {
    await api.forgetVault(path);
    await get().loadRecents();
  },

  dismissError() {
    set({ error: null });
  },
}));

/**
 * Keep the store in step with the backend.
 *
 * Called once at startup. Returns a teardown so tests and hot reloads do not
 * accumulate subscriptions.
 */
export function subscribeVaultEvents(): () => void {
  const offs = [
    events.on('indexProgress', (progress) => {
      useVaultStore.setState({
        status: 'indexing',
        index: { scanned: progress.scanned, total: progress.total, done: false },
      });
    }),

    events.on('indexCompleted', ({ files, diagnostics }) => {
      useVaultStore.setState({
        status: 'ready',
        diagnostics,
        index: { scanned: files, total: files, done: true },
      });
      void useVaultStore.getState().refreshInfo();
    }),

    events.on('indexError', ({ message }) => {
      useVaultStore.setState({ status: 'error', error: message });
    }),

    events.on('vaultClosed', () => {
      useVaultStore.setState({
        status: 'closed',
        info: null,
        diagnostics: [],
        index: IDLE_INDEX,
      });
    }),
  ];

  return () => offs.forEach((off) => off());
}
