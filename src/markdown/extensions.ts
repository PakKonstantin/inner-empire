/**
 * The app's Markdown extensions, for the frontend.
 *
 * Rust owns the authoritative extraction — what the index records, what a
 * rename rewrites. This module exists only to *display* the same constructs,
 * and the two are held together by a shared fixture corpus that both test
 * suites run (see `tests/fixtures/markdown`).
 *
 * The grammar implemented here is exactly the grammar in
 * `ie-core::markdown::scanner`, and any change to one must change the other.
 */

export interface WikiLinkToken {
  kind: 'wikiLink' | 'embed';
  raw: string;
  target: string;
  heading: string | null;
  blockId: string | null;
  alias: string | null;
  start: number;
  end: number;
}

export interface TagToken {
  name: string;
  start: number;
  end: number;
}

/** Parse the text between `[[` and `]]`. */
export function parseLinkTarget(inner: string): Omit<WikiLinkToken, 'kind' | 'raw' | 'start' | 'end'> {
  // The alias runs to the end, so the split is at the first pipe.
  const pipe = inner.indexOf('|');
  const locator = (pipe === -1 ? inner : inner.slice(0, pipe)).trim();
  const aliasText = pipe === -1 ? null : inner.slice(pipe + 1).trim();
  const alias = aliasText ? aliasText : null;

  const hash = locator.indexOf('#');
  if (hash === -1) {
    return { target: locator, heading: null, blockId: null, alias };
  }

  const target = locator.slice(0, hash).trim();
  const fragment = locator.slice(hash + 1);
  if (fragment.startsWith('^')) {
    const blockId = fragment.slice(1).trim();
    return { target, heading: null, blockId: blockId || null, alias };
  }
  const heading = fragment.trim();
  return { target, heading: heading || null, blockId: null, alias };
}

/** Render a link target back to the text between `[[` and `]]`. */
export function formatLinkTarget(token: {
  target: string;
  heading?: string | null;
  blockId?: string | null;
  alias?: string | null;
}): string {
  let out = token.target;
  if (token.blockId) out += `#^${token.blockId}`;
  else if (token.heading) out += `#${token.heading}`;
  if (token.alias) out += `|${token.alias}`;
  return out;
}

/** What a link shows when it has no alias. */
export function displayTextOf(token: WikiLinkToken): string {
  if (token.alias) return token.alias;
  if (!token.target) return token.heading ?? token.blockId ?? '';
  return token.target;
}

/**
 * Find wiki links and embeds in a string.
 *
 * `skip` marks regions the caller has already decided are not prose — code
 * spans, code blocks — so this never has to know about Markdown structure
 * itself.
 */
export function findWikiLinks(
  text: string,
  skip: (offset: number) => boolean = () => false,
): WikiLinkToken[] {
  const out: WikiLinkToken[] = [];
  let cursor = 0;

  while (cursor < text.length) {
    const open = text.indexOf('[[', cursor);
    if (open === -1) break;

    const isEmbed = open > 0 && text[open - 1] === '!';
    const start = isEmbed ? open - 1 : open;

    if (skip(start)) {
      cursor = open + 2;
      continue;
    }

    const close = text.indexOf(']]', open + 2);
    if (close === -1) break;

    const inner = text.slice(open + 2, close);
    // A wiki link never spans lines and never nests.
    if (inner.includes('\n') || inner.includes('[[')) {
      cursor = open + 2;
      continue;
    }

    const parsed = parseLinkTarget(inner);
    if (!parsed.target && !parsed.heading && !parsed.blockId) {
      cursor = close + 2;
      continue;
    }

    out.push({
      kind: isEmbed ? 'embed' : 'wikiLink',
      raw: text.slice(start, close + 2),
      start,
      end: close + 2,
      ...parsed,
    });
    cursor = close + 2;
  }

  return out;
}

const TAG_CHARACTER = /[\p{L}\p{N}_/-]/u;

/** A `#` only starts a tag when what precedes it is not word-like. */
function canStartTag(previous: string | undefined): boolean {
  if (previous === undefined) return true;
  return !/[\p{L}\p{N}_#/\\]/u.test(previous);
}

/** Find `#tags`, including nested ones. */
export function findTags(
  text: string,
  skip: (offset: number) => boolean = () => false,
): TagToken[] {
  const out: TagToken[] = [];

  for (let index = 0; index < text.length; index += 1) {
    if (text[index] !== '#') continue;
    if (skip(index) || !canStartTag(text[index - 1])) continue;

    let end = index + 1;
    while (end < text.length && TAG_CHARACTER.test(text[end]!)) end += 1;

    let name = text.slice(index + 1, end);
    // A trailing separator is punctuation, not part of the tag.
    name = name.replace(/[/-]+$/, '');

    // `#`, `#123` and `#---` are not tags: requiring a letter keeps issue
    // references and horizontal rules out of the taxonomy.
    if (!name || !/\p{L}/u.test(name) || name.includes('//')) continue;

    out.push({ name, start: index, end: index + 1 + name.length });
    index = end - 1;
  }

  return out;
}

const BLOCK_ID = /(?:^|\s)\^([A-Za-z0-9_-]+)\s*$/;

/** The block identifier at the end of a line, if there is one. */
export function findBlockId(line: string): { id: string; start: number } | null {
  const match = BLOCK_ID.exec(line.replace(/\s+$/, ''));
  if (!match || match.index === undefined) return null;
  const caret = line.lastIndexOf('^');
  return { id: match[1]!, start: caret };
}

/** Does this target name an image, so an embed becomes a picture? */
export function isImageTarget(target: string): boolean {
  return /\.(png|jpe?g|gif|webp|svg|bmp|avif|ico)$/i.test(target);
}

export function isPdfTarget(target: string): boolean {
  return /\.pdf$/i.test(target);
}

export function isAudioTarget(target: string): boolean {
  return /\.(mp3|wav|ogg|m4a|flac|aac|opus)$/i.test(target);
}

export function isVideoTarget(target: string): boolean {
  return /\.(mp4|webm|mkv|mov|avi|m4v)$/i.test(target);
}

/** Turn a heading into the anchor the backend would generate. */
export function slugify(heading: string): string {
  let out = '';
  let lastWasDash = false;
  for (const character of heading) {
    if (/[\p{L}\p{N}]/u.test(character)) {
      out += character.toLowerCase();
      lastWasDash = false;
    } else if (out && !lastWasDash) {
      out += '-';
      lastWasDash = true;
    }
  }
  return out.replace(/-+$/, '');
}
