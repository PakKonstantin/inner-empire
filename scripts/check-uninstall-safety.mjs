#!/usr/bin/env node
//
// That uninstalling cannot reach anything the user wrote.
//
// This is the one rule in the brief with no acceptable failure mode. A
// person's notes are theirs; an uninstaller that removes a vault has
// destroyed work that may exist nowhere else. So rather than promise it in a
// document, it is asserted here, on the artifacts themselves, every build.
//
//   node scripts/check-uninstall-safety.mjs
//
// Two things are checked, because the two platforms fail differently.
//
// Debian: a package removes what it installed, and runs its maintainer
// scripts. If every installed file is under /usr and there are no maintainer
// scripts, `dpkg -r` has no way to reach $HOME — not "does not", *cannot*.
// That is a stronger statement than reading a script and believing it.
//
// Windows: the uninstaller is a script, so the script is read. What matters
// is not what it does but what it must never mention.

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const problems = [];
const checked = [];

function expect(description, condition, detail) {
  checked.push(description);
  if (!condition) problems.push(detail ? `${description} — ${detail}` : description);
}

// ---------------------------------------------------------------------------
// Debian
// ---------------------------------------------------------------------------

const debDir = join(root, 'src-tauri/target/release/bundle/deb');
const deb = existsSync(debDir) ? readdirSync(debDir).find((n) => n.endsWith('.deb')) : undefined;

if (!deb) {
  console.log('No .deb built; skipping the Debian checks.');
  console.log('  Build one first:  pnpm tauri build --bundles deb\n');
} else {
  const path = join(debDir, deb);
  const contents = execFileSync('dpkg-deb', ['-c', path], { encoding: 'utf8' });

  // Maintainer scripts are the only way a Debian package can run code at
  // removal. None means there is nothing to audit and nothing to go wrong.
  const controlDir = join(root, 'src-tauri/target/release/.uninstall-audit');
  execFileSync('rm', ['-rf', controlDir]);
  execFileSync('dpkg-deb', ['-e', path, controlDir]);
  const scripts = readdirSync(controlDir).filter((name) =>
    ['preinst', 'postinst', 'prerm', 'postrm', 'config'].includes(name),
  );

  if (scripts.length === 0) {
    expect('the package runs no maintainer scripts, so removal cannot run code', true);
  } else {
    // If scripts appear later, they are read rather than trusted.
    for (const name of scripts) {
      const text = readFileSync(join(controlDir, name), 'utf8');
      expect(
        `${name} does not reach into a home directory`,
        !/\$HOME|~\/|\/home\/|getent passwd|\/root\//.test(text),
        'a maintainer script that walks user directories can delete a vault',
      );
    }
  }
  execFileSync('rm', ['-rf', controlDir]);

  // Every payload path must be under /usr. Anything else is a file dpkg will
  // remove that is not the application's to own.
  const payload = contents
    .split('\n')
    .filter((line) => line.trim().length > 0)
    // `drwxr-xr-x 0/0  0 date time path` — the path is the last field, and
    // may contain spaces, so take everything after the time.
    .map((line) => /\d\d:\d\d\s+(.+)$/.exec(line)?.[1])
    .filter((entry) => entry !== undefined)
    .map((entry) => entry.replace(/^\.?\//, ''));

  // `usr` itself appears as a directory entry without a trailing slash.
  const outside = payload.filter(
    (entry) => entry !== '' && entry !== 'usr' && !entry.startsWith('usr/'),
  );
  expect(
    'every installed file is under /usr, so removal cannot reach a vault',
    outside.length === 0,
    outside.slice(0, 5).join(', '),
  );
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

const hooks = join(root, 'src-tauri/installer/hooks.nsh');
expect('the Windows installer hooks are present', existsSync(hooks));

if (existsSync(hooks)) {
  const text = readFileSync(hooks, 'utf8');

  // What the uninstaller removes, with the comments stripped so a path
  // discussed in prose is not mistaken for one that is acted on.
  const code = text
    .split('\n')
    .filter((line) => !line.trim().startsWith(';'))
    .join('\n');

  const removals = [...code.matchAll(/(?:RMDir(?:\s+\/r)?|Delete)\s+(?:\/REBOOTOK\s+)?"([^"]+)"/gi)].map(
    (match) => match[1],
  );

  // The only directory the uninstaller may delete outright. The cache is
  // rebuilt from the notes, so losing it costs nothing.
  const ALLOWED = [/^\$LOCALAPPDATA\\InnerEmpire\\cache$/i];

  for (const target of removals) {
    expect(
      `the uninstaller may remove ${target}`,
      ALLOWED.some((pattern) => pattern.test(target)),
      'only the cache may be removed; settings and data are kept, and a vault is never known',
    );
  }

  // Named explicitly, because these are the mistakes that would matter and
  // a reviewer skimming a diff would not necessarily catch them.
  const FORBIDDEN = [
    [/\$APPDATA\\InnerEmpire\\data/i, 'the data directory'],
    [/\$DOCUMENTS/i, "the user's Documents folder"],
    [/\$PROFILE/i, 'the user profile'],
    [/\$DESKTOP\\/i, 'the desktop'],
    [/vault/i, 'anything vault-shaped'],
    [/\*\.md/i, 'Markdown files'],
  ];

  for (const [pattern, description] of FORBIDDEN) {
    const offending = removals.filter((target) => pattern.test(target));
    expect(
      `the uninstaller never removes ${description}`,
      offending.length === 0,
      offending.join(', '),
    );
  }

  // The uninstaller keeps settings. Checked as an absence rather than a
  // comment, so someone "tidying up" later trips this rather than shipping.
  expect(
    'settings and hotkeys survive an uninstall',
    !/RMDir[^\n]*\$APPDATA\\InnerEmpire\\config/i.test(code),
    'a reinstall should find the preferences it left behind',
  );
}

if (problems.length > 0) {
  console.error('Uninstall is not safe:\n');
  for (const problem of problems) console.error(`  ✗ ${problem}`);
  console.error(
    `\n${problems.length} of ${checked.length} checks failed. ` +
      'This is the rule with no acceptable failure mode: a vault is work that ' +
      'may exist nowhere else.',
  );
  process.exit(1);
}

console.log(`Uninstall is safe: all ${checked.length} checks pass.`);
for (const line of checked) console.log(`  ✓ ${line}`);
