#!/usr/bin/env node
//
// Are the generated files current?
//
// Two things in ios/ are generated and committed: the Swift bindings, from the
// Rust bridge, and the design tokens, from the desktop's theme. Committing
// them is deliberate — it is what makes a change to the Rust API visible in a
// diff without anyone needing a Mac — but committed generated files go stale,
// and a stale binding is a build that fails on someone else's machine.
//
// It generates into a temporary directory and compares, rather than
// regenerating in place and asking git. Asking git cannot work before a
// commit: the files you just regenerated are exactly the uncommitted changes
// it would complain about, so the check could only ever pass after the commit
// it was supposed to gate.
//
// It exists because it did not, and a commit went out with 442 lines of
// missing bindings having passed every local check.
//
//   node ios/Scripts/check-generated.mjs

import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, '../..');
const scratch = mkdtempSync(join(tmpdir(), 'ie-generated-'));

const run = (command, args, env) =>
  execFileSync(command, args, {
    cwd: repoRoot,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, ...env },
  });

const read = (path) => {
  try {
    return readFileSync(path, 'utf8');
  } catch {
    return null;
  }
};

const problems = [];

try {
  // --- Swift bindings ---------------------------------------------------
  const bindingsOut = join(scratch, 'Generated');
  run('./ios/Scripts/build-core.sh', ['--bindings'], { IE_BINDINGS_OUT: bindingsOut });
  for (const name of readdirSync(bindingsOut)) {
    const fresh = read(join(bindingsOut, name));
    const committed = read(join(repoRoot, 'ios/Generated', name));
    if (committed === null) {
      problems.push(`ios/Generated/${name} is missing`);
    } else if (fresh !== committed) {
      problems.push(`ios/Generated/${name} is stale`);
    }
  }

  // --- Design tokens ----------------------------------------------------
  const tokensOut = join(scratch, 'DesignTokens.swift');
  run('node', ['ios/Scripts/generate-tokens.mjs', tokensOut]);
  if (read(tokensOut) !== read(join(repoRoot, 'ios/Services/DesignTokens.swift'))) {
    problems.push('ios/Services/DesignTokens.swift is stale');
  }
} catch (error) {
  console.error(`Could not regenerate:\n${error.stderr || error.message}`);
  process.exit(1);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}

if (problems.length > 0) {
  console.error('Generated files do not match their sources:\n');
  for (const problem of problems) console.error(`  - ${problem}`);
  console.error('\nRun: pnpm ios:bindings && pnpm ios:tokens — then commit the result.');
  process.exit(1);
}
console.log('Generated bindings and design tokens are current.');
