/**
 * Live preview.
 *
 * The approach matters: this decorates the *source document* rather than
 * swapping in a rendered view. Markup is concealed on lines the cursor is not
 * on, and revealed the moment the cursor enters. Because the document in the
 * editor is always the real Markdown, the cursor, the selection, undo history
 * and every editing command keep working on the text the user actually has —
 * which is what makes cursor behaviour sane, and what a separate rendered pane
 * cannot give you.
 */

import { syntaxTree } from '@codemirror/language';
import type { EditorState, Range } from '@codemirror/state';
import { RangeSetBuilder, StateField } from '@codemirror/state';
import type { DecorationSet, ViewUpdate } from '@codemirror/view';
import { Decoration, EditorView, ViewPlugin, WidgetType } from '@codemirror/view';

import { findTags, findWikiLinks, isImageTarget } from '@/markdown/extensions';

/** Callbacks the decorations need, supplied once when the editor is created. */
export interface PreviewHandlers {
  onFollowLink: (target: string, newPane: boolean) => void;
  onFollowTag: (tag: string) => void;
  /** Whether a link target exists, so unresolved links can look different. */
  isResolved: (target: string) => boolean;
  resolveAsset: (target: string) => string | null;
}

const conceal = Decoration.replace({});

/** A rendered wiki link, shown in place of its source text. */
class LinkWidget extends WidgetType {
  constructor(
    private readonly label: string,
    private readonly target: string,
    private readonly resolved: boolean,
    private readonly handlers: PreviewHandlers,
  ) {
    super();
  }

  override eq(other: LinkWidget): boolean {
    return (
      other.label === this.label &&
      other.target === this.target &&
      other.resolved === this.resolved
    );
  }

  toDOM(): HTMLElement {
    const anchor = document.createElement('span');
    anchor.className = this.resolved ? 'cm-ie-link' : 'cm-ie-link cm-ie-link-unresolved';
    anchor.textContent = this.label;
    anchor.title = this.resolved ? `Open ${this.target}` : `Create ${this.target}`;
    anchor.addEventListener('mousedown', (event) => {
      // Only a modified click or a plain click on the label navigates; a drag
      // to select text must still work.
      if (event.button !== 0) return;
      event.preventDefault();
      this.handlers.onFollowLink(this.target, event.ctrlKey || event.metaKey);
    });
    return anchor;
  }

  override ignoreEvent(): boolean {
    return false;
  }
}

class TagWidget extends WidgetType {
  constructor(
    private readonly name: string,
    private readonly handlers: PreviewHandlers,
  ) {
    super();
  }

  override eq(other: TagWidget): boolean {
    return other.name === this.name;
  }

  toDOM(): HTMLElement {
    const span = document.createElement('span');
    span.className = 'cm-ie-tag';
    span.textContent = `#${this.name}`;
    span.addEventListener('mousedown', (event) => {
      if (event.button !== 0) return;
      event.preventDefault();
      this.handlers.onFollowTag(this.name);
    });
    return span;
  }

  override ignoreEvent(): boolean {
    return false;
  }
}

/** An inline image, for `![[picture.png]]`. */
class ImageWidget extends WidgetType {
  constructor(
    private readonly source: string,
    private readonly alt: string,
    private readonly width: number | undefined,
  ) {
    super();
  }

  override eq(other: ImageWidget): boolean {
    return other.source === this.source && other.width === this.width;
  }

  toDOM(): HTMLElement {
    const wrapper = document.createElement('span');
    wrapper.className = 'cm-ie-image';
    const image = document.createElement('img');
    image.src = this.source;
    image.alt = this.alt;
    image.loading = 'lazy';
    if (this.width) image.width = this.width;
    wrapper.appendChild(image);
    return wrapper;
  }
}

/** A horizontal rule drawn instead of three dashes. */
class RuleWidget extends WidgetType {
  override eq(): boolean {
    return true;
  }

  toDOM(): HTMLElement {
    const element = document.createElement('span');
    element.className = 'cm-ie-rule';
    return element;
  }
}

/** Lines the cursor or selection touches, which must show their raw markup. */
function activeLines(state: EditorState): Set<number> {
  const lines = new Set<number>();
  for (const range of state.selection.ranges) {
    const from = state.doc.lineAt(range.from).number;
    const to = state.doc.lineAt(range.to).number;
    for (let line = from; line <= to; line += 1) lines.add(line);
  }
  return lines;
}

function buildDecorations(view: EditorView, handlers: PreviewHandlers): DecorationSet {
  const decorations: Range<Decoration>[] = [];
  const state = view.state;
  const active = activeLines(state);

  // Only the visible viewport is decorated, so a ten-thousand-line note costs
  // the same as a short one.
  for (const { from, to } of view.visibleRanges) {
    const startLine = state.doc.lineAt(from).number;
    const endLine = state.doc.lineAt(to).number;

    for (let lineNumber = startLine; lineNumber <= endLine; lineNumber += 1) {
      const line = state.doc.line(lineNumber);
      const isActive = active.has(lineNumber);
      const text = line.text;

      // Wiki links and embeds.
      for (const token of findWikiLinks(text)) {
        const start = line.from + token.start;
        const end = line.from + token.end;
        if (isActive) continue;

        if (token.kind === 'embed' && isImageTarget(token.target)) {
          const source = handlers.resolveAsset(token.target);
          if (source) {
            const width = token.alias && /^\d+$/.test(token.alias) ? Number(token.alias) : undefined;
            decorations.push(
              Decoration.replace({
                widget: new ImageWidget(source, token.target, width),
              }).range(start, end),
            );
            continue;
          }
        }

        const label = token.alias ?? token.target ?? '';
        decorations.push(
          Decoration.replace({
            widget: new LinkWidget(
              token.kind === 'embed' ? `❖ ${label}` : label,
              token.target,
              handlers.isResolved(token.target),
              handlers,
            ),
          }).range(start, end),
        );
      }

      // Tags, skipping any that fall inside a link.
      const links = findWikiLinks(text);
      for (const tag of findTags(text, (offset) =>
        links.some((link) => offset >= link.start && offset < link.end),
      )) {
        if (isActive) continue;
        decorations.push(
          Decoration.replace({ widget: new TagWidget(tag.name, handlers) }).range(
            line.from + tag.start,
            line.from + tag.end,
          ),
        );
      }

      if (isActive) continue;

      // A horizontal rule.
      if (/^ {0,3}(-{3,}|\*{3,}|_{3,})\s*$/.test(text) && lineNumber > 1) {
        decorations.push(
          Decoration.replace({ widget: new RuleWidget() }).range(line.from, line.to),
        );
        continue;
      }

      // Heading hashes.
      const heading = /^(#{1,6})\s+/.exec(text);
      if (heading) {
        decorations.push(conceal.range(line.from, line.from + heading[0].length));
      }

      // Blockquote markers.
      const quote = /^(\s*>\s?)+/.exec(text);
      if (quote) {
        decorations.push(conceal.range(line.from, line.from + quote[0].length));
      }

      // A trailing block identifier is machinery, not prose.
      const block = /\s\^[A-Za-z0-9_-]+\s*$/.exec(text);
      if (block && block.index !== undefined) {
        decorations.push(conceal.range(line.from + block.index, line.to));
      }
    }
  }

  // Emphasis markers come from the syntax tree rather than from matching
  // asterisks, so `a * b * c` is not mistaken for emphasis.
  for (const { from, to } of view.visibleRanges) {
    syntaxTree(state).iterate({
      from,
      to,
      enter: (node) => {
        if (node.name !== 'EmphasisMark' && node.name !== 'CodeMark') return;
        const lineNumber = state.doc.lineAt(node.from).number;
        if (active.has(lineNumber)) return;
        decorations.push(conceal.range(node.from, node.to));
      },
    });
  }

  decorations.sort((a, b) => a.from - b.from || a.to - b.to);

  const builder = new RangeSetBuilder<Decoration>();
  let lastEnd = -1;
  for (const decoration of decorations) {
    // Overlaps would make CodeMirror throw; the outer decoration wins.
    if (decoration.from < lastEnd) continue;
    builder.add(decoration.from, decoration.to, decoration.value);
    lastEnd = decoration.to;
  }
  return builder.finish();
}

export function livePreview(handlers: PreviewHandlers) {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;

      constructor(view: EditorView) {
        this.decorations = buildDecorations(view, handlers);
      }

      update(update: ViewUpdate): void {
        // Selection changes matter as much as document changes: moving the
        // cursor onto a line is what reveals its markup.
        if (update.docChanged || update.selectionSet || update.viewportChanged) {
          this.decorations = buildDecorations(update.view, handlers);
        }
      }
    },
    { decorations: (plugin) => plugin.decorations },
  );
}

/** Styles for the widgets above. */
export const livePreviewTheme = EditorView.theme({
  '.cm-ie-link': {
    color: 'var(--link-color)',
    cursor: 'pointer',
    textDecoration: 'none',
  },
  '.cm-ie-link:hover': { textDecoration: 'underline' },
  '.cm-ie-link-unresolved': {
    color: 'var(--link-unresolved-color)',
    textDecoration: 'underline dotted',
  },
  '.cm-ie-tag': {
    background: 'var(--tag-background)',
    color: 'var(--tag-color)',
    borderRadius: '10px',
    padding: '1px 8px',
    fontSize: '0.88em',
    cursor: 'pointer',
  },
  '.cm-ie-image img': {
    maxWidth: '100%',
    borderRadius: 'var(--radius-m)',
    display: 'block',
    margin: 'var(--space-2) 0',
  },
  '.cm-ie-rule': {
    display: 'block',
    borderTop: '1px solid var(--border)',
    margin: 'var(--space-3) 0',
    height: '0',
  },
});

/** Nothing concealed, for people who prefer to see the source. */
export const sourceModeField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update: (value) => value,
  provide: (field) => EditorView.decorations.from(field),
});
