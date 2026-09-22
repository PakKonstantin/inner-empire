#!/usr/bin/env node
//
// What is actually inside the Linux package.
//
// A bundle configuration can be wrong in ways `cargo build` cannot see: a
// desktop template that did not get substituted, a MimeType that silently
// went missing, an icon installed nowhere the theme will look, a dependency
// named for the wrong distribution. None of it shows until someone installs
// the package, and by then the tag is pushed.
//
// So this opens the .deb and checks the things that have to be true:
//
//   node scripts/check-package.mjs [bundle directory]
//
// It is not a substitute for installing the package on a real desktop, which
// TESTING.md still asks for. It is the subset a machine can settle.

import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const bundleDir = resolve(process.argv[2] ?? join(root, 'src-tauri/target/release/bundle/deb'));

if (!existsSync(bundleDir)) {
  console.error(
    `No bundle directory at ${bundleDir}.\n\nBuild one first:  pnpm tauri build --bundles deb`,
  );
  process.exit(1);
}

const deb = readdirSync(bundleDir).find((name) => name.endsWith('.deb'));
if (!deb) {
  console.error(`No .deb in ${bundleDir}.`);
  process.exit(1);
}
const path = join(bundleDir, deb);

const contents = execFileSync('dpkg-deb', ['-c', path], { encoding: 'utf8' });
const control = execFileSync('dpkg-deb', ['-I', path], { encoding: 'utf8' });

/**
 * Read one file out of the package without unpacking it.
 *
 * Through a pipe rather than a temporary directory, and with the member name
 * passed to tar as an argument rather than interpolated into a shell string
 * — the desktop entry's filename contains a space.
 */
function fileInPackage(member) {
  const tarball = execFileSync('dpkg-deb', ['--fsys-tarfile', path], {
    maxBuffer: 512 * 1024 * 1024,
  });

  // Whether members carry a `./` prefix depends on how the archive was
  // written, and both forms are legal, so try each rather than guess.
  for (const name of [member, `./${member}`]) {
    try {
      const text = execFileSync('tar', ['-xO', name], {
        input: tarball,
        encoding: 'utf8',
        maxBuffer: 16 * 1024 * 1024,
        stdio: ['pipe', 'pipe', 'ignore'],
      });
      if (text.length > 0) return text;
    } catch {
      // Not under this name; try the other.
    }
  }
  return '';
}

const problems = [];
const checked = [];

function expect(description, condition) {
  checked.push(description);
  if (!condition) problems.push(description);
}

// `dpkg-deb -c` prints paths without a leading slash, and a name may contain
// spaces, so everything below matches to the end of the line rather than to
// the next space.
const listed = contents.split('\n');
const member = (pattern) => listed.find((line) => pattern.test(line));

// The binary, and the desktop entry that makes it appear in a launcher.
expect('the executable is installed', Boolean(member(/\busr\/bin\/inner-empire$/)));
expect('a desktop entry is installed', Boolean(member(/\busr\/share\/applications\/.+\.desktop$/)));

// Icons, in the hicolor theme where both GNOME and KDE look.
expect(
  'icons are installed into the hicolor theme',
  Boolean(member(/\busr\/share\/icons\/hicolor\/.+\.png$/)),
);

// The dependencies have to be named for Debian, not for Fedora.
expect('it depends on webkit2gtk', /Depends:.*libwebkit2gtk-4\.1-0/.test(control));
expect('it depends on gtk3', /Depends:.*libgtk-3-0/.test(control));

// The desktop entry itself: the template's placeholders must have been
// substituted, and the MimeType must have survived.
const desktopLine = member(/\busr\/share\/applications\/.+\.desktop$/);
const desktopName = desktopLine
  ? /usr\/share\/applications\/(.+\.desktop)$/.exec(desktopLine)?.[1]
  : undefined;
if (desktopName) {
  const entry = fileInPackage(`usr/share/applications/${desktopName}`);
  expect('the desktop entry was read', entry.length > 0);
  expect(
    'no unsubstituted template placeholders remain',
    entry.length === 0 || !/\{\{/.test(entry),
  );
  expect(
    'it advertises that it can open Markdown',
    /MimeType=.*text\/markdown/.test(entry),
  );
  expect('it has an Exec line', /^Exec=/m.test(entry));
  expect('it has an Icon line', /^Icon=/m.test(entry));
  // The window class, so the launcher groups the window with its own icon.
  expect('it sets StartupWMClass', /^StartupWMClass=/m.test(entry));

  // The bundle config's single `category` expands to one Categories value,
  // which is less than a text editor should claim. Hard-coded in the
  // template, and checked here so a future edit cannot quietly lose them.
  for (const category of ['Office', 'Utility', 'TextEditor']) {
    expect(`it is categorised as ${category}`, new RegExp(`^Categories=.*\\b${category};`, 'm').test(entry));
  }

  // Developer commentary in a file that lands in /usr/share is commentary
  // shipped to every user. One line pointing at the plan is the limit.
  const comments = entry.split('\n').filter((line) => line.startsWith('#'));
  expect('it carries at most a line or two of comment', comments.length <= 2);
}

// Tauri adds the webkit and gtk dependencies itself. Listing them again in
// the bundle config produced each one twice.
{
  const depends = /Depends:\s*(.+)/.exec(control)?.[1] ?? '';
  const names = depends.split(',').map((name) => name.trim()).filter(Boolean);
  expect('no dependency is listed twice', new Set(names).size === names.length);
}

// Nothing that needs root at runtime, and no service installed behind the
// user's back.
expect('it installs no systemd unit', !/systemd\//.test(contents));
expect('it installs nothing into /etc', !listed.some((line) => /\s\.?\/?etc\//.test(line)));
expect(
  'no file is setuid or setgid',
  !contents.split('\n').some((line) => /^[-d][rwx-]{2}[sS]|^[-d][rwx-]{5}[sS]/.test(line)),
);

if (problems.length > 0) {
  console.error(`${deb} is not right:\n`);
  for (const problem of problems) console.error(`  ✗ ${problem}`);
  console.error(`\n${problems.length} of ${checked.length} checks failed.`);
  process.exit(1);
}

console.log(`${deb}: all ${checked.length} checks pass.`);
for (const line of checked) console.log(`  ✓ ${line}`);
