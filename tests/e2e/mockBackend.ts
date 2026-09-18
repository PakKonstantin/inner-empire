/**
 * An in-memory stand-in for the Tauri backend.
 *
 * It implements the command surface the frontend calls, backed by a plain
 * object holding note text. The point is not to reimplement the indexer — it
 * is to be faithful enough that the frontend's wiring is genuinely exercised:
 * links resolve, backlinks appear, search matches, saving persists.
 *
 * Injected before the app's own scripts run, so `invoke()` finds it exactly
 * where the real one would be.
 */

export interface MockVaultFile {
  path: string;
  content: string;
}

export interface MockOptions {
  /** Files the vault starts with. */
  files?: MockVaultFile[];
  /** Open a vault immediately, as a reopened session would. */
  openVault?: boolean;
  /** Offer a recent vault. With one, the app reopens it at launch. */
  recentVaults?: boolean;
}

/**
 * The script installed into the page.
 *
 * Written as a single function serialised into the page, because Playwright's
 * `addInitScript` runs it in a context with none of this module's scope.
 */
export function installMockBackend(options: MockOptions = {}): string {
  const payload = JSON.stringify({
    files: options.files ?? [],
    openVault: options.openVault ?? true,
    recentVaults: options.recentVaults ?? true,
  });

  return `
(() => {
  const setup = ${payload};

  // ---- the vault ----------------------------------------------------------
  const files = new Map();
  for (const file of setup.files) files.set(file.path, file.content);

  let vaultOpen = setup.openVault;
  let workspace = null;
  const trash = [];
  const listeners = new Map();

  // Exposed so a test can assert on what reached the "disk".
  window.__mockVault = {
    files,
    read: (path) => files.get(path),
    has: (path) => files.has(path),
    paths: () => [...files.keys()],
    savedWorkspace: () => workspace,
  };

  // ---- parsing, matching what the backend recognises -----------------------
  const stripFrontmatter = (text) => {
    if (!text.startsWith('---')) return text;
    const end = text.indexOf('\\n---', 3);
    if (end === -1) return text;
    const after = text.indexOf('\\n', end + 1);
    return after === -1 ? '' : text.slice(after + 1);
  };

  const withoutCode = (text) =>
    text.replace(/\`\`\`[\\s\\S]*?\`\`\`/g, '').replace(/\`[^\`\\n]*\`/g, '');

  const linksIn = (text) => {
    const body = withoutCode(stripFrontmatter(text));
    const out = [];
    const pattern = /(!?)\\[\\[([^\\]\\n]+)\\]\\]/g;
    let match;
    while ((match = pattern.exec(body)) !== null) {
      const inner = match[2];
      const pipe = inner.indexOf('|');
      const locator = (pipe === -1 ? inner : inner.slice(0, pipe)).trim();
      const alias = pipe === -1 ? null : inner.slice(pipe + 1).trim();
      const hash = locator.indexOf('#');
      const target = (hash === -1 ? locator : locator.slice(0, hash)).trim();
      const fragment = hash === -1 ? null : locator.slice(hash + 1);
      out.push({
        kind: match[1] === '!' ? 'embed' : 'wikiLink',
        raw: match[0],
        target,
        heading: fragment && !fragment.startsWith('^') ? fragment : null,
        blockId: fragment && fragment.startsWith('^') ? fragment.slice(1) : null,
        alias,
        line: body.slice(0, match.index).split('\\n').length - 1,
        byteStart: match.index,
        byteEnd: match.index + match[0].length,
      });
    }
    return out;
  };

  const tagsIn = (text) => {
    const body = withoutCode(stripFrontmatter(text));
    const out = [];
    const pattern = /(^|[^\\p{L}\\p{N}_#/\\\\])#([\\p{L}\\p{N}_/-]*\\p{L}[\\p{L}\\p{N}_/-]*)/gu;
    let match;
    while ((match = pattern.exec(body)) !== null) out.push(match[2].replace(/[/-]+$/, ''));
    return out;
  };

  const headingsIn = (text) => {
    const body = stripFrontmatter(text);
    return body.split('\\n').flatMap((line, index) => {
      const match = /^(#{1,6})\\s+(.*)$/.exec(line);
      if (!match) return [];
      const heading = match[2].trim();
      return [{
        level: match[1].length,
        text: heading,
        slug: heading.toLowerCase().replace(/[^\\p{L}\\p{N}]+/gu, '-').replace(/-+$/, ''),
        line: index,
        byteStart: 0,
      }];
    });
  };

  const titleOf = (path) => {
    const content = files.get(path) ?? '';
    const frontmatter = /^---\\n([\\s\\S]*?)\\n---/.exec(content);
    const explicit = frontmatter && /^title:\\s*(.+)$/m.exec(frontmatter[1]);
    if (explicit) return explicit[1].trim();
    const first = headingsIn(content).find((heading) => heading.level === 1);
    return first ? first.text : stemOf(path);
  };

  const stemOf = (path) => {
    const name = path.slice(path.lastIndexOf('/') + 1);
    const dot = name.lastIndexOf('.');
    return dot <= 0 ? name : name.slice(0, dot);
  };

  const parentOf = (path) => {
    const slash = path.lastIndexOf('/');
    return slash === -1 ? '' : path.slice(0, slash);
  };

  // The backend's ranking, in miniature: an exact path beats an implicit .md,
  // which beats a bare stem, and a shallower file wins within a tier.
  const resolveTarget = (target) => {
    if (!target) return null;
    const candidates = [...files.keys()];
    const exact = candidates.find((path) => path === target);
    if (exact) return exact;
    const implicit = candidates.find((path) => path === target + '.md');
    if (implicit) return implicit;
    const byStem = candidates
      .filter((path) => stemOf(path).toLowerCase() === target.toLowerCase())
      .sort((a, b) => a.split('/').length - b.split('/').length || a.localeCompare(b));
    return byStem[0] ?? null;
  };

  const entryFor = (path) => ({
    path,
    name: path.slice(path.lastIndexOf('/') + 1),
    kind: /\\.(md|markdown)$/i.test(path)
      ? 'note'
      : /\\.canvas$/i.test(path)
        ? 'canvas'
        : /\\.(png|jpe?g|gif|webp|svg)$/i.test(path)
          ? 'image'
          : /\\.pdf$/i.test(path)
            ? 'pdf'
            : 'other',
    size: (files.get(path) ?? '').length,
    modifiedMs: 1_789_653_909_000,
    title: titleOf(path),
  });

  const emit = (event, payload) => {
    for (const handler of listeners.get(event) ?? []) handler({ event, payload });
  };

  // ---- the command surface -------------------------------------------------
  const commands = {
    is_vault_open: () => vaultOpen,
    recent_vaults: () =>
      setup.recentVaults ? [{ path: '/vault', name: 'Test Vault', lastOpenedMs: 1 }] : [],
    forget_vault: () => null,

    open_vault: ({ path }) => {
      vaultOpen = true;
      const report = { root: path, name: 'Test Vault', index: 'reused', created: false, caseSensitive: true };
      queueMicrotask(() => {
        emit('vaultOpened', { root: path, name: 'Test Vault' });
        emit('indexCompleted', { files: files.size, durationMs: 1, diagnostics: [] });
      });
      return report;
    },

    create_vault: ({ path, name }) => {
      vaultOpen = true;
      queueMicrotask(() => {
        emit('vaultOpened', { root: path, name });
        emit('indexCompleted', { files: files.size, durationMs: 1, diagnostics: [] });
      });
      return { root: path, name, index: 'created', created: true, caseSensitive: true };
    },

    close_vault: () => {
      vaultOpen = false;
      emit('vaultClosed', undefined);
      return null;
    },

    vault_info: () => ({
      root: '/vault',
      fileCount: files.size,
      caseSensitive: true,
      settings: {
        version: 1,
        id: 'test',
        name: 'Test Vault',
        createdMs: 0,
        attachments: { mode: 'vaultFolder', folder: 'Attachments' },
        templatesFolder: 'Templates',
        dailyNotes: { folder: 'Daily', format: 'YYYY-MM-DD', template: null, openOnStartup: false },
        linkStyle: 'shortestWikiLink',
        updateLinksOnRename: true,
        extraIgnoredFolders: [],
        newNoteFolder: null,
      },
    }),
    update_vault_settings: () => null,
    vault_diagnostics: () => [],
    rebuild_index: () => null,
    index_status: () => ({ scanned: files.size, indexed: files.size, total: files.size, current: null }),

    list_folder: ({ path }) => {
      const prefix = path ? path + '/' : '';
      const children = [...files.keys()].filter((candidate) => candidate.startsWith(prefix));
      const folders = new Set();
      const direct = [];
      for (const candidate of children) {
        const rest = candidate.slice(prefix.length);
        const slash = rest.indexOf('/');
        if (slash === -1) direct.push(candidate);
        else folders.add(prefix + rest.slice(0, slash));
      }
      return {
        path,
        folders: [...folders].sort().map((folder) => ({
          path: folder,
          name: folder.slice(folder.lastIndexOf('/') + 1),
          childFileCount: children.filter((c) => c.startsWith(folder + '/')).length,
          childFolderCount: 0,
        })),
        files: direct.sort().map(entryFor),
      };
    },

    read_note: ({ path }) => {
      const content = files.get(path);
      if (content === undefined) throw { code: 'not_found', message: path + ' does not exist', recoverableByReindex: false };
      return {
        path,
        title: titleOf(path),
        content,
        modifiedMs: 1_789_653_909_000,
        metadata: {
          title: titleOf(path),
          properties: [],
          links: linksIn(content),
          tags: tagsIn(content).map((name) => ({ name, line: 0, byteStart: 0, byteEnd: 0 })),
          headings: headingsIn(content),
          blocks: [],
          frontmatterBytes: 0,
          wordCount: content.split(/\\s+/).filter(Boolean).length,
        },
      };
    },

    save_note: ({ path, content }) => {
      files.set(path, content);
      emit('fileModified', { path });
      return null;
    },

    create_note: ({ path, content }) => {
      files.set(path, content ?? '');
      emit('fileCreated', { path });
      return path;
    },

    create_note_from_link: ({ target, folder }) => {
      const safe = target.replace(/[<>:"|?*\\\\/]/g, '-');
      const path = (folder ? folder + '/' : '') + safe + '.md';
      files.set(path, '# ' + safe + '\\n\\n');
      emit('fileCreated', { path });
      return path;
    },

    create_folder: () => null,
    resolve_asset_path: ({ path }) => '/vault/' + path,
    read_file_bytes: () => [],

    rename_entry: ({ from, to }) => {
      const content = files.get(from) ?? '';
      files.delete(from);
      files.set(to, content);

      // Rewrite every link that pointed at the old note, which is the
      // behaviour the flow test is actually checking.
      let filesUpdated = 0;
      let linksUpdated = 0;
      const oldStem = stemOf(from);
      const newStem = stemOf(to);
      for (const [path, text] of [...files.entries()]) {
        if (path === to) continue;
        let count = 0;
        const updated = text.replace(/(!?)\\[\\[([^\\]\\n]+)\\]\\]/g, (whole, bang, inner) => {
          const pipe = inner.indexOf('|');
          const locator = pipe === -1 ? inner : inner.slice(0, pipe);
          const rest = pipe === -1 ? '' : inner.slice(pipe);
          const hash = locator.indexOf('#');
          const target = hash === -1 ? locator : locator.slice(0, hash);
          const fragment = hash === -1 ? '' : locator.slice(hash);
          if (target.trim().toLowerCase() !== oldStem.toLowerCase()) return whole;
          count += 1;
          return bang + '[[' + newStem + fragment + rest + ']]';
        });
        if (count > 0) {
          files.set(path, updated);
          filesUpdated += 1;
          linksUpdated += count;
        }
      }

      emit('fileRenamed', { from, to });
      return { from, to, filesUpdated, linksUpdated, failures: [] };
    },

    preview_rename: ({ from, to }) => ({ from, to, edits: [], totalLinks: 0 }),

    delete_entry: ({ path }) => {
      const entry = {
        id: 'trash-' + trash.length,
        originalPath: path,
        trashedMs: Date.now(),
        storedAs: path,
        isDir: false,
        size: 0,
      };
      trash.push({ entry, content: files.get(path) ?? '' });
      files.delete(path);
      emit('fileDeleted', { path });
      return entry;
    },

    duplicate_entry: ({ path }) => {
      const copy = path.replace(/\\.md$/, ' 1.md');
      files.set(copy, files.get(path) ?? '');
      emit('fileCreated', { path: copy });
      return copy;
    },

    list_trash: () => trash.map((item) => item.entry),
    restore_from_trash: ({ id }) => {
      const index = trash.findIndex((item) => item.entry.id === id);
      if (index === -1) return '';
      const [item] = trash.splice(index, 1);
      files.set(item.entry.originalPath, item.content);
      emit('fileCreated', { path: item.entry.originalPath });
      return item.entry.originalPath;
    },
    purge_from_trash: () => null,
    empty_trash: () => 0,
    set_properties: () => null,
    import_attachment: ({ fileName }) => 'Attachments/' + fileName,
    reveal_in_file_manager: () => null,
    open_external: () => null,

    backlinks: ({ path }) => {
      const out = [];
      for (const [source, text] of files.entries()) {
        if (source === path) continue;
        for (const link of linksIn(text)) {
          if (resolveTarget(link.target) !== path) continue;
          out.push({
            sourcePath: source,
            sourceTitle: titleOf(source),
            kind: link.kind,
            line: link.line,
            context: link.raw,
            alias: link.alias,
          });
        }
      }
      return out;
    },

    outgoing_links: ({ path }) =>
      linksIn(files.get(path) ?? '').map((link) => ({
        ...link,
        targetPath: resolveTarget(link.target),
      })),

    outline: ({ path }) => headingsIn(files.get(path) ?? ''),
    note_blocks: () => [],
    unlinked_mentions: () => [],

    unresolved_links: () => {
      const counts = new Map();
      for (const [source, text] of files.entries()) {
        for (const link of linksIn(text)) {
          if (resolveTarget(link.target)) continue;
          const existing = counts.get(link.target) ?? { target: link.target, count: 0, sources: [] };
          existing.count += 1;
          existing.sources.push(source);
          counts.set(link.target, existing);
        }
      }
      return [...counts.values()];
    },

    ambiguous_links: () => [],

    resolve_link: ({ target }) => {
      if (/^https?:/.test(target)) return { target, outcome: 'external', url: target };
      const path = resolveTarget(target.split('#')[0]);
      return path
        ? { target, outcome: 'resolved', path, line: null, anchorMissing: false, ambiguous: false }
        : { target, outcome: 'unresolved', suggestedName: target };
    },

    all_tags: () => {
      const counts = new Map();
      for (const text of files.values()) {
        for (const tag of tagsIn(text)) counts.set(tag, (counts.get(tag) ?? 0) + 1);
      }
      return [...counts.entries()].map(([name, count]) => ({
        name,
        count,
        totalCount: [...counts.entries()]
          .filter(([other]) => other === name || other.startsWith(name + '/'))
          .reduce((sum, [, value]) => sum + value, 0),
      }));
    },

    files_with_tag: ({ tag }) =>
      [...files.entries()]
        .filter(([, text]) => tagsIn(text).some((name) => name === tag || name.startsWith(tag + '/')))
        .map(([path]) => entryFor(path)),

    property_keys: () => [],
    property_values: () => [],
    recent_files: ({ limit }) => [...files.keys()].slice(0, limit ?? 20).map(entryFor),

    graph: () => {
      const nodes = [...files.keys()]
        .filter((path) => /\\.md$/i.test(path))
        .map((path) => ({
          id: path,
          path,
          label: titleOf(path),
          kind: 'note',
          degree: 0,
          tags: [],
          folder: parentOf(path),
        }));
      const known = new Set(nodes.map((node) => node.id));
      const edges = [];
      for (const [source, text] of files.entries()) {
        for (const link of linksIn(text)) {
          const target = resolveTarget(link.target);
          if (target && known.has(source) && known.has(target) && source !== target) {
            edges.push({ source, target, kind: link.kind });
          }
        }
      }
      return { nodes, edges, truncated: false };
    },

    local_graph: () => ({ nodes: [], edges: [], truncated: false }),

    search_vault: ({ query, limit }) => {
      const needle = query.trim().toLowerCase();
      const tagMatch = /^tag:(\\S+)$/.exec(needle);
      const hits = [];

      for (const [path, text] of files.entries()) {
        const body = stripFrontmatter(text).toLowerCase();
        const matches = tagMatch
          ? tagsIn(text).some((tag) => tag.toLowerCase() === tagMatch[1] || tag.toLowerCase().startsWith(tagMatch[1] + '/'))
          : needle.split(/\\s+/).every((word) => body.includes(word.replace(/^"|"$/g, '')));
        if (!matches) continue;

        const index = body.indexOf(needle.split(/\\s+/)[0] ?? '');
        hits.push({
          path,
          title: titleOf(path),
          kind: 'note',
          snippet: index === -1 ? '' : '<mark>' + stripFrontmatter(text).slice(index, index + 60) + '</mark>',
          line: null,
          modifiedMs: 1,
          rank: 0,
        });
      }
      return { hits: hits.slice(0, limit ?? 100), total: hits.length, truncated: false };
    },

    validate_query: () => null,

    quick_switch: ({ needle, limit }) => {
      const lowered = (needle ?? '').toLowerCase();
      return [...files.keys()]
        .filter((path) => {
          if (!lowered) return true;
          // A subsequence match, the same shape the backend uses.
          let index = 0;
          for (const character of lowered) {
            index = path.toLowerCase().indexOf(character, index) + 1;
            if (index === 0) return false;
          }
          return true;
        })
        .slice(0, limit ?? 50)
        .map((path) => ({ ...entryFor(path), score: 1, positions: [] }));
    },

    complete_tags: () => [],
    complete_headings: () => [],
    complete_blocks: () => [],

    load_workspace: () => ({
      workspace: workspace ?? {
        version: 1,
        layout: { root: { type: 'leaf', id: 'pane-root', tabs: [], activeTabId: null }, activePaneId: 'pane-root' },
        leftSidebar: { visible: true, width: 260, activePanel: 'files' },
        rightSidebar: { visible: true, width: 300, activePanel: 'backlinks' },
        activeFile: null,
        graphState: null,
      },
      removed: [],
    }),

    save_workspace: ({ workspace: next }) => {
      workspace = next;
      // Persisted across a reload, so the restart test is meaningful.
      try {
        window.localStorage.setItem('mock-workspace', JSON.stringify(next));
      } catch {}
      return null;
    },

    reset_workspace: () => {
      workspace = null;
      return commands.load_workspace().workspace;
    },

    save_workspace_as: () => null,
    load_saved_workspace: () => commands.load_workspace().workspace,
    list_saved_workspaces: () => [],
    delete_saved_workspace: () => null,
    list_templates: () => [],
    render_template: () => '',
    open_daily_note: () => {
      const path = 'Daily/2026-09-17.md';
      const created = !files.has(path);
      if (created) files.set(path, '# 2026-09-17\\n\\n');
      return { path, created };
    },
    export_notes: () => ({ files: [], unresolvedLinks: [] }),
    import_files: () => ({ imported: [], skipped: [] }),
    app_directories: () => ({ config: '/config', data: '/data', logs: '/logs', cache: '/cache', platform: 'linux' }),
    load_app_settings: () => ({}),
    save_app_settings: () => null,
    read_log_tail: () => '',
  };

  // Restore a workspace saved before a reload.
  try {
    const stored = window.localStorage.getItem('mock-workspace');
    if (stored) workspace = JSON.parse(stored);
  } catch {}

  // ---- the Tauri surface ---------------------------------------------------
  window.__TAURI_INTERNALS__ = {
    invoke: (command, args) => {
      const handler = commands[command];
      if (!handler) {
        return Promise.reject('The mock backend has no command named ' + command);
      }
      try {
        return Promise.resolve(handler(args ?? {}));
      } catch (error) {
        return Promise.reject(error);
      }
    },
    transformCallback: (callback) => {
      const id = Math.random();
      window[\`_\${id}\`] = callback;
      return id;
    },
    convertFileSrc: (path) => 'mock-asset://' + path,
  };

  // Tauri's event plugin goes through the same invoke path; intercept the two
  // commands it uses so listen() and emit() work.
  const rawInvoke = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = (command, args) => {
    if (command === 'plugin:event|listen') {
      const { event, handler } = args;
      const callback = window[\`_\${handler}\`];
      const existing = listeners.get(event) ?? [];
      existing.push(callback);
      listeners.set(event, existing);
      return Promise.resolve(existing.length);
    }
    if (command === 'plugin:event|unlisten') return Promise.resolve();
    if (command === 'plugin:event|emit' || command === 'plugin:event|emit_to') return Promise.resolve();
    return rawInvoke(command, args);
  };
})();
`;
}
