/**
 * The shapes that cross the IPC boundary.
 *
 * Every type here mirrors a `serde` type in `ie-core::model`. They are written
 * by hand rather than generated so the frontend can carry the doc comments
 * that matter to a UI author, and a conformance test asserts the two
 * definitions agree on the fields the UI actually reads.
 */

/**
 * A location inside the vault: relative, `/`-separated on every platform, and
 * never containing `..`.
 *
 * It is a branded string rather than a plain one so a filesystem path cannot
 * be passed where a vault path is expected. Values always come from the
 * backend; construct one with `vaultPath()` only when parsing user input.
 */
export type VaultPath = string & { readonly __brand: 'VaultPath' };

/** Assert that a string is a vault path. Only for values the backend produced. */
export function asVaultPath(value: string): VaultPath {
  return value as VaultPath;
}

/** The vault root. */
export const VAULT_ROOT = '' as VaultPath;

export function pathFileName(path: VaultPath): string {
  const index = path.lastIndexOf('/');
  return index === -1 ? path : path.slice(index + 1);
}

export function pathStem(path: VaultPath): string {
  const name = pathFileName(path);
  const dot = name.lastIndexOf('.');
  return dot <= 0 ? name : name.slice(0, dot);
}

export function pathParent(path: VaultPath): VaultPath {
  const index = path.lastIndexOf('/');
  return (index === -1 ? '' : path.slice(0, index)) as VaultPath;
}

export function pathExtension(path: VaultPath): string | null {
  const name = pathFileName(path);
  const dot = name.lastIndexOf('.');
  return dot <= 0 ? null : name.slice(dot + 1).toLowerCase();
}

export function joinPath(folder: VaultPath, segment: string): VaultPath {
  return (folder ? `${folder}/${segment}` : segment) as VaultPath;
}

export type FileKind = 'note' | 'canvas' | 'image' | 'pdf' | 'audio' | 'video' | 'other';

export interface FileEntry {
  path: VaultPath;
  name: string;
  kind: FileKind;
  size: number;
  modifiedMs: number;
  title: string;
}

export interface FolderEntry {
  path: VaultPath;
  name: string;
  childFileCount: number;
  childFolderCount: number;
}

export interface DirectoryListing {
  path: VaultPath;
  folders: FolderEntry[];
  files: FileEntry[];
}

export type LinkKind = 'wikiLink' | 'embed' | 'markdown' | 'markdownImage' | 'external';

export interface Link {
  kind: LinkKind;
  raw: string;
  target: string;
  heading: string | null;
  blockId: string | null;
  alias: string | null;
  line: number;
  byteStart: number;
  byteEnd: number;
}

export interface ResolvedLink extends Link {
  targetPath: VaultPath | null;
}

export interface Backlink {
  sourcePath: VaultPath;
  sourceTitle: string;
  kind: LinkKind;
  line: number;
  context: string;
  alias: string | null;
}

export interface Tag {
  name: string;
  line: number;
  byteStart: number;
  byteEnd: number;
}

export interface TagSummary {
  name: string;
  /** Uses of exactly this tag. */
  count: number;
  /** Uses of this tag and everything nested beneath it. */
  totalCount: number;
}

export interface Heading {
  level: number;
  text: string;
  slug: string;
  line: number;
  byteStart: number;
}

export interface Block {
  id: string;
  line: number;
  byteStart: number;
  byteEnd: number;
}

export type PropertyKind =
  | 'text'
  | 'number'
  | 'checkbox'
  | 'date'
  | 'datetime'
  | 'list'
  | 'object'
  | 'null';

/**
 * A frontmatter value, tagged the way `serde` writes it.
 *
 * The distinction is load-bearing: `rating: 8` is a number and `rating: "8"`
 * is text, and a query saying `rating > 7` needs to know which.
 */
export type PropertyValue =
  | { kind: 'text'; value: string }
  | { kind: 'number'; value: number }
  | { kind: 'checkbox'; value: boolean }
  | { kind: 'date'; value: string }
  | { kind: 'datetime'; value: string }
  | { kind: 'list'; value: PropertyValue[] }
  | { kind: 'object'; value: Record<string, PropertyValue> }
  | { kind: 'null' };

export interface Property {
  key: string;
  value: PropertyValue;
}

export interface NoteMetadata {
  title: string | null;
  properties: Property[];
  links: Link[];
  tags: Tag[];
  headings: Heading[];
  blocks: Block[];
  frontmatterBytes: number;
  wordCount: number;
}

export interface Note {
  path: VaultPath;
  title: string;
  content: string;
  metadata: NoteMetadata;
  modifiedMs: number;
}

export type GraphNodeKind = 'note' | 'attachment' | 'unresolved' | 'tag';

export interface GraphNode {
  id: string;
  path: VaultPath | null;
  label: string;
  kind: GraphNodeKind;
  degree: number;
  tags: string[];
  folder: string;
}

export interface GraphEdge {
  source: string;
  target: string;
  kind: LinkKind;
}

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
  truncated: boolean;
}

export interface SearchHit {
  path: VaultPath;
  title: string;
  kind: FileKind;
  /** The match with context, containing `<mark>` around the matched words. */
  snippet: string;
  line: number | null;
  modifiedMs: number;
  rank: number;
}

export interface SearchResults {
  hits: SearchHit[];
  total: number;
  truncated: boolean;
}

export interface FileMatch {
  path: VaultPath;
  title: string;
  kind: FileKind;
  score: number;
  /** Character offsets in `path` that matched, for highlighting. */
  positions: number[];
  modifiedMs: number;
}

export interface UnresolvedTarget {
  target: string;
  count: number;
  sources: VaultPath[];
}

export type TabMode = 'edit' | 'read' | 'canvas' | 'graph' | 'pdf' | 'image';

export interface TabState {
  id: string;
  path: VaultPath;
  pinned: boolean;
  mode: TabMode;
  scrollLine: number;
  cursorOffset: number;
}

export type SplitDirection = 'horizontal' | 'vertical';

export type PaneNode =
  | { type: 'leaf'; id: string; tabs: TabState[]; activeTabId: string | null }
  | {
      type: 'split';
      id: string;
      direction: SplitDirection;
      ratio: number;
      first: PaneNode;
      second: PaneNode;
    };

export interface PaneLayout {
  root: PaneNode;
  activePaneId: string;
}

export interface WorkspaceSidebar {
  visible: boolean;
  width: number;
  activePanel: string;
}

export interface Workspace {
  version: number;
  layout: PaneLayout;
  leftSidebar: WorkspaceSidebar;
  rightSidebar: WorkspaceSidebar;
  activeFile: VaultPath | null;
  graphState: unknown | null;
}

export type AttachmentLocation =
  | { mode: 'vaultFolder'; folder: VaultPath }
  | { mode: 'nextToNote' }
  | { mode: 'subfolderOfNote'; name: string };

export type LinkStyle = 'shortestWikiLink' | 'absoluteWikiLink' | 'markdownLink';

export interface DailyNoteSettings {
  folder: VaultPath;
  format: string;
  template: VaultPath | null;
  openOnStartup: boolean;
}

export interface VaultSettings {
  version: number;
  id: string;
  name: string;
  createdMs: number;
  attachments: AttachmentLocation;
  templatesFolder: VaultPath;
  dailyNotes: DailyNoteSettings;
  linkStyle: LinkStyle;
  updateLinksOnRename: boolean;
  extraIgnoredFolders: string[];
  newNoteFolder: VaultPath | null;
}

export interface VaultInfo {
  root: string;
  settings: VaultSettings;
  fileCount: number;
  caseSensitive: boolean;
}

export interface OpenReport {
  root: string;
  name: string;
  index: 'reused' | 'created' | 'rebuiltForSchemaChange' | 'rebuiltAfterCorruption';
  created: boolean;
  caseSensitive: boolean;
}

export interface RecentVault {
  path: string;
  name: string;
  lastOpenedMs: number;
}

export interface TrashEntry {
  id: string;
  originalPath: VaultPath;
  trashedMs: number;
  storedAs: string;
  isDir: boolean;
  size: number;
}

export interface RenamePlan {
  from: VaultPath;
  to: VaultPath;
  edits: { path: VaultPath; linkCount: number }[];
  totalLinks: number;
}

export interface RenameOutcome {
  from: VaultPath;
  to: VaultPath;
  filesUpdated: number;
  linksUpdated: number;
  failures: string[];
}

export interface TemplateInfo {
  path: VaultPath;
  name: string;
}

export interface IndexProgress {
  scanned: number;
  indexed: number;
  total: number | null;
  current: VaultPath | null;
}

export type Diagnostic =
  | { kind: 'caseConflict'; paths: VaultPath[] }
  | { kind: 'unportableName'; path: VaultPath; reason: string }
  | { kind: 'interruptedWrite'; path: VaultPath }
  | { kind: 'unreadableFile'; path: VaultPath; message: string }
  | { kind: 'malformedFrontmatter'; path: VaultPath; message: string }
  | { kind: 'escapingSymlink'; path: VaultPath };

export interface AppDirectories {
  config: string;
  data: string;
  logs: string;
  cache: string;
  platform: string;
}

export type LinkResolution = {
  target: string;
} & (
  | {
      outcome: 'resolved';
      path: VaultPath;
      line: number | null;
      anchorMissing: boolean;
      ambiguous: boolean;
    }
  | { outcome: 'unresolved'; suggestedName: string }
  | { outcome: 'external'; url: string }
);

/** An error from a command: a code to branch on and a sentence to show. */
export interface CommandError {
  code: string;
  message: string;
  recoverableByReindex: boolean;
}

export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' &&
    value !== null &&
    'code' in value &&
    'message' in value &&
    typeof (value as CommandError).message === 'string'
  );
}
