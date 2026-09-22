#!/usr/bin/env node
//
// SHA256SUMS for a release.
//
// Someone downloading an installer from the internet should be able to tell
// whether they got the file the project built. That needs two things: a list
// of hashes, and the list being hard to tamper with. This writes the list;
// the release job signs it with GPG when a key is available.
//
//   node scripts/checksums.mjs dist-release
//
// The format is the one `sha256sum -c` reads, so verifying is a command
// people already know rather than instructions they have to follow.

import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';

const directory = resolve(process.argv[2] ?? 'dist-release');

/** The extensions a release actually ships. */
const ARTIFACTS = /\.(exe|msi|deb|rpm|AppImage|zip|tar\.gz)$/i;

let entries;
try {
  entries = readdirSync(directory);
} catch {
  console.error(`No such directory: ${directory}`);
  process.exit(1);
}

const files = entries
  .filter((name) => ARTIFACTS.test(name))
  .filter((name) => statSync(join(directory, name)).isFile())
  // Sorted, so two runs over the same artifacts produce identical bytes and
  // a diff of the file means the artifacts changed.
  .sort((a, b) => a.localeCompare(b));

if (files.length === 0) {
  console.error(
    `Nothing to checksum in ${directory}.\n` +
      'Expected installers or archives; the release job collects them there first.',
  );
  process.exit(1);
}

const lines = files.map((name) => {
  const digest = createHash('sha256').update(readFileSync(join(directory, name))).digest('hex');
  // Two spaces and the bare filename: `sha256sum -c` wants the name as it
  // sits beside the sums file, not a path from somewhere else.
  return `${digest}  ${basename(name)}`;
});

const output = join(directory, 'SHA256SUMS');
writeFileSync(output, `${lines.join('\n')}\n`);

console.log(`${output} — ${files.length} ${files.length === 1 ? 'artifact' : 'artifacts'}`);
for (const line of lines) console.log(`  ${line}`);
