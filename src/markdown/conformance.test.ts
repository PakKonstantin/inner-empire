/**
 * The TypeScript half of the Markdown conformance corpus.
 *
 * The Rust half is `crates/ie-core/tests/conformance.rs`, reading the same
 * fixtures and asserting the same expectations. Two implementations of one
 * grammar drift; this is what stops them.
 */

import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { extract } from './extract';

const CORPUS = join(process.cwd(), 'tests', 'fixtures', 'markdown');

interface Expected {
  links: {
    kind: string;
    target: string;
    heading: string | null;
    blockId: string | null;
    alias: string | null;
  }[];
  tags: string[];
  headings: { level: number; text: string; slug: string }[];
  blocks: string[];
}

const fixtures = readdirSync(CORPUS)
  .filter((name) => name.endsWith('.md') && name !== 'README.md')
  .sort();

describe('markdown conformance corpus', () => {
  it('has fixtures to run', () => {
    // A corpus that quietly becomes empty would make every test below pass
    // while checking nothing.
    expect(fixtures.length).toBeGreaterThan(5);
  });

  for (const fixture of fixtures) {
    const name = fixture.replace(/\.md$/, '');

    it(`agrees on ${name}`, () => {
      const source = readFileSync(join(CORPUS, fixture), 'utf8');
      const expected = JSON.parse(
        readFileSync(join(CORPUS, `${name}.expected.json`), 'utf8'),
      ) as Expected;

      const actual = extract(source);

      expect(actual.links).toEqual(expected.links);
      expect(actual.tags).toEqual(expected.tags);
      expect(actual.headings).toEqual(expected.headings);
      expect(actual.blocks).toEqual(expected.blocks);
    });
  }
});
