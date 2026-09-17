/**
 * The editor's appearance.
 *
 * Every colour is a CSS custom property, so the editor follows whatever theme
 * is active — including a user theme — without CodeMirror needing to know a
 * theme system exists.
 */

import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';

export const editorTheme = EditorView.theme({
  '&': {
    color: 'var(--text-normal)',
    backgroundColor: 'transparent',
    height: '100%',
    fontSize: 'var(--font-size-editor)',
  },
  '.cm-content': {
    fontFamily: 'var(--font-text)',
    lineHeight: '1.7',
    padding: 'var(--space-5) 0 40vh 0',
    caretColor: 'var(--cursor-color)',
  },
  '.cm-scroller': {
    fontFamily: 'var(--font-text)',
    overflow: 'auto',
  },
  '&.cm-focused': { outline: 'none' },
  '.cm-line': { padding: '0 var(--space-4)' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--cursor-color)', borderLeftWidth: '2px' },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': {
    backgroundColor: 'var(--selection-background)',
  },
  '.cm-activeLine': { backgroundColor: 'transparent' },
  '.cm-gutters': {
    backgroundColor: 'transparent',
    color: 'var(--text-faint)',
    border: 'none',
    fontFamily: 'var(--font-monospace)',
    fontSize: 'var(--font-size-smaller)',
  },
  '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--text-muted)' },
  '.cm-foldPlaceholder': {
    backgroundColor: 'var(--background-secondary)',
    border: '1px solid var(--border)',
    color: 'var(--text-muted)',
    borderRadius: 'var(--radius-s)',
    padding: '0 6px',
  },
  '.cm-panels': {
    backgroundColor: 'var(--background-secondary)',
    color: 'var(--text-normal)',
    borderTop: '1px solid var(--border)',
  },
  '.cm-searchMatch': {
    backgroundColor: 'var(--highlight-background)',
    outline: '1px solid transparent',
  },
  '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: 'var(--accent-muted)' },
  '.cm-tooltip': {
    backgroundColor: 'var(--background-secondary)',
    border: '1px solid var(--border)',
    borderRadius: 'var(--radius-m)',
    boxShadow: 'var(--shadow-popover)',
    color: 'var(--text-normal)',
    overflow: 'hidden',
  },
  '.cm-tooltip-autocomplete > ul > li': {
    padding: '5px 10px',
    fontFamily: 'var(--font-interface)',
    fontSize: 'var(--font-size-small)',
  },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    backgroundColor: 'var(--background-modifier-active)',
    color: 'var(--text-normal)',
  },
  '.cm-completionIcon': { display: 'none' },
  '.cm-completionDetail': {
    color: 'var(--text-faint)',
    fontStyle: 'normal',
    marginLeft: 'var(--space-2)',
  },
});

/** Widen the text column so prose stays readable on a wide window. */
export const readableLineLength = EditorView.theme({
  '.cm-content': { maxWidth: '46rem', margin: '0 auto' },
  '.cm-line': { padding: '0 var(--space-2)' },
});

export const markdownHighlighting = HighlightStyle.define([
  { tag: tags.heading1, fontSize: '1.7em', fontWeight: '700', color: 'var(--h1-color)', lineHeight: '1.3' },
  { tag: tags.heading2, fontSize: '1.45em', fontWeight: '700', color: 'var(--h2-color)', lineHeight: '1.3' },
  { tag: tags.heading3, fontSize: '1.25em', fontWeight: '650', color: 'var(--h3-color)' },
  { tag: tags.heading4, fontSize: '1.1em', fontWeight: '650', color: 'var(--h4-color)' },
  { tag: tags.heading5, fontWeight: '650', color: 'var(--h5-color)' },
  { tag: tags.heading6, fontWeight: '650', color: 'var(--h6-color)' },
  { tag: tags.strong, fontWeight: '700', color: 'var(--text-normal)' },
  { tag: tags.emphasis, fontStyle: 'italic' },
  { tag: tags.strikethrough, textDecoration: 'line-through', color: 'var(--text-muted)' },
  { tag: tags.link, color: 'var(--link-color)' },
  { tag: tags.url, color: 'var(--text-faint)' },
  { tag: tags.quote, color: 'var(--text-muted)', fontStyle: 'italic' },
  {
    tag: tags.monospace,
    fontFamily: 'var(--font-monospace)',
    fontSize: '0.92em',
    color: 'var(--code-text)',
    background: 'var(--code-background)',
    borderRadius: 'var(--radius-s)',
    padding: '0.1em 0.3em',
  },
  { tag: tags.meta, color: 'var(--text-faint)' },
  { tag: tags.processingInstruction, color: 'var(--text-faint)' },
  { tag: tags.contentSeparator, color: 'var(--text-faint)' },
  { tag: tags.list, color: 'var(--text-muted)' },
  { tag: tags.keyword, color: 'var(--accent)' },
  { tag: tags.comment, color: 'var(--text-faint)', fontStyle: 'italic' },
  { tag: tags.string, color: 'var(--text-success)' },
  { tag: tags.number, color: 'var(--text-warning)' },
  { tag: tags.variableName, color: 'var(--text-normal)' },
]);

export const highlighting = syntaxHighlighting(markdownHighlighting, { fallback: true });
