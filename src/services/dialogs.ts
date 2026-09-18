/**
 * Native dialogs.
 *
 * Separate from `api.ts` because these go through a Tauri plugin rather than
 * this app's own commands, and because a browser build would replace them with
 * a file-input shim rather than with an IPC transport.
 */

import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';

export interface PickFolderOptions {
  title?: string;
  startDirectory?: string;
}

export async function pickFolder(options: PickFolderOptions = {}): Promise<string | null> {
  const selected = await openDialog({
    directory: true,
    multiple: false,
    title: options.title ?? 'Choose a folder',
    defaultPath: options.startDirectory,
  });
  return typeof selected === 'string' ? selected : null;
}

export interface PickFilesOptions {
  title?: string;
  extensions?: string[];
  multiple?: boolean;
}

export async function pickFiles(options: PickFilesOptions = {}): Promise<string[]> {
  const selected = await openDialog({
    multiple: options.multiple ?? true,
    title: options.title ?? 'Choose files',
    filters: options.extensions
      ? [{ name: 'Supported files', extensions: options.extensions }]
      : undefined,
  });
  if (selected === null) return [];
  return Array.isArray(selected) ? selected : [selected];
}

export async function pickSaveLocation(
  defaultName: string,
  extensions?: string[],
): Promise<string | null> {
  const selected = await saveDialog({
    defaultPath: defaultName,
    filters: extensions ? [{ name: 'Export', extensions }] : undefined,
  });
  return selected ?? null;
}
