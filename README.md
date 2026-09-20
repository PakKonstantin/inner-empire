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

## Installing

There are no prebuilt downloads yet. Packaging runs on a `v*` tag and uploads
the installers as artifacts on the workflow run; no tag has been cut, so every
platform means building from source. That is three tools and one set of system
libraries, and the steps below are the whole of it.

| Platform | Status | Result |
|---|---|---|
| Linux x86_64 | Supported | `.deb`, `.rpm`, `.AppImage` |
| Windows x86_64 | Supported | NSIS `.exe`, `.msi` |
| iOS / iPadOS 17+ | Supported; a Mac is required to build it | An app you run from Xcode |
| macOS desktop | Not supported | — |

The desktop app has platform adapters for Linux and Windows only, so there is
no macOS build to install; a Mac is needed for the iOS client and nothing else.

### 1. Prerequisites, on every desktop platform

| | Version | Note |
|---|---|---|
| [Node.js](https://nodejs.org) | 20 or later | CI runs 22 |
| [pnpm](https://pnpm.io) | 10.33.0 | pinned by `packageManager`; Corepack installs it for you |
| [Rust](https://rustup.rs) | 1.80 or later | the workspace's `rust-version` |

```sh
corepack enable          # pnpm, at the version package.json pins
rustup default stable
```

### 2. System libraries

**Linux.** Tauri renders through WebKitGTK, so the development headers have to
be present at build time:

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

**Windows.** Install the "Desktop development with C++" workload from the
Visual Studio Build Tools — Rust links with MSVC and needs it. WebView2 ships
with Windows 11 and recent Windows 10; the installer this repository produces
bundles a bootstrapper for machines without it.

### 3. Run it from source

```sh
git clone https://github.com/pakkonstantin/inner-empire
cd inner-empire
pnpm install
pnpm tauri:dev
```

The first run compiles the Rust workspace and takes a few minutes; after that
it is incremental and the window reloads on save. The dev server is fixed to
port 1420 and fails rather than moving, because a webview pointed at a port
nothing is serving is a worse failure than a clear one.

### 4. Build an installer

```sh
pnpm tauri build                      # everything this platform can produce
pnpm tauri build --bundles deb        # just one
```

| Platform | Artifacts land in |
|---|---|
| Linux | `src-tauri/target/release/bundle/{deb,rpm,appimage}/` |
| Windows | `src-tauri\target\release\bundle\{nsis,msi}\` |

```sh
sudo apt install ./src-tauri/target/release/bundle/deb/*.deb
sudo dnf install ./src-tauri/target/release/bundle/rpm/*.rpm
chmod +x src-tauri/target/release/bundle/appimage/*.AppImage
```

The `.deb` and `.rpm` install a freedesktop `.desktop` entry and icons, so the
app appears in the launcher on GNOME, KDE and anything else following the
spec. The AppImage carries its own dependencies and needs no install at all.
On Windows, run the NSIS `.exe` (it installs per-machine) or the `.msi`.

Release builds happen on `ubuntu-22.04` so the Linux packages run on
distributions at least that old. Don't cross-compile: Tauri links against the
platform's native webview, and a second machine is less trouble than making
that work.

### 5. iOS and iPadOS

| | Version |
|---|---|
| macOS | 14+ |
| Xcode | 16+ |
| XcodeGen | 2.4+ |
| Rust | 1.80+ |

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
brew install xcodegen

cp ios/Config/Signing.xcconfig.example ios/Config/Signing.xcconfig
# fill in IE_DEVELOPMENT_TEAM and IE_BUNDLE_ID_PREFIX

./ios/Scripts/build-core.sh          # cargo → lipo → xcframework → bindings
cd ios && xcodegen generate          # project.yml → InnerEmpire.xcodeproj
open InnerEmpire.xcodeproj
```

The deployment target is iOS 17.0, and the app builds for both iPhone and
iPad, along with a share extension and a widget. `Signing.xcconfig` is
gitignored and every bundle identifier is derived from the prefix in it, so a
fork needs one untracked file and no source edits.

The bindings generator is vendored as a binary in the `ie-ffi` crate — there
is nothing to `cargo install`. `./ios/Scripts/build-core.sh --bindings` runs on
any host, which is how the generated Swift stays reviewable without a Mac;
everything else needs the iOS SDK, because the FFI crate pulls in SQLite and
compiles it from C.

**No Swift file in this repository has been compiled.** It was written in an
environment with no `swiftc`, and the checks that stand in for one catch names
that do not resolve, not types or concurrency. Expect a first Mac session to be
a compile session; `docs/ios/IMPLEMENTATION_PLAN.md` §9.1 says exactly what is
unverified and `docs/ios/TESTING.md` gives the order to work through it.

### 6. Portable mode

Put a file named `portable.txt` beside the executable and the app keeps its
config, data, logs and cache under `./data/` next to itself instead of in the
system directories — a vault on a USB stick and an app that leaves nothing
behind. It works on Linux and Windows alike.

### 7. Where the app keeps its own files

| | Linux (XDG) | Windows |
|---|---|---|
| Settings, recent vaults | `~/.config/inner-empire/` | `%APPDATA%\InnerEmpire\config\` |
| Data | `~/.local/share/inner-empire/` | `%APPDATA%\InnerEmpire\data\` |
| Logs | `~/.local/share/inner-empire/logs/` | `%LOCALAPPDATA%\InnerEmpire\logs\` |
| Cache | `~/.cache/inner-empire/` | `%LOCALAPPDATA%\InnerEmpire\cache\` |

Everything belonging to a vault — the index, the workspace layout, the trash,
plugins — lives in that vault's own `.inner-empire/` folder, so it travels with
the vault instead of being stranded on one machine. Deleting any of it costs
nothing but a rebuild.

### 8. When it does not build

| Symptom | Cause |
|---|---|
| A `-sys` crate fails its build script (`webkit2gtk`, `javascriptcore`, `soup`) | The Linux `-dev` packages in step 2 are missing |
| `link.exe not found` | The Visual Studio C++ workload is missing |
| `Port 1420 is already in use` | Another dev server is running; stop it rather than changing the port, which the webview also has to know |
| pnpm refuses to start over a version mismatch | Run `corepack enable`; `packageManager` pins the version deliberately |
| `build-core.sh` fails on anything but `--bindings`, off a Mac | Expected — that step needs the iOS SDK |

[DEVELOPMENT.md](DEVELOPMENT.md) covers the test commands and the layout of
the tree; [docs/ios/DEVELOPMENT.md](docs/ios/DEVELOPMENT.md) covers the iOS
side in the same detail.

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
