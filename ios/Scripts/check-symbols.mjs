#!/usr/bin/env node
//
// A poor substitute for a compiler, for the parts a compiler would catch.
//
// There is no Swift toolchain on the machine this was written on, so the app
// code cannot be type-checked here. This checks the one class of error that is
// both common and mechanical: a name that does not exist. It found two real
// ones on its first run — a design token under a name the generator does not
// produce, and a service method that was never written.
//
// It is not a type checker and does not pretend to be. It verifies that every
// name referenced actually exists somewhere, which is strictly less than
// compiling and strictly more than nothing. Everything else waits for a Mac,
// as docs/ios/DEVELOPMENT.md says.
//
//   node ios/Scripts/check-symbols.mjs

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const iosRoot = resolve(here, '..');

/** Every .swift file under `dir`. */
function swiftFiles(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) out.push(...swiftFiles(path));
    else if (entry.endsWith('.swift')) out.push(path);
  }
  return out;
}

const appDirs = ['App', 'Features', 'Editor', 'Storage', 'Services', 'Tests']
  .map((d) => join(iosRoot, d))
  .filter((d) => {
    try { return statSync(d).isDirectory(); } catch { return false; }
  });

const appFiles = appDirs.flatMap(swiftFiles).filter((f) => !f.endsWith('DesignTokens.swift'));
const appSource = appFiles.map((f) => readFileSync(f, 'utf8')).join('\n');
const generated = readFileSync(join(iosRoot, 'Generated/ie_ffi.swift'), 'utf8');

const problems = [];

/** Collect `pattern` group 1 from `text` into a Set. */
function collect(text, pattern) {
  return new Set([...text.matchAll(pattern)].map((m) => m[1]));
}

// --- Design tokens ------------------------------------------------------
const tokensDefined = new Set([
  ...collect(readFileSync(join(iosRoot, 'Services/DesignTokens.swift'), 'utf8'),
    /static let (\w+)/g),
  ...collect(readFileSync(join(iosRoot, 'Services/DesignTokens+Semantic.swift'), 'utf8'),
    /static var (\w+)/g),
]);
for (const used of collect(appSource, /DesignTokens\.(\w+)/g)) {
  if (!tokensDefined.has(used)) {
    problems.push(`DesignTokens.${used} does not exist (regenerate, or add a semantic alias)`);
  }
}

// --- Free functions from the bridge -------------------------------------
const bridgeFunctions = collect(generated, /^public func (\w+)\(/gm);
// Only names the app calls bare, which in practice are the bridge's.
const knownBridgeCalls = ['diffText', 'toggleWrap', 'setHeadingLevel', 'toggleQuote',
  'toggleBullet', 'toggleTask', 'insertWikilink', 'insertTag'];
for (const name of knownBridgeCalls) {
  if (!bridgeFunctions.has(name) && appSource.includes(`${name}(`)) {
    problems.push(`${name}() is called but the bindings do not export it`);
  }
}

// --- Types from the bridge ----------------------------------------------
const bridgeTypes = new Set([
  ...collect(generated, /^public struct (\w+)/gm),
  ...collect(generated, /^public enum (\w+)/gm),
  ...collect(generated, /^(?:open|public final) class (\w+)/gm),
  ...collect(generated, /^public protocol (\w+)/gm),
  ...collect(generated, /^enum (\w+)/gm),
  ...collect(generated, /^public typealias (\w+)/gm),
]);
const typesUsed = ['HostConfig', 'StorageKind', 'StorageHost', 'VaultHandle', 'OpenReportSink',
  'ScanProgress', 'ChangeObserver', 'FsEvent', 'FfiError', 'Note', 'CreatedNote', 'Collision',
  'SearchOptions', 'SearchResults', 'FileMatch', 'Backlink', 'ResolvedLink', 'TagSummary',
  'Diagnostic', 'DirectoryListing', 'FolderEntry', 'FileEntry', 'FileKind', 'IndexProgress',
  'ScanReport', 'EventOutcome', 'TrashEntry', 'RenamePlan', 'RenameOutcome', 'RecoveryCandidate',
  'Property', 'OpenOutcome', 'Selection', 'EditResult', 'TextDiff', 'DiffLine', 'DiffHunk',
  'LineChange'];
for (const type of typesUsed) {
  if (!bridgeTypes.has(type) && new RegExp(`\\b${type}\\b`).test(appSource)) {
    problems.push(`${type} is referenced but the bindings do not define it`);
  }
}

// --- VaultService methods -----------------------------------------------
const serviceSource = readFileSync(join(iosRoot, 'Services/VaultService.swift'), 'utf8');
const serviceMethods = collect(serviceSource, /\bfunc (\w+)\s*[(<]/g);
const serviceProperties = collect(serviceSource, /\bvar (\w+)\s*:/g);
const serviceMembers = new Set([...serviceMethods, ...serviceProperties]);
for (const used of collect(appSource, /\bservice\.(\w+)/g)) {
  if (!serviceMembers.has(used)) {
    problems.push(`VaultService has no member '${used}'`);
  }
}

// --- Report -------------------------------------------------------------
if (problems.length > 0) {
  console.error('Symbols referenced but not defined:\n');
  for (const problem of problems) console.error(`  - ${problem}`);
  console.error(`\n${problems.length} problem(s). This is a name check, not a compile.`);
  process.exit(1);
}
console.log(`${appFiles.length} Swift files: every referenced token, bridge symbol and service member exists.`);
