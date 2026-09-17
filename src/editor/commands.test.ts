import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { beforeEach, describe, expect, it } from 'vitest';

import {
  continueList,
  indentListItem,
  parseListMarker,
  setHeadingLevel,
  toggleTask,
  toggleWrap,
} from './commands';

function view(doc: string, cursor?: number): EditorView {
  const anchor = cursor ?? doc.length;
  return new EditorView({
    state: EditorState.create({ doc, selection: { anchor } }),
  });
}

function select(doc: string, from: number, to: number): EditorView {
  return new EditorView({
    state: EditorState.create({ doc, selection: { anchor: from, head: to } }),
  });
}

describe('parsing list markers', () => {
  it('recognises bullets, numbers and tasks', () => {
    expect(parseListMarker('- item')).toMatchObject({ bullet: '-', number: null, task: false });
    expect(parseListMarker('* item')).toMatchObject({ bullet: '*' });
    expect(parseListMarker('3. item')).toMatchObject({ number: 3 });
    expect(parseListMarker('3) item')).toMatchObject({ number: 3 });
    expect(parseListMarker('- [ ] task')).toMatchObject({ task: true, content: 'task' });
    expect(parseListMarker('- [x] done')).toMatchObject({ task: true });
  });

  it('records indentation', () => {
    expect(parseListMarker('    - nested')).toMatchObject({ indent: '    ' });
  });

  it('returns null for a line that is not a list item', () => {
    expect(parseListMarker('plain text')).toBeNull();
    expect(parseListMarker('# Heading')).toBeNull();
    expect(parseListMarker('-no space')).toBeNull();
  });
});

describe('continuing lists', () => {
  it('starts the next bullet', () => {
    const editor = view('- first');
    expect(continueList(editor)).toBe(true);
    expect(editor.state.doc.toString()).toBe('- first\n- ');
  });

  it('increments an ordered list', () => {
    const editor = view('3. third');
    continueList(editor);
    expect(editor.state.doc.toString()).toBe('3. third\n4. ');
  });

  it('carries an unchecked box onto the next task', () => {
    const editor = view('- [x] done');
    continueList(editor);
    expect(editor.state.doc.toString()).toBe('- [x] done\n- [ ] ');
  });

  it('preserves indentation', () => {
    const editor = view('    - nested');
    continueList(editor);
    expect(editor.state.doc.toString()).toBe('    - nested\n    - ');
  });

  it('ends the list when the item is empty', () => {
    const editor = view('- first\n- ');
    continueList(editor);
    expect(editor.state.doc.toString()).toBe('- first\n');
  });

  it('declines outside a list, so Enter behaves normally', () => {
    const editor = view('plain paragraph');
    expect(continueList(editor)).toBe(false);
    expect(editor.state.doc.toString()).toBe('plain paragraph');
  });

  it('declines mid-item, so Enter splits the line normally', () => {
    const editor = view('- first item', 4);
    expect(continueList(editor)).toBe(false);
  });
});

describe('indenting list items', () => {
  it('indents with Tab', () => {
    const editor = view('- item');
    expect(indentListItem(false)(editor)).toBe(true);
    expect(editor.state.doc.toString()).toBe('  - item');
  });

  it('outdents with Shift+Tab', () => {
    const editor = view('    - item');
    indentListItem(true)(editor);
    expect(editor.state.doc.toString()).toBe('  - item');
  });

  it('declines outside a list, so Tab inserts a tab', () => {
    const editor = view('plain');
    expect(indentListItem(false)(editor)).toBe(false);
  });
});

describe('wrapping', () => {
  it('wraps a selection', () => {
    const editor = select('make this bold', 5, 14);
    toggleWrap('**')(editor);
    expect(editor.state.doc.toString()).toBe('make **this bold**');
  });

  it('unwraps when the markers are already outside the selection', () => {
    const editor = select('make **this bold**', 7, 16);
    toggleWrap('**')(editor);
    expect(editor.state.doc.toString()).toBe('make this bold');
  });

  it('unwraps when the markers are inside the selection', () => {
    const editor = select('make **this bold**', 5, 18);
    toggleWrap('**')(editor);
    expect(editor.state.doc.toString()).toBe('make this bold');
  });

  it('inserts an empty pair and puts the cursor between them', () => {
    const editor = view('text ');
    toggleWrap('**')(editor);
    expect(editor.state.doc.toString()).toBe('text ****');
    expect(editor.state.selection.main.head).toBe(7);
  });
});

describe('headings', () => {
  it('adds a heading marker', () => {
    const editor = view('Title');
    setHeadingLevel(2)(editor);
    expect(editor.state.doc.toString()).toBe('## Title');
  });

  it('changes an existing level', () => {
    const editor = view('# Title');
    setHeadingLevel(3)(editor);
    expect(editor.state.doc.toString()).toBe('### Title');
  });

  it('removes the marker when the same level is chosen again', () => {
    const editor = view('## Title');
    setHeadingLevel(2)(editor);
    expect(editor.state.doc.toString()).toBe('Title');
  });

  it('level zero strips any heading', () => {
    const editor = view('#### Title');
    setHeadingLevel(0)(editor);
    expect(editor.state.doc.toString()).toBe('Title');
  });

  it('applies to every line in a selection', () => {
    const editor = select('One\nTwo\nThree', 0, 13);
    setHeadingLevel(2)(editor);
    expect(editor.state.doc.toString()).toBe('## One\n## Two\n## Three');
  });
});

describe('tasks', () => {
  let editor: EditorView;

  beforeEach(() => {
    editor = view('- [ ] something to do', 3);
  });

  it('checks an unchecked box', () => {
    expect(toggleTask(editor)).toBe(true);
    expect(editor.state.doc.toString()).toBe('- [x] something to do');
  });

  it('unchecks a checked box', () => {
    toggleTask(editor);
    toggleTask(editor);
    expect(editor.state.doc.toString()).toBe('- [ ] something to do');
  });

  it('declines on a line with no checkbox', () => {
    expect(toggleTask(view('- plain item', 3))).toBe(false);
  });
});
