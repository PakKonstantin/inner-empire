# Inner Empire

A local-first knowledge platform built on a plain folder of Markdown files.

Your notes are ordinary `.md` files in an ordinary directory. There is no
import step, no proprietary container, and no server. Everything the app adds —
links, backlinks, tags, typed properties, full-text search, a graph, a canvas —
is derived from those files and can be thrown away and rebuilt from them.

```
MyVault/
├── Notes/
├── Projects/
├── Attachments/
├── Templates/
├── Daily/
└── .inner-empire/      ← index, workspace layout, trash, plugins
```

Delete `.inner-empire/index.db` and the app rebuilds it on the next open. That
is not a recovery path bolted on afterwards; it is the property the whole
design is arranged around, and there is a test that deletes the index and
proves the rebuilt state matches byte for byte.

## What it does

**Writing.** A Markdown editor with live preview that conceals syntax on the
lines you are not editing — decorating the real document, so the cursor, the
selection and undo all keep working on the text you actually have. Lists
continue on Enter, brackets close, tables and task lists work, and the whole
of GitHub-flavoured Markdown renders.

**Linking.** `[[Wiki links]]`, with aliases, heading anchors, block references
and embeds. Renaming a note rewrites every link that pointed at it, preserving
aliases and anchors, and leaving links inside code blocks alone. Shortening a
link is refused when it would make the link ambiguous, so a move never quietly
repoints a reference.

**Finding.** Full-text search over SQLite's FTS5 with a small query language:
phrases, negation, `tag:`, `path:`, `file:`, `ext:`, `section:`, property
comparisons that compare numerically, and structural filters such as
`is:orphan`. A fuzzy quick switcher. A graph view, global and local.

**Organising.** Tags, including nested ones whose counts roll up. Typed
frontmatter properties with an editor per type. Templates with date variables.
Daily notes. A canvas for arranging notes and ideas spatially.

**Keeping.** Deletion moves a file to a trash folder inside the vault, so it
travels with the vault and can be recovered with any file manager. Every write
is atomic: a crash or a power cut can never leave a truncated note.

## Running it

Requires [Node.js 20+](https://nodejs.org), [pnpm](https://pnpm.io) and a
[Rust toolchain](https://rustup.rs).

```sh
pnpm install
pnpm tauri:dev
```

On Linux you also need the WebKitGTK development packages:

```sh
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev patchelf
```

On Windows you need the Microsoft C++ build tools and WebView2, which ships
with Windows 11 and recent Windows 10.

See [DEVELOPMENT.md](DEVELOPMENT.md) for the full setup, the test commands and
how to produce installers.

## How it is built

```
React + TypeScript          the window
        │  one typed IPC facade
Tauri 2                     the shell: commands, events, workers
        │  plain function calls
ie-core (Rust)              vault, Markdown, links, index, search, workspace
        │  trait objects only
ie-platform (Rust)          FileSystem, FileWatcher, Clock, dialogs, shell
        └── linux/ · windows/
```

The core has no Tauri dependency and no platform conditionals, which is what
lets the same code serve a future command-line tool, sync engine or headless
indexer. [ARCHITECTURE.md](ARCHITECTURE.md) explains the reasoning, the
invariants and the trade-offs.

## Documentation

| | |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | How it is put together and why |
| [DEVELOPMENT.md](DEVELOPMENT.md) | Setup, tests, builds, packaging |
| [USER_GUIDE.md](USER_GUIDE.md) | Using the app |
| [PLUGIN_API.md](PLUGIN_API.md) | Writing a plugin |

## Privacy

Nothing leaves your machine. There is no telemetry, no account, no sync and no
network client in the core. A plugin can only reach the network if its manifest
asks for it and you agree.

## Licence

MIT. See [LICENSE](LICENSE).

This is an independent implementation of publicly known note-taking concepts.
It is not affiliated with, derived from, or compatible with any other
application's proprietary formats or code.
