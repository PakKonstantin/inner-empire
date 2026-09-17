import { describe, expect, it } from 'vitest';

import {
  displayTextOf,
  findBlockId,
  findTags,
  findWikiLinks,
  formatLinkTarget,
  parseLinkTarget,
  slugify,
} from './extensions';

describe('link targets', () => {
  it('parses every supported form', () => {
    expect(parseLinkTarget('Note')).toMatchObject({ target: 'Note', alias: null });
    expect(parseLinkTarget('Note|Alias')).toMatchObject({ target: 'Note', alias: 'Alias' });
    expect(parseLinkTarget('Note#Heading')).toMatchObject({
      target: 'Note',
      heading: 'Heading',
      blockId: null,
    });
    expect(parseLinkTarget('Note#^block')).toMatchObject({
      target: 'Note',
      blockId: 'block',
      heading: null,
    });
    expect(parseLinkTarget('Note#Heading|Alias')).toMatchObject({
      target: 'Note',
      heading: 'Heading',
      alias: 'Alias',
    });
  });

  it('treats an empty target as a reference within the same note', () => {
    expect(parseLinkTarget('#Heading')).toMatchObject({ target: '', heading: 'Heading' });
  });

  it('keeps inner hashes in a nested heading path', () => {
    expect(parseLinkTarget('Note#Chapter#Section').heading).toBe('Chapter#Section');
  });

  it('lets an alias contain a pipe', () => {
    expect(parseLinkTarget('Note|a|b').alias).toBe('a|b');
  });

  it('round trips through formatting', () => {
    for (const inner of [
      'Note',
      'Note|Alias',
      'Note#Heading',
      'Note#Heading|Alias',
      'Note#^block',
      'Folder/Sub/Note#Heading|Alias',
    ]) {
      expect(formatLinkTarget(parseLinkTarget(inner))).toBe(inner);
    }
  });
});

describe('finding wiki links', () => {
  it('finds links and embeds with their positions', () => {
    const text = 'See [[A]] and ![[B]].';
    const tokens = findWikiLinks(text);

    expect(tokens).toHaveLength(2);
    expect(tokens[0]).toMatchObject({ kind: 'wikiLink', target: 'A' });
    expect(text.slice(tokens[0]!.start, tokens[0]!.end)).toBe('[[A]]');
    expect(tokens[1]).toMatchObject({ kind: 'embed', target: 'B' });
    expect(text.slice(tokens[1]!.start, tokens[1]!.end)).toBe('![[B]]');
  });

  it('skips regions the caller marks as code', () => {
    const text = 'real [[Yes]] code [[No]]';
    const codeStart = text.indexOf('[[No]]');
    const tokens = findWikiLinks(text, (offset) => offset >= codeStart);
    expect(tokens.map((t) => t.target)).toEqual(['Yes']);
  });

  it('ignores an unterminated link', () => {
    expect(findWikiLinks('[[never closed')).toHaveLength(0);
  });

  it('will not let a link span lines', () => {
    expect(findWikiLinks('[[Note\nstill]]')).toHaveLength(0);
  });

  it('ignores empty brackets', () => {
    expect(findWikiLinks('[[]] and [[|alias]]')).toHaveLength(0);
  });

  it('shows the alias when there is one', () => {
    const [plain] = findWikiLinks('[[Target]]');
    const [aliased] = findWikiLinks('[[Target|Shown]]');
    expect(displayTextOf(plain!)).toBe('Target');
    expect(displayTextOf(aliased!)).toBe('Shown');
  });
});

describe('finding tags', () => {
  it('finds flat and nested tags', () => {
    expect(findTags('#AI and #GameDesign/Mechanics').map((t) => t.name)).toEqual([
      'AI',
      'GameDesign/Mechanics',
    ]);
  });

  it('will not turn a hash inside a word into a tag', () => {
    expect(findTags('C# and issue#42 and a#b')).toHaveLength(0);
  });

  it('will not turn a heading into a tag', () => {
    expect(findTags('# Heading\n## Sub')).toHaveLength(0);
  });

  it('will not turn a numeric reference into a tag', () => {
    expect(findTags('see #42 and #2026')).toHaveLength(0);
  });

  it('accepts a tag after punctuation', () => {
    expect(findTags('(#AI) [#Research]').map((t) => t.name)).toEqual(['AI', 'Research']);
  });

  it('leaves trailing punctuation out of the tag', () => {
    expect(findTags('tagged #AI, and #Research.').map((t) => t.name)).toEqual(['AI', 'Research']);
    expect(findTags('#AI/').map((t) => t.name)).toEqual(['AI']);
  });

  it('reports positions that cover the hash and the name', () => {
    const text = 'text #AI more';
    const [tag] = findTags(text);
    expect(text.slice(tag!.start, tag!.end)).toBe('#AI');
  });

  it('accepts non-ASCII tag names', () => {
    expect(findTags('#日本語 and #Idée').map((t) => t.name)).toEqual(['日本語', 'Idée']);
  });
});

describe('block identifiers', () => {
  it('finds a trailing marker', () => {
    expect(findBlockId('The important point. ^key-point')).toMatchObject({ id: 'key-point' });
  });

  it('finds a marker alone on a line', () => {
    expect(findBlockId('^standalone')).toMatchObject({ id: 'standalone' });
  });

  it('ignores a caret inside a word', () => {
    expect(findBlockId('the expression a^b is maths')).toBeNull();
  });

  it('rejects characters that would break a link', () => {
    expect(findBlockId('text ^has spaces')).toBeNull();
    expect(findBlockId('text ^has/slash')).toBeNull();
  });
});

describe('slugs', () => {
  it('matches what the backend generates', () => {
    expect(slugify('Getting Started')).toBe('getting-started');
    expect(slugify("What's *this*?")).toBe('what-s-this');
    expect(slugify('  Trailing  ')).toBe('trailing');
    expect(slugify('Section 2.1')).toBe('section-2-1');
  });
});
