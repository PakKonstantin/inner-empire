# Development

## Getting set up

You need Node.js 20 or later, pnpm, and a Rust toolchain from
[rustup](https://rustup.rs).

### Linux

Tauri renders through WebKitGTK, so the development headers must be present:

```sh
# Debian, Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
                 libayatana-appindicator3-dev patchelf build-essential

# Fedora
sudo dnf install webkit2gtk4.1-devel gtk3-devel librsvg2-devel \
                 libappindicator-gtk3-devel patchelf

# Arch
sudo pacman -S webkit2gtk-4.1 gtk3 librsvg libayatana-appindicator patchelf
```

Both GNOME and KDE Plasma are supported, and nothing in the code is specific to
either: desktop integration goes through the freedesktop `.desktop` entry and
icon theme specifications, which both consume.

### Windows

Install the "Desktop development with C++" workload from the Visual Studio
Build Tools. WebView2 ships with Windows 11 and recent Windows 10; the
installer bundles a bootstrapper for older machines.

### Both

```sh
pnpm install
pnpm tauri:dev
```

## Where things live

```
src/                  React frontend
├── app/              shell, layout, settings, commands
├── editor/           CodeMirror integration
├── markdown/         rendering, extensions, the conformance corpus's TS half
├── explorer/ search/ graph/ canvas/ workspace/ components/
├── plugins/          the plugin host
├── services/         the IPC facade — the only place invoke() is called
├── state/            zustand stores
└── types/            domain types mirroring the Rust model

src-tauri/
├── src/              the Tauri shell: commands, events, state
└── crates/
    ├── ie-core/      vault, Markdown, links, index, search, workspace
    └── ie-platform/  OS abstractions and per-platform adapters

tests/fixtures/       the shared Markdown conformance corpus
docs/                 user and plugin documentation
```

Two rules keep the layering honest, and both are mechanically checked:

- `ie-core` has no Tauri dependency and no `#[cfg(windows)]` or `#[cfg(unix)]`.
  Platform differences live in `ie-platform/src/platform/{linux,windows}` and
  nowhere else.
- Every `invoke()` call in the frontend is in `src/services/api.ts`. Nothing
  else imports from `@tauri-apps/api/core`.

## Running the tests

```sh
pnpm check:all          # typecheck, lint, frontend tests, Rust tests

pnpm typecheck          # tsc --noEmit
pnpm lint               # eslint, zero warnings allowed
pnpm test               # vitest
pnpm rust:test          # cargo test --workspace
pnpm rust:clippy        # clippy, warnings are errors
pnpm test:e2e           # playwright
```

### The Markdown conformance corpus

Two implementations parse the app's Markdown extensions: `ie-core::markdown` in
Rust, which is authoritative, and `src/markdown` in TypeScript, which renders.
`tests/fixtures/markdown/` holds `.md` files with matching `.expected.json`,
and **both** suites read the same corpus:

```sh
cargo test -p ie-core --test conformance
pnpm test src/markdown/conformance.test.ts
```

Adding a construct means adding a fixture first. A change made in one language
and not the other fails in the other, which is the point.

### What the Rust tests cover

- `ie-platform` — atomic writes, case-sensitivity probing, watcher event
  normalisation, path arithmetic
- `ie-core` unit tests — path validation, Markdown parsing and extraction,
  frontmatter typing, link resolution ranking, query parsing, fuzzy matching
- `tests/indexing.rs` — scanning, incremental updates, backlinks, rebuilds
- `tests/searching.rs` — the query language against a real index
- `tests/session.rs` — a real temporary vault: rename propagation, trash,
  crash-safety, index rebuild

Two of these deserve a mention because they check invariants rather than
behaviour. `rebuilding_the_index_from_scratch_reproduces_it_exactly` captures
the full derived state, drops every row, rescans, and asserts the snapshots
match — that is the "index is only a cache" rule, enforced.
`a_vault_authored_on_linux_opens_on_a_case_insensitive_filesystem` runs a
case-folding filesystem on a case-sensitive host, so Windows behaviour is
tested on a Linux runner.

## Building installers

```sh
pnpm tauri:build                      # everything the platform can produce
pnpm tauri build --bundles deb        # just one
```

| Platform | Artifacts | Where |
|---|---|---|
| Linux | `.deb`, `.rpm`, `.AppImage` | `src-tauri/target/release/bundle/` |
| Windows | NSIS `.exe`, `.msi` | `src-tauri\target\release\bundle\` |

Linux packages install a freedesktop `.desktop` entry and icons, so the app
appears in the launcher on GNOME, KDE and anything else following the spec.
The AppImage bundles its own dependencies and runs on any reasonably recent
distribution.

The Windows installer bundles the WebView2 bootstrapper, so a machine without
it still works; nothing asks the user to install Node.js or Rust.

Portable mode: put a file named `portable.txt` beside the executable and the
app keeps its settings, logs and cache in `./data/` instead of the system
directories.

### Cross-compiling

Don't. Build each platform on that platform — the CI matrix does exactly this.
Tauri links against the platform's native webview, and cross-compilation is
more trouble than a second runner.

## Where the app keeps its own files

| | Linux (XDG) | Windows |
|---|---|---|
| Settings, recent vaults | `~/.config/inner-empire/` | `%APPDATA%\InnerEmpire\config\` |
| Data | `~/.local/share/inner-empire/` | `%APPDATA%\InnerEmpire\data\` |
| Logs | `~/.local/share/inner-empire/logs/` | `%LOCALAPPDATA%\InnerEmpire\logs\` |
| Cache | `~/.cache/inner-empire/` | `%LOCALAPPDATA%\InnerEmpire\cache\` |

Per-vault state — the index, the workspace layout, the trash, plugins — lives
in the vault's own `.inner-empire/` folder instead, so it travels with the
vault rather than being stranded on one machine.

## Conventions

**TypeScript** runs in strict mode with `noUncheckedIndexedAccess`. `any` is a
lint error, not a warning: the typed IPC boundary is the point of having one.

**Rust** is checked with clippy at deny-warnings. Errors are typed enums with a
stable `code()`, never strings, because the UI branches on them.

**Comments** explain why, not what. A comment restating the code is noise; a
comment recording a decision — why the temp file is a sibling, why the FTS
table is not aliased, why a pin does not restore the tab's old position — is
what makes the next change safe.

**Tests** are named as sentences describing the behaviour, so a failure reads
as a statement about what broke.

## Adding a feature

Most features touch four places in the same order:

1. `ie-core` — the logic, with unit tests
2. `src-tauri/src/commands/` — a thin typed command
3. `src/services/api.ts` — one line
4. `src/` — the interface

If step 1 is hard to write without Tauri, the design is wrong. The core should
be testable with nothing but `cargo test`.
