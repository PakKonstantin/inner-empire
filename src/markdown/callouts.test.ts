/**
 * Reading a blockquote as a callout.
 *
 * The marker is only the first line of an ordinary blockquote, which is what
 * keeps a vault portable: another editor sees a quote, and a callout type
 * nobody has heard of degrades into one here. What has to be right is the
 * boundary — what counts as a marker, and what is left over as the body once
 * the marker is taken away.
 */

import { describe, expect, it } from 'vitest';
import type { Blockquote, Root } from 'mdast';
import { fromMarkdown } from 'mdast-util-from-markdown';

import { readCallout } from './renderer';

function quote(source: string): Blockquote {
  const tree = fromMarkdown(source) as Root;
  const first = tree.children[0];
  if (!first || first.type !== 'blockquote') throw new Error('not a blockquote');
  return first;
}

function bodyText(node: Blockquote, source: string): string {
  const callout = readCallout(node);
  if (!callout) throw new Error(`not a callout: ${source}`);
  return JSON.stringify(callout.body);
}

describe('readCallout', () => {
  it('reads the kind from the marker', () => {
    const callout = readCallout(quote('> [!warning] Mind the gap\n> Stand clear.'));
    expect(callout?.kind).toBe('warning');
    expect(callout?.title).toBe('Mind the gap');
  });

  it('lowercases the kind so [!NOTE] and [!note] agree', () => {
    expect(readCallout(quote('> [!NOTE]\n> text'))?.kind).toBe('note');
  });

  it('has no title when the marker stands alone', () => {
    const callout = readCallout(quote('> [!info]\n> Just the body.'));
    expect(callout?.title).toBeNull();
  });

  it('leaves an ordinary blockquote alone', () => {
    expect(readCallout(quote('> Just a quotation.'))).toBeNull();
    expect(readCallout(quote('> Not a callout [!note] midway through.'))).toBeNull();
  });

  it('is not fooled by something that only looks like a marker', () => {
    expect(readCallout(quote('> [note] no bang'))).toBeNull();
    expect(readCallout(quote('> [!] no kind'))).toBeNull();
    expect(readCallout(quote('> [!9lives] a kind cannot start with a digit'))).toBeNull();
  });

  it('reads the fold marker', () => {
    const folded = readCallout(quote('> [!tip]- Hidden\n> text'));
    expect(folded?.collapsible).toBe(true);
    expect(folded?.folded).toBe(true);

    const open = readCallout(quote('> [!tip]+ Shown\n> text'));
    expect(open?.collapsible).toBe(true);
    expect(open?.folded).toBe(false);

    const plain = readCallout(quote('> [!tip] Always\n> text'));
    expect(plain?.collapsible).toBe(false);
    expect(plain?.folded).toBe(false);
  });

  it('keeps the body and drops only the marker line', () => {
    const source = '> [!note] Title\n> First line.\n> Second line.';
    const body = bodyText(quote(source), source);
    expect(body).toContain('First line.');
    expect(body).toContain('Second line.');
    // The marker itself must not survive into the rendered body.
    expect(body).not.toContain('[!note]');
    expect(body).not.toContain('Title');
  });

  it('keeps every block of a multi-paragraph callout', () => {
    const source = '> [!example] Two parts\n> One.\n>\n> Two.';
    const callout = readCallout(quote(source));
    expect(callout?.body).toHaveLength(2);
  });

  it('has an empty body when there is only a title', () => {
    const callout = readCallout(quote('> [!note] Nothing else'));
    expect(callout?.body).toEqual([]);
  });

  it('keeps inline markup that follows the marker line', () => {
    const source = '> [!note] Title\n> Some **bold** text.';
    const body = bodyText(quote(source), source);
    expect(body).toContain('strong');
    expect(body).toContain('bold');
  });
});
