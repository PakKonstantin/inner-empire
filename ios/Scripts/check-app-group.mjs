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

import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const iosRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const groupsIn = (file) => {
  const text = readFileSync(join(iosRoot, file), 'utf8');
  const block = text.match(
    /<key>com\.apple\.security\.application-groups<\/key>\s*<array>([\s\S]*?)<\/array>/
  );
  if (!block) return [];
  return [...block[1].matchAll(/<string>([^<]+)<\/string>/g)].map((m) => m[1]);
};

const app = groupsIn('Resources/InnerEmpire.entitlements');
const extension_ = groupsIn('Resources/ShareExtension.entitlements');
const problems = [];

if (app.length === 0) problems.push('the app declares no App Group');
if (extension_.length === 0) problems.push('the share extension declares no App Group');

const shared = app.filter((group) => extension_.includes(group));
if (app.length > 0 && extension_.length > 0 && shared.length === 0) {
  problems.push(
    `no group in common: app has ${app.join(', ')}, extension has ${extension_.join(', ')}`
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
console.log(`App and extension share the App Group ${shared.join(', ')}.`);
