#!/usr/bin/env node
//
// Do the App Group identifiers agree?
//
// The app and the share extension must name the same group, or the extension
// cannot read the vault bookmark and every share fails with "choose your
// vault first" — a symptom that points nowhere near the cause. Neither a
// compiler nor a symbol check can see this: the files are plists and a
// build-setting template, and each is individually valid.
//
// They drifted the moment the second one was written, which is why this exists.
//
//   node ios/Scripts/check-app-group.mjs

import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const iosRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const groupsIn = (file) => {
  const text = readFileSync(file, 'utf8');
  const block = text.match(
    /<key>com\.apple\.security\.application-groups<\/key>\s*<array>([\s\S]*?)<\/array>/
  );
  if (!block) return [];
  return [...block[1].matchAll(/<string>([^<]+)<\/string>/g)].map((m) => m[1]);
};

// Every entitlements file, not a hardcoded pair. A third target was added and
// the two-target version would have gone on passing while the new one drifted,
// which is the exact failure this script exists to prevent.
const resources = join(iosRoot, 'Resources');
const entitlements = readdirSync(resources).filter((f) => f.endsWith('.entitlements'));
const problems = [];

if (entitlements.length < 2) {
  problems.push(`only ${entitlements.length} entitlements file(s) found — expected the app and its extensions`);
}

const groups = new Map();
for (const file of entitlements) {
  const found = groupsIn(join(resources, file));
  if (found.length === 0) {
    problems.push(`${file} declares no App Group`);
  }
  groups.set(file, found);
}

// Every target must share at least one group with every other.
const shared = [...groups.values()].reduce(
  (common, mine) => common.filter((group) => mine.includes(group)),
  [...(groups.values().next().value ?? [])]
);
if (groups.size > 1 && shared.length === 0) {
  problems.push(
    'no App Group common to every target:\n' +
      [...groups].map(([file, list]) => `      ${file}: ${list.join(', ') || '(none)'}`).join('\n')
  );
}

// And the Swift must not hardcode one, which is how the drift started.
const swift = readFileSync(join(iosRoot, 'Services/AppGroup.swift'), 'utf8');
if (/static let identifier\s*=\s*"group\./.test(swift)) {
  problems.push(
    'AppGroup.identifier is a literal — derive it from the bundle id so there is one source of truth'
  );
}

if (problems.length > 0) {
  console.error('App Group configuration is inconsistent:\n');
  for (const problem of problems) console.error(`  - ${problem}`);
  process.exit(1);
}
console.log(
  `${entitlements.length} targets share the App Group ${shared.join(', ')}.`
);
