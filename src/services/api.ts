/**
 * The only place in the application that talks to the backend.
 *
 * Nothing else imports `@tauri-apps/api`. That single choke point is what
 * makes the frontend testable without a desktop shell (the test double
 * replaces this module) and what would let a future web or mobile client
 * swap the transport without touching a component.
 */

import { invoke } from '@tauri-apps/api/core';

import type {
  AppDirectories,
  Backlink,
  Block,
  CommandError,
  Diagnostic,
  DirectoryListing,
  FileEntry,
  FileMatch,
  GraphData,
  Heading,
  IndexProgress,
  LinkResolution,
  Note,
  OpenReport,
  Property,
  RecentVault,
  RenameOutcome,
  RenamePlan,
  ResolvedLink,
  SearchHit,
  SearchResults,
  TagSummary,
  TemplateInfo,
  TrashEntry,
  VaultInfo,
  VaultPath,
  VaultSettings,
  Workspace,
} from '@/types/domain';
import { isCommandError } from '@/types/domain';

/** Arguments are plain JSON; the backend types them on arrival. */
type Args = Record<string, unknown>;

/**
 * Call a command, normalising whatever comes back on failure into a
 * `CommandError`.
 *
 * Tauri rejects with the serialised error when a command returns `Err`, but
 * with a plain string when the call itself fails — an unknown command, a
 * capability the window does not have. Both must end up the same shape, or
 * every caller would need two error paths.
 */
async function call<T>(command: string, args?: Args): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw normaliseError(command, error);
  }
}

export function normaliseError(command: string, error: unknown): CommandError {
  if (isCommandError(error)) return error;
  if (typeof error === 'string') {
    return { code: 'ipc_error', message: error, recoverableByReindex: false };
  }
  if (error instanceof Error) {
    return { code: 'ipc_error', message: error.message, recoverableByReindex: false };
  }
  return {
    code: 'ipc_error',
    message: `The command "${command}" failed for an unknown reason.`,
    recoverableByReindex: false,
  };
}

export interface GraphRequest {
  includeAttachments?: boolean;
  includeUnresolved?: boolean;
  includeTags?: boolean;
  folder?: VaultPath;
  maxNodes?: number;
}

export interface ExportRequest {
  format?: 'markdown' | 'html';
  includeLinkedNotes?: boolean;
  includeAttachments?: boolean;
  includeProperties?: boolean;
  depth?: number;
}

export const api = {
  // Vault lifecycle
  openVault: (path: string) => call<OpenReport>('open_vault', { path }),
  createVault: (path: string, name: string) => call<OpenReport>('create_vault', { path, name }),
  closeVault: () => call<void>('close_vault'),
  isVaultOpen: () => call<boolean>('is_vault_open'),
  vaultInfo: () => call<VaultInfo>('vault_info'),
  updateVaultSettings: (settings: VaultSettings) =>
    call<void>('update_vault_settings', { settings }),
  recentVaults: () => call<RecentVault[]>('recent_vaults'),
  forgetVault: (path: string) => call<void>('forget_vault', { path }),
  rebuildIndex: () => call<void>('rebuild_index'),
  indexStatus: () => call<IndexProgress>('index_status'),
  vaultDiagnostics: () => call<Diagnostic[]>('vault_diagnostics'),

  // Files
  listFolder: (path: VaultPath) => call<DirectoryListing>('list_folder', { path }),
  readNote: (path: VaultPath) => call<Note>('read_note', { path }),
  readFileBytes: (path: VaultPath) => call<number[]>('read_file_bytes', { path }),
  resolveAssetPath: (path: VaultPath) => call<string>('resolve_asset_path', { path }),
  saveNote: (path: VaultPath, content: string) => call<void>('save_note', { path, content }),
  createNote: (path: VaultPath, content: string, overwrite = false) =>
    call<VaultPath>('create_note', { path, content, overwrite }),
  createNoteFromLink: (target: string, folder?: VaultPath) =>
    call<VaultPath>('create_note_from_link', { target, folder }),
  createFolder: (path: VaultPath) => call<void>('create_folder', { path }),
  renameEntry: (from: VaultPath, to: VaultPath) =>
    call<RenameOutcome>('rename_entry', { from, to }),
  previewRename: (from: VaultPath, to: VaultPath) =>
    call<RenamePlan>('preview_rename', { from, to }),
  deleteEntry: (path: VaultPath) => call<TrashEntry>('delete_entry', { path }),
  duplicateEntry: (path: VaultPath) => call<VaultPath>('duplicate_entry', { path }),
  listTrash: () => call<TrashEntry[]>('list_trash'),
  restoreFromTrash: (id: string) => call<VaultPath>('restore_from_trash', { id }),
  purgeFromTrash: (id: string) => call<void>('purge_from_trash', { id }),
  emptyTrash: () => call<number>('empty_trash'),
  setProperties: (path: VaultPath, properties: Property[]) =>
    call<void>('set_properties', { path, properties }),
  importAttachment: (fileName: string, bytes: number[], note?: VaultPath) =>
    call<VaultPath>('import_attachment', { fileName, bytes, note }),
  revealInFileManager: (path: VaultPath) => call<void>('reveal_in_file_manager', { path }),
  openExternal: (url: string) => call<void>('open_external', { url }),

  // Links, tags and structure
  backlinks: (path: VaultPath) => call<Backlink[]>('backlinks', { path }),
  outgoingLinks: (path: VaultPath) => call<ResolvedLink[]>('outgoing_links', { path }),
  outline: (path: VaultPath) => call<Heading[]>('outline', { path }),
  noteBlocks: (path: VaultPath) => call<Block[]>('note_blocks', { path }),
  unlinkedMentions: (path: VaultPath, limit?: number) =>
    call<SearchHit[]>('unlinked_mentions', { path, limit }),
  unresolvedLinks: (limit?: number) => call<UnresolvedTargetDto[]>('unresolved_links', { limit }),
  ambiguousLinks: (limit?: number) => call<UnresolvedTargetDto[]>('ambiguous_links', { limit }),
  resolveLink: (from: VaultPath, target: string) =>
    call<LinkResolution>('resolve_link', { from, target }),
  allTags: () => call<TagSummary[]>('all_tags'),
  filesWithTag: (tag: string, limit?: number) =>
    call<FileEntry[]>('files_with_tag', { tag, limit }),
  propertyKeys: () => call<[string, number][]>('property_keys'),
  propertyValues: (key: string, limit?: number) =>
    call<string[]>('property_values', { key, limit }),
  recentFiles: (limit?: number) => call<FileEntry[]>('recent_files', { limit }),
  graph: (options?: GraphRequest) => call<GraphData>('graph', { options }),
  localGraph: (path: VaultPath, depth?: number, options?: GraphRequest) =>
    call<GraphData>('local_graph', { path, depth, options }),

  // Search
  searchVault: (query: string, limit?: number, offset?: number) =>
    call<SearchResults>('search_vault', { query, limit, offset }),
  validateQuery: (query: string) => call<void>('validate_query', { query }),
  quickSwitch: (needle: string, limit?: number) =>
    call<FileMatch[]>('quick_switch', { needle, limit }),
  completeTags: (needle: string, limit?: number) =>
    call<[string, number][]>('complete_tags', { needle, limit }),
  completeHeadings: (path: VaultPath, needle: string, limit?: number) =>
    call<string[]>('complete_headings', { path, needle, limit }),
  completeBlocks: (path: VaultPath, limit?: number) =>
    call<string[]>('complete_blocks', { path, limit }),

  // Workspace, templates and settings
  loadWorkspace: () => call<{ workspace: Workspace; removed: VaultPath[] }>('load_workspace'),
  saveWorkspace: (workspace: Workspace) => call<void>('save_workspace', { workspace }),
  resetWorkspace: () => call<Workspace>('reset_workspace'),
  saveWorkspaceAs: (name: string, workspace: Workspace) =>
    call<void>('save_workspace_as', { name, workspace }),
  loadSavedWorkspace: (name: string) => call<Workspace>('load_saved_workspace', { name }),
  listSavedWorkspaces: () => call<string[]>('list_saved_workspaces'),
  deleteSavedWorkspace: (name: string) => call<void>('delete_saved_workspace', { name }),
  listTemplates: () => call<TemplateInfo[]>('list_templates'),
  renderTemplate: (template: VaultPath, target: VaultPath) =>
    call<string>('render_template', { template, target }),
  openDailyNote: (dayOffset?: number) =>
    call<{ path: VaultPath; created: boolean }>('open_daily_note', { dayOffset }),
  exportNotes: (paths: VaultPath[], destination: string, options?: ExportRequest) =>
    call<{ files: unknown[]; unresolvedLinks: string[] }>('export_notes', {
      paths,
      destination,
      options,
    }),
  importFiles: (folder: VaultPath, sources: string[]) =>
    call<{ imported: { target: VaultPath }[]; skipped: string[] }>('import_files', {
      folder,
      sources,
    }),
  appDirectories: () => call<AppDirectories>('app_directories'),
  loadAppSettings: () => call<Record<string, unknown>>('load_app_settings'),
  saveAppSettings: (settings: Record<string, unknown>) =>
    call<void>('save_app_settings', { settings }),
  readLogTail: (lines?: number) => call<string>('read_log_tail', { lines }),
};

export interface UnresolvedTargetDto {
  target: string;
  count: number;
  sources: VaultPath[];
}

export type Api = typeof api;
