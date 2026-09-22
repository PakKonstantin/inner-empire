#!/usr/bin/env node
//
// Do the iOS docs name things that exist?
//
// docs/ios/ was written in phase 0, before the code, which is the right order
// — but it means the docs describe intentions, and intentions drift. Three had
// by the time anyone looked: TESTING.md named two round-trip tests that exist
// under different names, so following it ran nothing; the plan referred to a
// `MaterializeHook` and an `IosContainerDirs` that were never built under
// those names.
//
// A doc that names a function nobody can find is worse than no doc, because it
// is believed. This checks every backticked identifier the docs give for *our*
// code. Apple's API names and the explicitly-proposed-but-unbuilt designs are
// listed as exceptions, because they are legitimately absent.
//
//   node ios/Scripts/check-docs.mjs

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

function filesUnder(dir, extensions) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (entry === 'Generated' || entry === 'node_modules' || entry === 'target') continue;
    if (statSync(path).isDirectory()) out.push(...filesUnder(path, extensions));
    else if (extensions.includes(extname(path))) out.push(path);
  }
  return out;
}

// The frontend is in scope too: the iOS docs describe the shared workspace
// record, and the code that reads and writes it on the desktop lives in
// `src/`. Leaving it out produced a false positive on `graphState`, which is
// exactly the kind of noise that gets a checker switched off.
const source = filesUnder(join(repoRoot, 'ios'), ['.swift', '.yml', '.sh', '.mjs'])
  .concat(filesUnder(join(repoRoot, 'src-tauri/crates'), ['.rs']))
  .concat(filesUnder(join(repoRoot, 'src'), ['.ts', '.tsx']))
  .map((f) => readFileSync(f, 'utf8'))
  .join('\n');

// Apple's own names, which live in their SDK and not in this repository.
const appleNames = /^(NS|UI|CF|UT|BG|LS|XC)[A-Z]|^PrivacyInfo$/;

// Designs the docs describe as proposals rather than as built code. Each is
// named in a section that says so; they are not claims about this tree.
const proposals = new Set([
  'FileProviderSync', 'S3Sync', 'WebDAVSync', 'SyncProvider', 'SyncCapabilities',
  'RemoteChange', 'getChanges', 'resolveConflict', 'CorePluginAPI',
  'DesktopPluginCapabilities', 'iOSPluginCapabilities', 'ConformanceTests',
  'PollWatcher', 'RecommendedWatcher', 'ARCHITECTURE', 'APP_STORE', 'no_std',
]);

const problems = [];
const docsDir = join(repoRoot, 'docs/ios');
for (const name of readdirSync(docsDir).filter((f) => f.endsWith('.md'))) {
  const text = readFileSync(join(docsDir, name), 'utf8');
  const seen = new Set();
  for (const match of text.matchAll(/`([A-Za-z_][A-Za-z0-9_]*(?:::[a-z_]+)?(?:\(\))?)`/g)) {
    const identifier = match[1].replace('()', '').split('::')[0];
    // Short words and bare lowercase words are prose, not identifiers.
    if (identifier.length < 5) continue;
    if (identifier === identifier.toLowerCase() && !identifier.includes('_')) continue;
    if (appleNames.test(identifier) || proposals.has(identifier)) continue;
    if (seen.has(identifier)) continue;
    seen.add(identifier);
    if (!source.includes(identifier)) {
      problems.push(`${name} names \`${identifier}\`, which is nowhere in the tree`);
    }
  }
}

if (problems.length > 0) {
  console.error('The iOS docs name things that do not exist:\n');
  for (const problem of problems) console.error(`  - ${problem}`);
  console.error('\nRename the doc to match the code, or add it to the proposals list if it is a design rather than a claim.');
  process.exit(1);
}
console.log('Every identifier the iOS docs name for our own code exists.');
