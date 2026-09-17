/**
 * The frontend event bus.
 *
 * Backend events arrive through Tauri's event system and are re-emitted here,
 * so components and plugins subscribe to one bus rather than two. Plugins get
 * this bus and never the Tauri one, which is what keeps them from listening to
 * channels the app did not mean to expose.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { Diagnostic, IndexProgress, VaultPath } from '@/types/domain';

export interface AppEvents {
  vaultOpened: { root: string; name: string };
  vaultClosed: undefined;
  fileCreated: { path: VaultPath };
  fileModified: { path: VaultPath };
  fileDeleted: { path: VaultPath };
  fileRenamed: { from: VaultPath; to: VaultPath };
  indexProgress: IndexProgress;
  indexCompleted: { files: number; durationMs: number; diagnostics: Diagnostic[] };
  indexUpdated: { indexed: number; removed: number };
  indexError: { message: string };
  notice: { level: 'info' | 'success' | 'warning' | 'error'; message: string };
  activeFileChanged: { path: VaultPath | null };
  workspaceChanged: undefined;
}

export type AppEventName = keyof AppEvents;
type Handler<K extends AppEventName> = (payload: AppEvents[K]) => void;

/** The channels forwarded from the backend. */
const BACKEND_EVENTS: AppEventName[] = [
  'vaultOpened',
  'vaultClosed',
  'fileCreated',
  'fileModified',
  'fileDeleted',
  'fileRenamed',
  'indexProgress',
  'indexCompleted',
  'indexUpdated',
  'indexError',
  'notice',
];

class EventBus {
  private handlers = new Map<AppEventName, Set<(payload: never) => void>>();
  private unlisteners: UnlistenFn[] = [];
  private bridged = false;

  on<K extends AppEventName>(event: K, handler: Handler<K>): () => void {
    let set = this.handlers.get(event);
    if (!set) {
      set = new Set();
      this.handlers.set(event, set);
    }
    set.add(handler as (payload: never) => void);
    return () => {
      set?.delete(handler as (payload: never) => void);
    };
  }

  once<K extends AppEventName>(event: K, handler: Handler<K>): () => void {
    const off = this.on(event, (payload) => {
      off();
      handler(payload);
    });
    return off;
  }

  emit<K extends AppEventName>(event: K, payload: AppEvents[K]): void {
    const set = this.handlers.get(event);
    if (!set) return;
    // Copy before iterating: a handler may unsubscribe itself, and a plugin's
    // handler may throw, which must not stop the others from running.
    for (const handler of [...set]) {
      try {
        (handler as Handler<K>)(payload);
      } catch (error) {
        console.error(`An "${event}" handler threw:`, error);
      }
    }
  }

  /** Start forwarding backend events. Safe to call more than once. */
  async bridge(): Promise<void> {
    if (this.bridged) return;
    this.bridged = true;
    for (const name of BACKEND_EVENTS) {
      const unlisten = await listen(name, (event) => {
        this.emit(name, event.payload as AppEvents[typeof name]);
      });
      this.unlisteners.push(unlisten);
    }
  }

  dispose(): void {
    for (const unlisten of this.unlisteners) unlisten();
    this.unlisteners = [];
    this.handlers.clear();
    this.bridged = false;
  }
}

export const events = new EventBus();
export type { EventBus };
