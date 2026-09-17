/**
 * The editing surface.
 *
 * CodeMirror is created once per mount and then driven by transactions. React
 * never re-renders it: the document lives in CodeMirror's own state, and props
 * that change — the note's content when it is reloaded from disk, the font
 * size — are pushed in through effects. Re-creating the editor on every render
 * would lose the undo history and the cursor, which is exactly what a writer
 * notices.
 */

import { autocompletion, closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { languages } from '@codemirror/language-data';
import { indentUnit } from '@codemirror/language';
import { highlightSelectionMatches, search, searchKeymap } from '@codemirror/search';
import { Compartment, EditorState } from '@codemirror/state';
import {
  EditorView,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  placeholder as placeholderExtension,
  rectangularSelection,
} from '@codemirror/view';
import { useEffect, useRef } from 'react';

import type { EditorPreferences } from '@/state/settingsStore';
import type { VaultPath } from '@/types/domain';

import { markdownKeymap } from './commands';
import { tagCompletion, wikiLinkCompletion } from './completion';
import { livePreview, livePreviewTheme, type PreviewHandlers } from './livePreview';
import { editorTheme, highlighting, readableLineLength } from './theme';

export interface MarkdownEditorProps {
  path: VaultPath;
  /** The note's content. Changing it replaces the document. */
  content: string;
  preferences: EditorPreferences;
  onChange: (content: string) => void;
  onSave: () => void;
  /** Cursor moved, for the status bar and for remembering the position. */
  onCursorChange?: (position: { line: number; column: number; offset: number }) => void;
  onFollowLink: (target: string, newPane: boolean) => void;
  onFollowTag: (tag: string) => void;
  isResolved: (target: string) => boolean;
  resolveAsset: (target: string) => string | null;
  /** Where to put the cursor when the editor first appears. */
  initialCursor?: number;
  readOnly?: boolean;
  placeholder?: string;
}

export function MarkdownEditor(props: MarkdownEditorProps) {
  const host = useRef<HTMLDivElement | null>(null);
  const view = useRef<EditorView | null>(null);

  // Callbacks are read through a ref so changing one does not tear down the
  // editor. Only the note's identity does that.
  const callbacks = useRef(props);
  callbacks.current = props;

  const preferenceCompartment = useRef(new Compartment());
  const readOnlyCompartment = useRef(new Compartment());

  useEffect(() => {
    if (!host.current) return;

    const handlers: PreviewHandlers = {
      onFollowLink: (target, newPane) => callbacks.current.onFollowLink(target, newPane),
      onFollowTag: (tag) => callbacks.current.onFollowTag(tag),
      isResolved: (target) => callbacks.current.isResolved(target),
      resolveAsset: (target) => callbacks.current.resolveAsset(target),
    };

    const state = EditorState.create({
      doc: callbacks.current.content,
      selection: {
        anchor: Math.min(callbacks.current.initialCursor ?? 0, callbacks.current.content.length),
      },
      extensions: [
        history(),
        drawSelection(),
        dropCursor(),
        rectangularSelection(),
        highlightActiveLine(),
        highlightSelectionMatches(),
        highlightSpecialChars(),
        EditorState.allowMultipleSelections.of(true),
        EditorView.lineWrapping,
        search({ top: true }),
        markdown({ base: markdownLanguage, codeLanguages: languages, addKeymap: false }),
        editorTheme,
        highlighting,
        livePreviewTheme,
        autocompletion({
          override: [wikiLinkCompletion({ currentPath: () => callbacks.current.path }), tagCompletion()],
          closeOnBlur: true,
          activateOnTyping: true,
          icons: false,
        }),
        // Order matters: the Markdown bindings must see Enter and Tab before
        // the defaults do, or list continuation never runs.
        keymap.of([
          ...markdownKeymap,
          ...closeBracketsKeymap,
          ...searchKeymap,
          ...historyKeymap,
          ...defaultKeymap,
          indentWithTab,
          {
            key: 'Mod-s',
            preventDefault: true,
            run: () => {
              callbacks.current.onSave();
              return true;
            },
          },
        ]),
        preferenceCompartment.current.of(preferenceExtensions(callbacks.current.preferences, handlers)),
        readOnlyCompartment.current.of(EditorState.readOnly.of(callbacks.current.readOnly ?? false)),
        callbacks.current.placeholder
          ? placeholderExtension(callbacks.current.placeholder)
          : [],
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            callbacks.current.onChange(update.state.doc.toString());
          }
          if (update.selectionSet || update.docChanged) {
            const head = update.state.selection.main.head;
            const line = update.state.doc.lineAt(head);
            callbacks.current.onCursorChange?.({
              line: line.number,
              column: head - line.from + 1,
              offset: head,
            });
          }
        }),
      ],
    });

    const editor = new EditorView({ state, parent: host.current });
    view.current = editor;
    editor.focus();

    return () => {
      editor.destroy();
      view.current = null;
    };
    // Re-created only when the note changes, which is what makes the undo
    // history per note rather than per session. Everything else it needs is
    // read through `callbacks`, so this deliberately depends on the path alone.
  }, [props.path]);

  // Content replaced from outside — a reload after an external edit, or a
  // template insertion. Only dispatch when it genuinely differs, or every
  // keystroke would bounce back through here.
  useEffect(() => {
    const editor = view.current;
    if (!editor) return;
    const current = editor.state.doc.toString();
    if (current === props.content) return;

    const cursor = editor.state.selection.main.head;
    editor.dispatch({
      changes: { from: 0, to: current.length, insert: props.content },
      selection: { anchor: Math.min(cursor, props.content.length) },
    });
  }, [props.content]);

  useEffect(() => {
    const editor = view.current;
    if (!editor) return;
    const handlers: PreviewHandlers = {
      onFollowLink: (target, newPane) => callbacks.current.onFollowLink(target, newPane),
      onFollowTag: (tag) => callbacks.current.onFollowTag(tag),
      isResolved: (target) => callbacks.current.isResolved(target),
      resolveAsset: (target) => callbacks.current.resolveAsset(target),
    };
    editor.dispatch({
      effects: preferenceCompartment.current.reconfigure(
        preferenceExtensions(props.preferences, handlers),
      ),
    });
  }, [props.preferences]);

  useEffect(() => {
    const editor = view.current;
    if (!editor) return;
    editor.dispatch({
      effects: readOnlyCompartment.current.reconfigure(
        EditorState.readOnly.of(props.readOnly ?? false),
      ),
    });
  }, [props.readOnly]);

  return <div className="ie-editor" ref={host} data-path={props.path} />;
}

/** The extensions a preference change can swap in and out. */
function preferenceExtensions(preferences: EditorPreferences, handlers: PreviewHandlers) {
  return [
    preferences.livePreview ? livePreview(handlers) : [],
    preferences.readableLineLength ? readableLineLength : [],
    preferences.lineNumbers ? lineNumbers() : [],
    preferences.autoCloseBrackets ? closeBrackets() : [],
    indentUnit.of(preferences.indentWithSpaces ? ' '.repeat(preferences.tabSize) : '\t'),
    EditorState.tabSize.of(preferences.tabSize),
    EditorView.contentAttributes.of({
      spellcheck: preferences.spellcheck ? 'true' : 'false',
      'data-gramm': 'false',
    }),
  ];
}

/** Move the cursor to a line, for the outline and for following a link. */
export function scrollToLine(view: EditorView | null, line: number): void {
  if (!view) return;
  const target = Math.max(1, Math.min(line + 1, view.state.doc.lines));
  const position = view.state.doc.line(target).from;
  view.dispatch({
    selection: { anchor: position },
    effects: EditorView.scrollIntoView(position, { y: 'start', yMargin: 48 }),
  });
  view.focus();
}
