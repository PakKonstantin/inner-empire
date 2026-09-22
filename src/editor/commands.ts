/**
 * Editing behaviours specific to Markdown.
 *
 * Each one is a keymap entry that returns `false` when it does not apply, so
 * CodeMirror falls through to its own default. That is what keeps Enter
 * behaving normally everywhere except inside a list.
 */

import type { ChangeSpec, EditorState } from '@codemirror/state';
import { EditorSelection } from '@codemirror/state';
import type { Command, KeyBinding } from '@codemirror/view';

/** A list marker at the start of a line: bullet, number, or task. */
interface ListMarker {
  indent: string;
  bullet: string;
  /** For an ordered list, the number to continue from. */
  number: number | null;
  task: boolean;
  /** Everything after the marker. */
  content: string;
  /** Total length of indent plus marker plus the space after it. */
  prefixLength: number;
}

const LIST_PATTERN = /^(\s*)([-*+]|\d+[.)])\s+(\[[ xX]\]\s+)?(.*)$/;

export function parseListMarker(line: string): ListMarker | null {
  const match = LIST_PATTERN.exec(line);
  if (!match) return null;

  const [, indent = '', bullet = '', task, content = ''] = match;
  const numberMatch = /^(\d+)[.)]$/.exec(bullet);

  return {
    indent,
    bullet,
    number: numberMatch ? Number(numberMatch[1]) : null,
    task: Boolean(task),
    content,
    prefixLength: line.length - content.length,
  };
}

/** Build the marker that continues a list. */
function nextMarker(marker: ListMarker): string {
  const bullet = marker.number === null ? marker.bullet : `${marker.number + 1}${marker.bullet.slice(-1)}`;
  const task = marker.task ? '[ ] ' : '';
  return `${marker.indent}${bullet} ${task}`;
}

/**
 * Enter inside a list starts the next item.
 *
 * Pressing Enter on an *empty* item ends the list instead, which is what every
 * writer expects and the only way to get out of a list without reaching for the
 * mouse.
 */
export const continueList: Command = (view) => {
  const { state } = view;
  const changes: ChangeSpec[] = [];
  let handled = false;

  const selection = state.changeByRange((range) => {
    if (!range.empty) return { range };
    const line = state.doc.lineAt(range.head);
    const marker = parseListMarker(line.text);
    if (!marker) return { range };
    // Only continue when the cursor is at the end of the line; pressing Enter
    // mid-item should split it normally.
    if (range.head !== line.to) return { range };

    handled = true;

    if (marker.content.trim() === '') {
      // An empty item: remove the marker and leave the list.
      return {
        changes: { from: line.from, to: line.to, insert: '' },
        range: EditorSelection.cursor(line.from),
      };
    }

    const insert = `\n${nextMarker(marker)}`;
    return {
      changes: { from: range.head, insert },
      range: EditorSelection.cursor(range.head + insert.length),
    };
  });

  if (!handled) return false;
  view.dispatch(selection, { scrollIntoView: true, userEvent: 'input' });
  void changes;
  return true;
};

/** Tab indents a list item rather than inserting a tab character. */
export const indentListItem = (outdent: boolean): Command => {
  return (view) => {
    const { state } = view;
    let handled = false;

    const transaction = state.changeByRange((range) => {
      const line = state.doc.lineAt(range.head);
      const marker = parseListMarker(line.text);
      if (!marker) return { range };
      handled = true;

      if (outdent) {
        const removal = Math.min(marker.indent.length, 2);
        if (removal === 0) return { range };
        return {
          changes: { from: line.from, to: line.from + removal, insert: '' },
          range: EditorSelection.cursor(Math.max(line.from, range.head - removal)),
        };
      }

      return {
        changes: { from: line.from, insert: '  ' },
        range: EditorSelection.cursor(range.head + 2),
      };
    });

    if (!handled) return false;
    view.dispatch(transaction, { userEvent: outdent ? 'delete.dedent' : 'input.indent' });
    return true;
  };
};

/**
 * Wrap the selection in a marker, or unwrap it if it is already wrapped.
 *
 * With nothing selected, insert the pair and put the cursor between them, so
 * Ctrl+B then typing produces bold text.
 */
export function toggleWrap(marker: string): Command {
  return (view) => {
    const { state } = view;
    const transaction = state.changeByRange((range) => {
      const before = state.sliceDoc(Math.max(0, range.from - marker.length), range.from);
      const after = state.sliceDoc(range.to, Math.min(state.doc.length, range.to + marker.length));

      if (before === marker && after === marker) {
        return {
          changes: [
            { from: range.from - marker.length, to: range.from, insert: '' },
            { from: range.to, to: range.to + marker.length, insert: '' },
          ],
          range: EditorSelection.range(range.from - marker.length, range.to - marker.length),
        };
      }

      const selected = state.sliceDoc(range.from, range.to);
      if (selected.startsWith(marker) && selected.endsWith(marker) && selected.length > marker.length * 2) {
        const unwrapped = selected.slice(marker.length, selected.length - marker.length);
        return {
          changes: { from: range.from, to: range.to, insert: unwrapped },
          range: EditorSelection.range(range.from, range.from + unwrapped.length),
        };
      }

      const insert = `${marker}${selected}${marker}`;
      return {
        changes: { from: range.from, to: range.to, insert },
        range: range.empty
          ? EditorSelection.cursor(range.from + marker.length)
          : EditorSelection.range(range.from + marker.length, range.to + marker.length),
      };
    });

    view.dispatch(transaction, { userEvent: 'input' });
    return true;
  };
}

/** Set, change or remove the heading level of the current lines. */
export function setHeadingLevel(level: number): Command {
  return (view) => {
    const { state } = view;
    const changes: ChangeSpec[] = [];
    const visited = new Set<number>();

    for (const range of state.selection.ranges) {
      const fromLine = state.doc.lineAt(range.from).number;
      const toLine = state.doc.lineAt(range.to).number;

      for (let number = fromLine; number <= toLine; number += 1) {
        // A multi-range selection can cover the same line twice; changing it
        // twice would produce overlapping changes.
        if (visited.has(number)) continue;
        visited.add(number);

        const line = state.doc.line(number);
        const existing = /^(#{1,6})\s+/.exec(line.text);
        const currentLength = existing ? existing[0].length : 0;

        // Choosing the level a line already has removes it, so the same
        // shortcut toggles.
        const sameLevel = existing !== null && existing[1]!.length === level;
        const replacement = level === 0 || sameLevel ? '' : `${'#'.repeat(level)} `;
        if (replacement === line.text.slice(0, currentLength)) continue;

        changes.push({ from: line.from, to: line.from + currentLength, insert: replacement });
      }
    }

    if (changes.length === 0) return true;
    // No explicit selection: CodeMirror maps the existing one through the
    // changes, which is the only way to get it right when lines shrink.
    view.dispatch({ changes, userEvent: 'input' });
    return true;
  };
}

/** Toggle a task checkbox on the current line. */
export const toggleTask: Command = (view) => {
  const { state } = view;
  let handled = false;

  const transaction = state.changeByRange((range) => {
    const line = state.doc.lineAt(range.head);
    const match = /^(\s*(?:[-*+]|\d+[.)])\s+)\[([ xX])\]/.exec(line.text);
    if (!match) return { range };
    handled = true;

    const prefix = match[1]!;
    const checked = match[2] !== ' ';
    return {
      changes: {
        from: line.from + prefix.length + 1,
        to: line.from + prefix.length + 2,
        insert: checked ? ' ' : 'x',
      },
      range,
    };
  });

  if (!handled) return false;
  view.dispatch(transaction, { userEvent: 'input' });
  return true;
};

/** Insert a link around the selection, ready for the target to be typed. */
export const insertWikiLink: Command = (view) => {
  const { state } = view;
  const transaction = state.changeByRange((range) => {
    const selected = state.sliceDoc(range.from, range.to);
    const insert = `[[${selected}]]`;
    return {
      changes: { from: range.from, to: range.to, insert },
      range: EditorSelection.cursor(range.from + 2 + selected.length),
    };
  });
  view.dispatch(transaction, { userEvent: 'input' });
  return true;
};

/** The text of the line the cursor is on, for the status bar. */
export function currentLineText(state: EditorState): string {
  return state.doc.lineAt(state.selection.main.head).text;
}

export const markdownKeymap: KeyBinding[] = [
  { key: 'Enter', run: continueList },
  { key: 'Tab', run: indentListItem(false) },
  { key: 'Shift-Tab', run: indentListItem(true) },
  { key: 'Mod-b', run: toggleWrap('**') },
  { key: 'Mod-i', run: toggleWrap('*') },
  { key: 'Mod-Shift-x', run: toggleWrap('~~') },
  { key: 'Mod-e', run: toggleWrap('`') },
  { key: 'Mod-Shift-k', run: insertWikiLink },
  { key: 'Mod-Enter', run: toggleTask },
  { key: 'Mod-1', run: setHeadingLevel(1) },
  { key: 'Mod-2', run: setHeadingLevel(2) },
  { key: 'Mod-3', run: setHeadingLevel(3) },
  { key: 'Mod-4', run: setHeadingLevel(4) },
  { key: 'Mod-0', run: setHeadingLevel(0) },
];
