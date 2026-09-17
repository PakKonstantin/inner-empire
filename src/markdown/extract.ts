/**
 * Extracting the app's constructs from a note, the same way the backend does.
 *
 * The backend's extraction is authoritative — it is what the index, the
 * backlinks and a rename act on. This exists so the frontend can answer the
 * same question without a round trip, and so the two can be held to the same
 * answer by a shared fixture corpus that both test suites run.
 *
 * The two-stage shape is deliberate and matches `ie-core::markdown`: a real
 * CommonMark parser decides what is prose, and only then does the extension
 * scanner look at it.
 */

import type { Nodes as MdastNode, Parent } from 'mdast';

import { findBlockId, findTags, findWikiLinks, slugify } from './extensions';
import { parseMarkdown } from './renderer';

export interface ExtractedLink {
  kind: 'wikiLink' | 'embed' | 'markdown' | 'markdownImage' | 'external';
  target: string;
  heading: string | null;
  blockId: string | null;
  alias: string | null;
}

export interface ExtractedHeading {
  level: number;
  text: string;
  slug: string;
}

export interface Extraction {
  links: ExtractedLink[];
  tags: string[];
  headings: ExtractedHeading[];
  blocks: string[];
  /** Byte offset where the body begins, after any frontmatter. */
  bodyOffset: number;
}

/** Where the frontmatter block ends, or 0 when there is none. */
export function bodyOffset(source: string): number {
  if (!source.startsWith('---')) return 0;
  const afterMarker = source.slice(3);
  const firstBreak = afterMarker.startsWith('\r\n') ? 2 : afterMarker.startsWith('\n') ? 1 : -1;
  if (firstBreak === -1) return 0;

  let cursor = 3 + firstBreak;
  while (cursor <= source.length) {
    const lineEnd = source.indexOf('\n', cursor);
    const end = lineEnd === -1 ? source.length : lineEnd;
    const line = source.slice(cursor, end).replace(/\r$/, '');
    if (line === '---' || line === '...') {
      return lineEnd === -1 ? source.length : lineEnd + 1;
    }
    if (lineEnd === -1) break;
    cursor = lineEnd + 1;
  }
  // An unterminated block is not frontmatter; the whole file is body.
  return 0;
}

/** Everything the app recognises in a note. */
export function extract(source: string): Extraction {
  const offset = bodyOffset(source);
  const body = source.slice(offset);
  const tree = parseMarkdown(body);

  const links: ExtractedLink[] = [];
  const tags: string[] = [];
  const headings: ExtractedHeading[] = [];

  // Text nodes are what the CommonMark parser left over as prose: not code,
  // not HTML, not a link destination. Scanning only these is what keeps a
  // shebang out of the tag list.
  const walk = (node: MdastNode): void => {
    switch (node.type) {
      case 'text': {
        const found = findWikiLinks(node.value);
        for (const token of found) {
          links.push({
            kind: token.kind,
            target: token.target,
            heading: token.heading,
            blockId: token.blockId,
            alias: token.alias,
          });
        }
        for (const tag of findTags(node.value, (position) =>
          found.some((token) => position >= token.start && position < token.end),
        )) {
          tags.push(tag.name);
        }
        break;
      }

      case 'heading': {
        const text = plainTextOf(node).trim();
        headings.push({ level: node.depth, text, slug: slugify(text) });
        break;
      }

      case 'link':
      case 'image': {
        const external = /^[a-z][a-z0-9+.-]*:/i.test(node.url) || node.url.startsWith('//');
        const decoded = safeDecode(node.url);
        const [target, heading, blockId] = external
          ? [node.url, null, null]
          : splitDestination(decoded);
        links.push({
          kind: external ? 'external' : node.type === 'image' ? 'markdownImage' : 'markdown',
          target,
          heading,
          blockId,
          alias: null,
        });
        break;
      }

      // Code and HTML are stepped over entirely, which is the whole point of
      // going through the parser first.
      case 'code':
      case 'inlineCode':
      case 'html':
        return;

      default:
        break;
    }

    if ('children' in node) {
      for (const child of (node as Parent).children) walk(child as MdastNode);
    }
  };

  walk(tree);

  // Block identifiers are a line-level construct, so they are found by walking
  // lines rather than the tree, skipping any line inside a fenced block.
  const blocks: string[] = [];
  let inFence = false;
  for (const line of body.split('\n')) {
    if (/^\s*(```|~~~)/.test(line)) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;
    const found = findBlockId(line);
    if (found) blocks.push(found.id);
  }

  return { links, tags, headings, blocks, bodyOffset: offset };
}

function splitDestination(destination: string): [string, string | null, string | null] {
  const hash = destination.indexOf('#');
  if (hash === -1) return [destination, null, null];

  const target = destination.slice(0, hash);
  const fragment = destination.slice(hash + 1);
  if (fragment.startsWith('^')) {
    return [target, null, fragment.slice(1) || null];
  }
  return [target, fragment || null, null];
}

function safeDecode(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    // A destination with a stray percent sign is left as written rather than
    // being dropped.
    return value;
  }
}

function plainTextOf(node: MdastNode): string {
  if ('value' in node && typeof node.value === 'string') return node.value;
  if ('children' in node) {
    return (node as Parent).children.map((child) => plainTextOf(child as MdastNode)).join('');
  }
  return '';
}
