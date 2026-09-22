# Release testing

What a machine has already settled, and what still needs a person at a real
desktop. The second list is short on purpose, but nothing on it can be
skipped: every item is something that has shipped broken in some application
because everyone assumed someone else had checked it.

---

## 1. Already automated

These run in CI on every push. If they pass, the thing they describe is true
of the artifacts, not merely intended.

| Check | What it settles |
|---|---|
| `pnpm version:check` | The installer, the crate and the About box all claim the same version |
| `pnpm check:contrast` | Every colour pair in both themes meets WCAG AA |
| `pnpm package:check` | The built `.deb` contains the binary, a desktop entry with `MimeType`, icons in the hicolor theme, correctly named dependencies, no systemd unit, nothing in `/etc`, nothing setuid |
| `pnpm uninstall:check` | Removal cannot reach a vault — see §2 |
| `pnpm test` / `pnpm test:e2e` | The application's own behaviour, including a walk that fails on any console output |

---

## 2. The data-preservation test

**The rule with no acceptable failure mode.** A vault is work that may exist
nowhere else. An uninstaller that removes one has destroyed something a
backup may not have.

### Debian — settled, and settled strongly

Run against a built package:

```
pnpm tauri build --bundles deb
pnpm uninstall:check
```

It asserts two things that together make the guarantee structural rather
than behavioural:

1. **The package ships no maintainer scripts.** `preinst`, `postinst`,
   `prerm`, `postrm` are all absent, so removal runs no code of ours at all.
2. **Every installed file is under `/usr`.** `dpkg -r` removes what the
   package installed and nothing else.

With no code to run and nothing owned outside `/usr`, removal *cannot* reach
`$HOME`. That is a stronger statement than reading a script and believing it.

This was also verified by hand, once, end to end: install the package,
create a vault with a note and an attachment outside it, write settings,
`dpkg -r inner-empire`, then confirm the binary and desktop entry are gone
and every user file is byte-identical. It passed. The automated check is what
keeps it true.

### Windows — asserted statically, still needs a run

`pnpm uninstall:check` reads `src-tauri/installer/hooks.nsh` with comments
stripped, collects every `RMDir` and `Delete` target, and requires that the
only one is `$LOCALAPPDATA\InnerEmpire\cache`. It names the forbidden ones
explicitly — the data directory, Documents, the user profile, anything
vault-shaped, anything matching `*.md` — so a later edit trips the check
rather than shipping.

It cannot run the uninstaller. **Do this on a real Windows machine before
any public release:**

- [ ] Install. Create a vault somewhere ordinary — `Documents\Notes`.
- [ ] Write a note, attach an image, change a setting, add a hotkey.
- [ ] Note the vault's size and a file's hash.
- [ ] Uninstall from Settings → Apps.
- [ ] The vault folder is **untouched**: same files, same hashes.
- [ ] `%APPDATA%\InnerEmpire\config` still holds the settings.
- [ ] `%LOCALAPPDATA%\InnerEmpire\cache` is gone.
- [ ] Reinstall. The settings and hotkeys are still there.

---

## 3. What still needs a real desktop

Nothing below can be reached from a container with no display, no Windows,
and no package manager for the formats involved.

### Windows

- [ ] The NSIS installer runs, and the wizard's pages read correctly.
- [ ] Installing **without admin rights** works — `installMode` is `both`.
- [ ] After install, Inner Empire appears under "Open with" for a `.md`
      file, and double-clicking one still opens whatever opened it before.
      *The installer must not have changed the default.*
- [ ] Settings → Files and links → System integration: turning on "Open
      Markdown files with Inner Empire by default" changes the default and
      Explorer's icon updates without a sign-out.
- [ ] Turning it off again restores the previous handler.
- [ ] The folder context-menu entry appears, opens the folder as a vault,
      and disappears when turned off.
- [ ] Upgrade in place over an existing install: settings, hotkeys and the
      recent-vault list survive.
- [ ] The portable zip runs from a USB stick, writes only into its own
      `data/` folder, and leaves no registry keys behind.
- [ ] SmartScreen's warning on an unsigned build is the expected one, not a
      block.

### Linux

- [ ] The AppImage runs on a distribution other than the one it was built
      on. **Not built in this environment** — `appimagetool` is absent.
- [ ] The `.rpm` installs on Fedora. **Not built here** — `rpmbuild` absent.
- [ ] The desktop entry appears in the launcher with its own icon, and the
      window groups with it rather than showing a second generic entry.
- [ ] `xdg-mime query default text/markdown` is **unchanged** by installing
      the package.

### Both

- [ ] A vault of ten thousand notes opens, searches and scrolls without the
      interface stalling. The end-to-end suite covers four thousand in one
      folder, which exercises the windowing but is not the same as ten
      thousand on a slow disk.
- [ ] A screen reader reads the interface in a sensible order. The mechanical
      subset is checked; reading order is not something a script can judge.

---

## 4. Before publishing a release

The pipeline opens a **draft**. Publishing is a human action, and this is
what that human should have done first:

- [ ] Every artifact downloaded and `sha256sum -c SHA256SUMS` passes.
- [ ] The Windows installer's signature checked, or the release notes say
      plainly that it is unsigned and why.
- [ ] §2 done on Windows, against **this** build.
- [ ] The version in the About box matches the tag.
- [ ] Release notes written by a person.
