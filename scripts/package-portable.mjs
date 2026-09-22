#!/usr/bin/env node
//
// The portable Windows build.
//
// A folder you can drop on a USB stick: no installer, no registry, nothing
// left behind on the machine you ran it from. `ie-platform` switches to
// portable mode when it finds `portable.txt` beside the executable, and then
// keeps config, data, logs and cache under `data/` in that same folder.
//
// Run after a release build:
//
//   pnpm tauri build --bundles nsis
//   node scripts/package-portable.mjs
//
// Produces dist-release/InnerEmpire-<version>-windows-x64-portable.zip
//
// The zip is built with the system `zip` where it exists and PowerShell's
// Compress-Archive on Windows, rather than by adding an archiver dependency
// for one file a release job writes once.

import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;

const BINARY = 'Inner Empire.exe';
const release = join(root, 'src-tauri/target/release');
const outDir = join(root, 'dist-release');
const stageName = `InnerEmpire-${version}-windows-x64-portable`;
const stage = join(outDir, stageName);
const archive = `${stage}.zip`;

const binary = join(release, BINARY);
if (!existsSync(binary)) {
  console.error(
    `No release binary at ${binary}.\n\n` +
      'Build it first:  pnpm tauri build --bundles nsis\n' +
      'This script packages what that produced; it does not build anything.',
  );
  process.exit(1);
}

rmSync(stage, { recursive: true, force: true });
rmSync(archive, { force: true });
mkdirSync(stage, { recursive: true });

copyFileSync(binary, join(stage, BINARY));

// The marker itself. Its contents are for whoever opens it; only the name
// matters to the application.
writeFileSync(
  join(stage, 'portable.txt'),
  [
    'This file is what makes Inner Empire portable.',
    '',
    'While it sits beside the executable, every file the application writes —',
    'settings, the search index, logs — goes into the data folder here rather',
    'than into your user profile. Delete this file and Inner Empire behaves',
    'like an ordinary installed copy, using the usual Windows locations.',
    '',
    'Your notes are not kept here. A vault is a folder you choose, wherever',
    'you choose it, and moving this stick does not move or break it.',
    '',
  ].join('\n'),
);

writeFileSync(
  join(stage, 'README.txt'),
  [
    `Inner Empire ${version} — portable`,
    '='.repeat(40),
    '',
    'Run "Inner Empire.exe". There is nothing to install.',
    '',
    'What portable means here',
    '------------------------',
    'Settings, the search index and logs are written to the data folder next',
    'to the executable, not to your user profile, and nothing is written to',
    'the registry. Removing this folder removes every trace of the',
    'application from the machine.',
    '',
    'Your notes',
    '----------',
    'A vault is an ordinary folder of Markdown files that you pick. It is not',
    'inside this folder unless you put it there, and deleting this folder',
    'does not touch it.',
    '',
    'One requirement',
    '---------------',
    'Windows 10 and 11 ship with WebView2, which this application needs. On',
    'an older or stripped-down system you may have to install the Evergreen',
    'WebView2 Runtime from Microsoft first. The installer version handles',
    'that for you; a portable build cannot.',
    '',
    'File associations',
    '-----------------',
    'A portable copy changes nothing about your system by default. If you',
    'turn on the options under Settings → Files and links, it writes the same',
    'per-user registry entries an installed copy would, and turning them off',
    'removes them again.',
    '',
  ].join('\n'),
);

// `data/` is created by the application on first run. An empty directory
// cannot go into a zip anyway, and a stale one shipped with the archive
// would be a place for someone else's settings to travel.

function zip() {
  if (process.platform === 'win32') {
    execFileSync(
      'powershell',
      [
        '-NoProfile',
        '-Command',
        `Compress-Archive -Path '${stage}\\*' -DestinationPath '${archive}' -Force`,
      ],
      { stdio: 'inherit' },
    );
    return;
  }
  // -r recurse, -q quiet; run from the output directory so the archive holds
  // one folder rather than a chain of parent directories.
  execFileSync('zip', ['-rq', archive, stageName], { cwd: outDir, stdio: 'inherit' });
}

try {
  zip();
} catch (error) {
  console.error(
    `Could not create the archive: ${error.message}\n\n` +
      'Needs `zip` on Linux and macOS, or PowerShell on Windows.',
  );
  process.exit(1);
}

rmSync(stage, { recursive: true, force: true });
console.log(`${archive}`);
