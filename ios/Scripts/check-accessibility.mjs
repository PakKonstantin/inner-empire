#!/usr/bin/env node
//
// The parts of §53 a machine can check.
//
// Most of accessibility needs a device and a person: whether VoiceOver's
// reading order makes sense, whether the layout survives the largest Dynamic
// Type setting, whether a gesture has a reachable alternative. None of that is
// here, and TESTING.md says so.
//
// Two things are mechanical, though, and both are the kind of omission that
// ships because nothing complains:
//
//   1. An icon-only button with no accessibility label. VoiceOver reads it as
//      "button" — the user is told something is there and not what it does.
//   2. A hardcoded font size in a `.font(.system(size:))`, which ignores the
//      user's text size entirely.
//
// Running clean means those two are absent, not that the app is accessible.
//
//   node ios/Scripts/check-accessibility.mjs

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const iosRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function swiftFiles(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    if (entry === 'Generated') continue;
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) out.push(...swiftFiles(path));
    else if (entry.endsWith('.swift')) out.push(path);
  }
  return out;
}

const problems = [];

for (const file of swiftFiles(iosRoot)) {
  const text = readFileSync(file, 'utf8');
  const name = relative(iosRoot, file);
  const lineOf = (index) => text.slice(0, index).split('\n').length;

  // An icon-only button. The label body is captured along with the few lines
  // after the closing brace, because that is where a modifier would sit.
  const iconButton =
    /Button\s*\{[\s\S]*?\}\s*label:\s*\{\s*(Image\([^)]*\)[^}]*)\}([^\n]*(?:\n[^\n]*){0,4})/g;
  for (const match of text.matchAll(iconButton)) {
    const [, labelBody, following] = match;
    // A Text or Label inside gives it a name already.
    if (/Text\(|Label\(/.test(labelBody)) continue;
    if (/accessibilityLabel/.test(labelBody + following)) continue;
    problems.push(
      `${name}:${lineOf(match.index)} icon-only button with no accessibilityLabel — VoiceOver reads it as "button"`
    );
  }

  // A fixed point size ignores Dynamic Type. `.monospacedSystemFont` inside a
  // `UIFontMetrics` scaling call is fine and is spelled differently.
  for (const match of text.matchAll(/\.font\(\.system\(size:\s*\d+/g)) {
    problems.push(
      `${name}:${lineOf(match.index)} hardcoded font size — use a text style so Dynamic Type applies`
    );
  }
}

if (problems.length > 0) {
  console.error('Accessibility problems a machine can see:\n');
  for (const problem of problems) console.error(`  - ${problem}`);
  console.error(
    '\nThis is the mechanical subset of §53. The rest needs a device — see docs/ios/TESTING.md.'
  );
  process.exit(1);
}
console.log(
  'No unlabelled icon buttons and no hardcoded font sizes. (The rest of §53 needs a device.)'
);
