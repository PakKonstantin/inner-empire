/**
 * Rendering Markdown to React elements.
 *
 * `mdast-util-from-markdown` gives a proper syntax tree, which the walk below
 * turns into elements. Going through an AST rather than producing an HTML
 * string means links, embeds and tags become real components with click
 * handlers, and nothing has to set `dangerouslySetInnerHTML` on note content.
 */

import type {
  Blockquote,
  Image,
  Nodes as MdastNode,
  Paragraph,
  Parent,
  PhrasingContent,
  RootContent,
} from 'mdast';
import { fromMarkdown } from 'mdast-util-from-markdown';
import { gfmFromMarkdown } from 'mdast-util-gfm';
import { gfm } from 'micromark-extension-gfm';
import type { JSX, ReactNode } from 'react';
import { Fragment } from 'react';

import type { VaultPath } from '@/types/domain';
import { Icon, type IconName } from '@/ui/icons';

import {
  displayTextOf,
  findTags,
  findWikiLinks,
  isAudioTarget,
  isImageTarget,
  isPdfTarget,
  isVideoTarget,
  slugify,
  type WikiLinkToken,
} from './extensions';

export interface RenderContext {
  /** The note being rendered, so same-note links resolve. */
  path: VaultPath;
  /** Follow a wiki link or Markdown link. */
  onFollowLink: (target: string, options: { newPane?: boolean }) => void;
  /** Search for a tag. */
  onFollowTag: (tag: string) => void;
  /** Open a URL outside the app. */
  onFollowExternal: (url: string) => void;
  /** Turn a vault path into something an `<img>` can load. */
  resolveAsset: (target: string) => string | null;
  /** Render an embedded note, or `null` if it cannot be expanded here. */
  renderEmbed?: (token: WikiLinkToken) => ReactNode;
  /** Toggle a checkbox in the source. */
  onToggleTask?: (line: number, checked: boolean) => void;
}

/** How deep an embed chain may go before it is treated as circular. */
export const MAX_EMBED_DEPTH = 6;

export function parseMarkdown(source: string): MdastNode {
  return fromMarkdown(source, {
    extensions: [gfm()],
    mdastExtensions: [gfmFromMarkdown()],
  });
}

/** Render a note body. Frontmatter is the properties panel's job, not this. */
export function renderMarkdown(source: string, context: RenderContext): ReactNode {
  const tree = parseMarkdown(source);
  if (!('children' in tree)) return null;
  return renderChildren(tree as Parent, context);
}

function renderChildren(node: Parent, context: RenderContext): ReactNode {
  return node.children.map((child, index) => (
    <Fragment key={index}>{renderNode(child as RootContent, context)}</Fragment>
  ));
}

function renderNode(node: RootContent, context: RenderContext): ReactNode {
  switch (node.type) {
    case 'paragraph': {
      const image = soleImageOf(node);
      if (image) {
        const source = context.resolveAsset(decodeURI(image.url));
        if (source) {
          return (
            <figure className="ie-figure">
              <img src={source} alt={image.alt ?? ''} loading="lazy" />
              <figcaption>{image.alt}</figcaption>
            </figure>
          );
        }
      }
      return <p>{renderChildren(node, context)}</p>;
    }

    case 'heading': {
      const Tag = `h${node.depth}` as keyof JSX.IntrinsicElements;
      const text = plainText(node);
      return <Tag id={slugify(text)}>{renderChildren(node, context)}</Tag>;
    }

    case 'text':
      return renderInlineText(node.value, node.position?.start.line, context);

    case 'strong':
      return <strong>{renderChildren(node, context)}</strong>;

    case 'emphasis':
      return <em>{renderChildren(node, context)}</em>;

    case 'delete':
      return <del>{renderChildren(node, context)}</del>;

    case 'inlineCode':
      return <code className="ie-inline-code">{node.value}</code>;

    case 'code':
      return (
        <pre className="ie-code-block" data-language={node.lang ?? undefined}>
          <code>{node.value}</code>
        </pre>
      );

    case 'blockquote': {
      const callout = readCallout(node);
      if (callout) return <CalloutBlock callout={callout} context={context} />;
      return <blockquote>{renderChildren(node, context)}</blockquote>;
    }

    case 'list':
      return node.ordered ? (
        <ol start={node.start ?? 1}>{renderChildren(node, context)}</ol>
      ) : (
        <ul>{renderChildren(node, context)}</ul>
      );

    case 'listItem': {
      const line = node.position?.start.line;
      if (typeof node.checked === 'boolean') {
        return (
          <li className="ie-task-item">
            <input
              type="checkbox"
              checked={node.checked}
              onChange={(event) => {
                if (line !== undefined) {
                  context.onToggleTask?.(line - 1, event.currentTarget.checked);
                }
              }}
              disabled={!context.onToggleTask}
              aria-label={plainText(node) || 'Task'}
            />
            <span>{renderChildren(node, context)}</span>
          </li>
        );
      }
      return <li>{renderChildren(node, context)}</li>;
    }

    case 'thematicBreak':
      return <hr />;

    case 'break':
      return <br />;

    case 'link': {
      const href = node.url;
      const external = /^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith('//');
      return (
        <a
          href={href}
          className={external ? 'ie-external-link' : 'ie-internal-link'}
          onClick={(event) => {
            event.preventDefault();
            if (external) context.onFollowExternal(href);
            else context.onFollowLink(decodeURI(href), { newPane: event.ctrlKey || event.metaKey });
          }}
        >
          {renderChildren(node, context)}
        </a>
      );
    }

    case 'image': {
      const source = context.resolveAsset(decodeURI(node.url));
      if (!source) {
        return <span className="ie-missing-asset">{node.alt || node.url}</span>;
      }
      return <img src={source} alt={node.alt ?? ''} loading="lazy" />;
    }

    case 'table':
      return (
        <div className="ie-table-wrapper">
          <table>
            <tbody>{renderChildren(node, context)}</tbody>
          </table>
        </div>
      );

    case 'tableRow':
      return <tr>{renderChildren(node, context)}</tr>;

    case 'tableCell':
      return <td>{renderChildren(node, context)}</td>;

    case 'footnoteReference':
      return (
        <sup className="ie-footnote-ref">
          <a href={`#fn-${node.identifier}`}>{node.label ?? node.identifier}</a>
        </sup>
      );

    case 'footnoteDefinition':
      return (
        <div className="ie-footnote" id={`fn-${node.identifier}`}>
          <span className="ie-footnote-label">{node.label ?? node.identifier}</span>
          {renderChildren(node, context)}
        </div>
      );

    // Raw HTML in a note is shown as text rather than injected. A note is
    // untrusted input — it may have been synced from anywhere — and rendering
    // it would hand any writer a script tag in the app's own origin.
    case 'html':
      return <code className="ie-raw-html">{node.value}</code>;

    case 'yaml':
      return null;

    default:
      return 'children' in node ? renderChildren(node as Parent, context) : null;
  }
}

/**
 * Render a run of plain text, pulling out the app's own extensions.
 *
 * The CommonMark parser has already decided this is prose rather than code, so
 * anything found here is genuinely a link or a tag.
 */
function renderInlineText(
  text: string,
  line: number | undefined,
  context: RenderContext,
): ReactNode {
  const links = findWikiLinks(text);
  const tags = findTags(text, (offset) =>
    links.some((link) => offset >= link.start && offset < link.end),
  );

  type Span = { start: number; end: number; node: ReactNode };
  const spans: Span[] = [
    ...links.map((link) => ({
      start: link.start,
      end: link.end,
      node: renderWikiLink(link, context),
    })),
    ...tags.map((tag) => ({
      start: tag.start,
      end: tag.end,
      node: (
        <button
          type="button"
          className="ie-tag"
          onClick={() => context.onFollowTag(tag.name)}
          title={`Search for #${tag.name}`}
        >
          #{tag.name}
        </button>
      ),
    })),
  ].sort((a, b) => a.start - b.start);

  if (spans.length === 0) return text;

  const out: ReactNode[] = [];
  let cursor = 0;
  spans.forEach((span, index) => {
    if (span.start < cursor) return;
    if (span.start > cursor) out.push(text.slice(cursor, span.start));
    out.push(<Fragment key={`${line ?? 0}-${index}`}>{span.node}</Fragment>);
    cursor = span.end;
  });
  if (cursor < text.length) out.push(text.slice(cursor));

  return <>{out}</>;
}

function renderWikiLink(token: WikiLinkToken, context: RenderContext): ReactNode {
  const inner = token.target || context.path;

  if (token.kind === 'embed') {
    if (isImageTarget(token.target)) {
      const source = context.resolveAsset(token.target);
      // `![[image.png|400]]` sizes the image; the part after the pipe is a
      // width in pixels for an attachment rather than an alias.
      const width = token.alias && /^\d+$/.test(token.alias) ? Number(token.alias) : undefined;
      return source ? (
        <img src={source} alt={token.target} width={width} loading="lazy" />
      ) : (
        <span className="ie-missing-asset">{token.target}</span>
      );
    }
    if (isPdfTarget(token.target)) {
      const source = context.resolveAsset(token.target);
      return source ? (
        <object data={source} type="application/pdf" className="ie-embedded-pdf">
          <a href={source}>{token.target}</a>
        </object>
      ) : (
        <span className="ie-missing-asset">{token.target}</span>
      );
    }
    if (isAudioTarget(token.target)) {
      const source = context.resolveAsset(token.target);
      return source ? <audio controls src={source} /> : null;
    }
    if (isVideoTarget(token.target)) {
      const source = context.resolveAsset(token.target);
      return source ? <video controls src={source} className="ie-embedded-video" /> : null;
    }
    return context.renderEmbed?.(token) ?? (
      <span className="ie-embed-placeholder">{displayTextOf(token)}</span>
    );
  }

  return (
    <button
      type="button"
      className="ie-wikilink"
      data-target={token.target}
      onClick={(event) => context.onFollowLink(inner, { newPane: event.ctrlKey || event.metaKey })}
      title={token.target ? `Open ${token.target}` : 'Go to section'}
    >
      {displayTextOf(token)}
    </button>
  );
}

/** The text content of a node, ignoring formatting. */
export function plainText(node: MdastNode | PhrasingContent): string {
  if ('value' in node && typeof node.value === 'string') return node.value;
  if ('children' in node) {
    return (node.children as PhrasingContent[]).map(plainText).join('');
  }
  return '';
}

/** A short excerpt of a note, for a hover preview or a search result. */
export function excerpt(source: string, maxLength = 200): string {
  const tree = parseMarkdown(source);
  const text = 'children' in tree ? plainText(tree as MdastNode) : '';
  const collapsed = text.replace(/\s+/g, ' ').trim();
  return collapsed.length > maxLength ? `${collapsed.slice(0, maxLength - 1)}…` : collapsed;
}

/**
 * Callouts.
 *
 * `> [!warning] Mind the gap` is an ordinary blockquote as far as the parser
 * is concerned — the marker is just the first line of its first paragraph.
 * That is deliberate: recognising it here means the Markdown engine keeps one
 * definition of what a blockquote is, a vault stays readable in any other
 * editor, and a callout type nobody has heard of degrades into a quote rather
 * than an error.
 */
export interface Callout {
  kind: string;
  title: string | null;
  /** `> [!note]-` starts folded; `> [!note]+` starts open. */
  folded: boolean;
  collapsible: boolean;
  /** The blockquote with the marker line removed. */
  body: RootContent[];
}

const CALLOUT_MARKER = /^\[!([A-Za-z][\w-]*)\]([-+])?[ \t]*(.*)$/;

/** The callout kinds that get their own colour and icon; others fall back. */
const CALLOUT_KINDS: Record<string, IconName> = {
  note: 'file-text',
  info: 'info',
  tip: 'tip',
  success: 'check',
  question: 'help',
  warning: 'warning',
  danger: 'warning',
  error: 'error',
  bug: 'bug',
  example: 'list',
  quote: 'quote',
  abstract: 'list',
  todo: 'check',
};

/** Read a blockquote as a callout, or `null` when it is only a quote. */
export function readCallout(node: Blockquote): Callout | null {
  const first = node.children[0];
  if (!first || first.type !== 'paragraph') return null;

  const opener = first.children[0];
  if (!opener || opener.type !== 'text') return null;

  const [line, ...restOfLine] = opener.value.split('\n');
  const match = line ? CALLOUT_MARKER.exec(line.trim()) : null;
  if (!match) return null;

  const [, kind = '', fold, inlineTitle = ''] = match;

  // What is left of the first paragraph once the marker line is gone. An
  // empty remainder means the callout has a title and no body on that line.
  const remainder = restOfLine.join('\n');
  const rest: PhrasingContent[] = [
    ...(remainder ? [{ type: 'text' as const, value: remainder }] : []),
    ...first.children.slice(1),
  ];

  const body: RootContent[] = [
    ...(rest.length > 0 ? [{ type: 'paragraph' as const, children: rest }] : []),
    ...node.children.slice(1),
  ];

  return {
    kind: kind.toLowerCase(),
    title: inlineTitle.trim() || null,
    folded: fold === '-',
    collapsible: fold === '-' || fold === '+',
    body,
  };
}

function CalloutBlock({ callout, context }: { callout: Callout; context: RenderContext }) {
  const icon = CALLOUT_KINDS[callout.kind] ?? 'info';
  const known = callout.kind in CALLOUT_KINDS;
  const title = callout.title ?? capitalise(callout.kind);
  const body = (
    <div className="ie-callout__body">
      {callout.body.map((child, index) => (
        <Fragment key={index}>{renderNode(child, context)}</Fragment>
      ))}
    </div>
  );

  // An unknown kind still gets the callout's shape; only its colour falls
  // back, so a vault written against another editor's list still reads well.
  const className = `ie-callout ie-callout--${known ? callout.kind : 'note'}`;

  if (callout.collapsible) {
    return (
      <details className={className} open={!callout.folded}>
        <summary className="ie-callout__title">
          <Icon name={icon} size={16} className="ie-callout__icon" />
          {title}
        </summary>
        {body}
      </details>
    );
  }

  return (
    <div className={className}>
      <p className="ie-callout__title">
        <Icon name={icon} size={16} className="ie-callout__icon" />
        {title}
      </p>
      {body}
    </div>
  );
}

function capitalise(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/**
 * A paragraph holding nothing but one image becomes a figure.
 *
 * The alt text then serves twice: as the description for a reader who cannot
 * see the image, and as the caption under it. A figure inside a paragraph
 * would be invalid, which is why this is decided here rather than when the
 * image itself is rendered.
 */
function soleImageOf(node: Paragraph): Image | null {
  const meaningful = node.children.filter(
    (child) => !(child.type === 'text' && child.value.trim() === ''),
  );
  const only = meaningful.length === 1 ? meaningful[0] : undefined;
  return only && only.type === 'image' && only.alt?.trim() ? only : null;
}
