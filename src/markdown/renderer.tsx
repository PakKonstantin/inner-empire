/**
 * Rendering Markdown to React elements.
 *
 * `mdast-util-from-markdown` gives a proper syntax tree, which the walk below
 * turns into elements. Going through an AST rather than producing an HTML
 * string means links, embeds and tags become real components with click
 * handlers, and nothing has to set `dangerouslySetInnerHTML` on note content.
 */

import type { Nodes as MdastNode, Parent, PhrasingContent, RootContent } from 'mdast';
import { fromMarkdown } from 'mdast-util-from-markdown';
import { gfmFromMarkdown } from 'mdast-util-gfm';
import { gfm } from 'micromark-extension-gfm';
import type { JSX, ReactNode } from 'react';
import { Fragment } from 'react';

import type { VaultPath } from '@/types/domain';

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
    case 'paragraph':
      return <p>{renderChildren(node, context)}</p>;

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

    case 'blockquote':
      return <blockquote>{renderChildren(node, context)}</blockquote>;

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
