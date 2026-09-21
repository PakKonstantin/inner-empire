# Release implementation plan

How Inner Empire becomes something a person downloads, installs, updates and
removes — without ever putting their notes at risk.

The one rule everything below is arranged around: **an installer may never
delete a vault.** Notes are the user's files in the user's folder. The
application is disposable; the vault is not.

---

## 1. What is here today

### 1.1 Bundling

`src-tauri/tauri.conf.json` already declares:

```json
"bundle": {
  "targets": ["deb", "rpm", "appimage", "nsis", "msi"],
  "linux": {
    "deb":      { "depends": ["libwebkit2gtk-4.1-0", "libgtk-3-0"], "section": "utils" },
    "rpm":      { "depends": ["webkit2gtk4.1", "gtk3"] },
    "appimage": { "bundleMediaFramework": true }
  },
  "windows": {
    "nsis": { "installMode": "perMachine", "languages": ["English"] },
    "webviewInstallMode": { "type": "downloadBootstrapper" }
  }
}
```

So the five artifact types are configured. What is missing is everything
around them: the wizard has no pages beyond the default, nothing is
associated with `.md`, there is no portable build, no checksums, no release,
and the uninstaller has no opinion about what it may remove.

### 1.2 CI

`.github/workflows/ci.yml` runs on every push: frontend checks, Rust on
`ubuntu-22.04` and `windows-latest`, the iOS bridge, Markdown conformance, a
desktop build on both targets, and Playwright end-to-end.

`.github/workflows/release.yml` runs on a `v*` tag: builds with
`pnpm tauri build` on both runners and uploads the bundles as **workflow
artifacts**. It does not create a GitHub Release, does not produce checksums,
and does not build a portable archive. No tag has ever been pushed.

### 1.3 Versions

The version `0.1.0` is written in **four** places:

| Where | Line |
|---|---|
| `package.json` | `"version": "0.1.0"` |
| `src-tauri/tauri.conf.json` | `"version": "0.1.0"` |
| `src-tauri/Cargo.toml` | `[workspace.package] version = "0.1.0"` |
| `src/app/App.tsx` | `const APP_VERSION = '0.1.0'` |

Four places is three too many (§48).

### 1.4 Portable mode already exists

`ie-platform::dirs::PortableDirs` looks for a file named `portable.txt`
beside the executable and, when it is there, puts config, data, logs and
cache under `./data/` instead of the system directories. The behaviour is
built and tested; what is missing is an archive that ships with the marker.

---

## 2. One version, one source

`package.json` becomes the source of truth. A script propagates it:

```
package.json  →  src-tauri/tauri.conf.json   (bundle + about box)
              →  src-tauri/Cargo.toml        (workspace.package.version)
              →  src/version.ts              (generated, imported by the UI)
```

- `pnpm version:sync` writes the three derived places.
- `pnpm version:check` fails when any of them disagrees, and runs in CI next
  to the other generated-file checks (the repository already has this pattern
  for the iOS bindings and design tokens).
- `App.tsx`'s hard-coded constant is replaced by the generated import.

Bumping a version is then editing one number and running one script.

---

## 3. Windows

### 3.1 NSIS, not WiX

Both are available in Tauri v2. NSIS is the right choice here:

| | NSIS | WiX / MSI |
|---|---|---|
| Custom wizard pages | Yes, via `installerHooks` | Only with authored extensions |
| Per-user install without admin | Yes | Awkward |
| Optional components (shortcuts, associations) | Straightforward | Feature-table gymnastics |
| Already configured in this repo | Yes | Target present, unconfigured |

The `.msi` target stays — it is what an IT department deploys by policy — but
the interactive installer people download is the NSIS `.exe`.

### 3.2 The wizard

```
Welcome  →  Licence  →  Install location  →  Components  →  Install  →  Finish
                                              │                          │
                          Desktop shortcut ───┤            Launch app ───┘
                          Start-menu entry ───┤
                     Open .md with this app ──┤   (unchecked by default)
                       "Open with" in the     │
                        folder context menu ──┘   (unchecked by default)
```

`installMode` changes from `perMachine` to `both`, so a user without admin
rights can still install for themselves. The two integration options are
**unchecked by default**: the brief is explicit that file associations and
shell integration are not to be taken without asking, and an application that
seizes `.md` on install is the kind of thing people uninstall.

### 3.3 File associations and the protocol

Declared in the bundle config so the binary knows about them:

```json
"fileAssociations": [
  { "ext": ["md", "markdown"], "name": "Markdown", "description": "Markdown document", "role": "Editor" }
]
```

On Windows the *capability* is registered (the app appears under "Open
with"); becoming the **default** handler happens only when the user ticks the
box, and is implemented in the NSIS hook by writing the
`HKCU\Software\Classes\.md` association. Removing it on uninstall is part of
the same hook.

The `inner-empire://` protocol is registered the same way — capability
always, default only on request — and lets a note link open the app.

### 3.4 Uninstall

The uninstaller removes:

- the application binaries and its install directory,
- the Start-menu and desktop shortcuts,
- registry keys it created (associations, protocol, uninstall entry),
- the **cache** directory (`%LOCALAPPDATA%\InnerEmpire\cache`).

It offers, unchecked, "also remove settings and logs". It never touches
`%APPDATA%\InnerEmpire\data` without that box, and it has no code path that
can reach a vault: vaults live wherever the user put them and the uninstaller
is not told where that is.

### 3.5 Upgrade

NSIS detects an existing install by its registry key and upgrades in place:
stop the running instance, replace binaries, keep the install directory,
keep every user directory. Settings are forward-compatible by construction —
they are read with `serde(default)`, the same rule the workspace file follows.

### 3.6 Portable ZIP

`scripts/package-portable.mjs` assembles, after a release build:

```
InnerEmpire-<version>-windows-x64-portable.zip
├── Inner Empire.exe
├── portable.txt          ← the marker ie-platform looks for
├── data/                 ← created on first run: config, data, logs, cache
└── README.txt            ← what portable mode means, WebView2 requirement
```

No registry writes, no install, runs from a USB stick. Vault paths are
absolute or relative to the vault itself, never to the executable, so moving
the stick does not break a vault.

---

## 4. Linux

### 4.1 Artifacts

| Format | Produced by | Notes |
|---|---|---|
| `.AppImage` | Tauri (`bundleMediaFramework: true`) | Self-contained, no install |
| `.deb` | Tauri | Depends on `libwebkit2gtk-4.1-0`, `libgtk-3-0` |
| `.rpm` | Tauri | Depends on `webkit2gtk4.1`, `gtk3` |

Built on `ubuntu-22.04` so the glibc floor is as old as the runners allow.

### 4.2 Desktop integration

A `.desktop` entry shipped with the deb and rpm:

```ini
[Desktop Entry]
Type=Application
Name=Inner Empire
Comment=A local-first knowledge platform built on plain Markdown files
Exec=inner-empire %F
Icon=inner-empire
Categories=Office;Utility;TextEditor;
MimeType=text/markdown;text/x-markdown;
StartupWMClass=Inner Empire
```

`MimeType` advertises that the app *can* open Markdown. It does not make it
the default — that is `xdg-mime default`, which is the user's to run. Icons
are installed into the hicolor theme at 32, 128 and 256px from the PNGs the
bundle already carries.

Nothing GNOME- or KDE-specific: the `.desktop` and icon-theme specifications
are what both read.

### 4.3 Privileges

The AppImage needs none. The deb and rpm need the privileges any package
install needs, and nothing more — no setuid, no system service, no root at
runtime. The application writes only under `$XDG_CONFIG_HOME`,
`$XDG_DATA_HOME`, `$XDG_CACHE_HOME` and the vault.

### 4.4 Uninstall

`apt remove` / `dnf remove` takes the binaries, the `.desktop` entry and the
icons. Package scripts do not touch `~/.config/inner-empire`,
`~/.local/share/inner-empire` or any vault. `apt purge` removes the
application's own config only.

---

## 5. Checksums and signing

### 5.1 Checksums

`scripts/checksums.mjs` walks the collected artifacts and writes a single
`SHA256SUMS` in the standard `sha256sum -c` format. It is attached to the
release, so a download can be verified without trusting the transport.

### 5.2 Windows code signing

The pipeline is built for it and works without it:

```
secrets.WINDOWS_CERTIFICATE present  →  Tauri signs the exe and installer
                            absent   →  unsigned artifacts, build succeeds,
                                        a warning in the job summary
```

Certificates never enter the repository. `tauri.conf.json` gains a
`signCommand` placeholder driven by environment variables that CI supplies
from secrets.

### 5.3 Linux signing

`SHA256SUMS` always. When `secrets.GPG_PRIVATE_KEY` exists, it is also signed
detached to `SHA256SUMS.asc`. Absent, the release still happens.

---

## 6. Pipelines

### 6.1 CI (every push) — already exists, extended

Added to the existing jobs: `pnpm version:check`, and a packaging smoke test
on Linux that runs `pnpm tauri build --bundles deb` so a broken bundle
configuration fails on the branch rather than at release time.

### 6.2 Release (on `v*` tag)

```
        tag v1.2.3
            │
   ┌────────┴────────┐
   │                 │
windows-latest   ubuntu-22.04
   │                 │
 lint/test        lint/test
 build            build
 nsis + msi       deb + rpm + AppImage
 portable zip     │
   └────────┬─────┘
            │
      collect artifacts
            │
      SHA256SUMS (+ .asc)
            │
   GitHub Release (draft)
```

The release is created as a **draft**. Publishing is a human action, and
nothing is pushed to any store automatically (§52).

### 6.3 Channels

Decided by the tag, with no update server required yet:

| Tag | Channel |
|---|---|
| `v1.2.3` | stable |
| `v1.2.3-beta.1` | beta — release marked pre-release |
| `nightly-YYYYMMDD` | nightly — scheduled, artifacts expire |

The updater plugin is configured but points at no endpoint, so adding
auto-update later is an endpoint plus a signing key, not a redesign.

---

## 7. Testing

### 7.1 What can be checked here

This container is Linux without a display. `dpkg-deb` is present,
`makensis`, `appimagetool` and `rpmbuild` are not. So:

- deb construction and contents: **checkable locally**,
- NSIS, AppImage, rpm: **CI only**,
- installing, launching, uninstalling on a real desktop: **neither** — those
  are documented as manual gates below, and this plan does not pretend
  otherwise.

### 7.2 Installer test matrix

**Windows** — clean install; upgrade over an older version; uninstall;
reinstall; portable archive; file association on and off; shortcuts on and
off; install without admin rights.

**Linux** — AppImage on a clean system; deb install and remove; rpm install
and remove; launcher entry appears; icon appears; `.md` offered under "Open
with"; upgrade over an older package.

### 7.3 The data-preservation test

The one that must never fail. Scripted where the platform allows, manual
where it does not:

```
1. Create a vault with notes, attachments and a workspace layout
2. Record a hash of every file in it
3. Install the application
4. Launch, open the vault, edit a note, close
5. Uninstall
6. Reinstall
7. Compare: every file except the one edited is byte-identical;
   the edit is present; .inner-empire/ is intact
```

### 7.4 Release checklist

Kept in the repository as `docs/RELEASE_CHECKLIST.md` and ticked per release:
tests, build, each installer, version, icons, shortcuts, associations,
uninstaller, vault preserved, checksums, documentation.

---

## 8. Security

- No telemetry, no analytics, no tracker, and the core has no network client
  at all. The only outbound request the shipped app can make is the WebView2
  bootstrapper on Windows, during install, if WebView2 is absent.
- `cargo audit` and `pnpm audit` run in CI and report; they do not gate the
  build on an advisory in a transitive development dependency.
- The webview's CSP is already restrictive (`default-src 'self'`, no remote
  script or style).
- Artifact integrity is the user's to verify, which is what `SHA256SUMS` is
  for.

---

## 9. Phases

| Phase | Work |
|---|---|
| 11 | Version single source; NSIS configuration, hooks, associations, uninstall rules; portable archive |
| 12 | AppImage verification and the desktop entry |
| 13 | deb and rpm: desktop entry, icons, MIME, dependency check |
| 14 | CI: version check, packaging smoke test, release workflow, checksums, signing hooks |
| 15 | Release testing: the matrix above, the data-preservation test, the checklist |

## 10. Definition of done

**Windows**: `installer.exe` → install → launch → create or open a vault →
uninstall → the vault is byte-identical.

**Linux**: AppImage or `.deb` → launch → create or open a vault → remove →
the vault is byte-identical.

**Both**: one version number, artifacts with checksums, a draft release
created by a tag, and a pipeline that signs when it is given a key and builds
when it is not.
