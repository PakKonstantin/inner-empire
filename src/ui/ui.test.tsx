/**
 * The design system's tests.
 *
 * These check the things a component exists to guarantee and a class name
 * cannot: that a control has an accessible name, that a description is tied
 * to its field, that an error sets `aria-invalid` as well as a colour, that
 * arrow keys move focus inside a group. A visual regression is a screenshot's
 * job; these are the contracts.
 *
 * Rendered with React's own `act` into jsdom rather than through a testing
 * library, so the suite gains no dependency to assert on markup it can read
 * directly.
 */

import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ReactNode } from 'react';

import { Breadcrumb, Rail, SegmentedControl } from './Navigation';
import { Button, IconButton } from './Button';
import { Checkbox, Input, SearchInput, Select, Toggle } from './Input';
import { EmptyState, ErrorState, FilterChip, LoadingState, TagChip } from './Feedback';
import { Icon, iconForFile, iconNames } from './icons';
import { menu, separator } from './Menu';
import { CollapsibleSection, Panel } from './Panel';

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean;
}

globalThis.IS_REACT_ACT_ENVIRONMENT = true;

let mounted: { root: Root; container: HTMLElement } | null = null;

function render(element: ReactNode): HTMLElement {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const root = createRoot(container);
  act(() => {
    root.render(element);
  });
  mounted = { root, container };
  return container;
}

function click(element: Element | null | undefined): void {
  act(() => {
    (element as HTMLElement).click();
  });
}

/**
 * Look an element up by id.
 *
 * React's `useId` produces ids with colons in them, which a CSS selector
 * cannot hold unescaped, and this jsdom has no escaping helper.
 */
function byId(container: HTMLElement, id: string): Element | null {
  return container.ownerDocument.getElementById(id);
}

function press(element: Element | null | undefined, key: string): void {
  act(() => {
    element?.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
  });
}

function type(input: HTMLInputElement, value: string): void {
  act(() => {
    // React tracks the previous value on the node, so setting `.value`
    // directly is ignored unless the tracker is bypassed the way React's own
    // test utilities do.
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    setter?.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

afterEach(() => {
  if (mounted) {
    const { root, container } = mounted;
    act(() => root.unmount());
    container.remove();
    mounted = null;
  }
});

describe('Button', () => {
  it('shows its label and calls back when clicked', () => {
    const onClick = vi.fn();
    const container = render(<Button onClick={onClick}>Create note</Button>);

    const button = container.querySelector('button');
    expect(button?.textContent).toContain('Create note');
    click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it('refuses the click while it is loading, and says so', () => {
    const onClick = vi.fn();
    const container = render(
      <Button loading onClick={onClick}>
        Saving
      </Button>,
    );

    const button = container.querySelector('button');
    expect(button?.getAttribute('aria-busy')).toBe('true');
    expect((button as HTMLButtonElement).disabled).toBe(true);
    click(button);
    expect(onClick).not.toHaveBeenCalled();
  });

  it('is a real button, so Enter and Space work without any handling', () => {
    const container = render(<Button>Go</Button>);
    expect(container.querySelector('button')?.getAttribute('type')).toBe('button');
  });
});

describe('IconButton', () => {
  it('has an accessible name even though it shows only an icon', () => {
    const container = render(<IconButton icon="close" label="Close the note" />);
    const button = container.querySelector('button');

    expect(button?.getAttribute('aria-label')).toBe('Close the note');
    // The title gives a mouse user the same words.
    expect(button?.getAttribute('title')).toBe('Close the note');
    expect(button?.querySelector('svg')?.getAttribute('aria-hidden')).toBe('true');
  });

  it('reports a toggle state when it is one', () => {
    const container = render(<IconButton icon="pin" label="Pin" pressed />);
    expect(container.querySelector('button')?.getAttribute('aria-pressed')).toBe('true');
  });
});

describe('Icon', () => {
  it('is hidden from a screen reader unless it carries the meaning', () => {
    const plain = render(<Icon name="search" />);
    expect(plain.querySelector('svg')?.getAttribute('aria-hidden')).toBe('true');
    act(() => mounted!.root.unmount());
    mounted!.container.remove();
    mounted = null;

    const labelled = render(<Icon name="search" label="Search" />);
    const svg = labelled.querySelector('svg');
    expect(svg?.getAttribute('role')).toBe('img');
    expect(svg?.getAttribute('aria-label')).toBe('Search');
    expect(svg?.hasAttribute('aria-hidden')).toBe(false);
  });

  it('draws every name in the set', () => {
    // A missing path would render an empty box in the interface and nothing
    // in a review, which is how an icon set rots.
    for (const name of iconNames) {
      const container = render(<Icon name={name} />);
      expect(container.querySelectorAll('path').length).toBeGreaterThan(0);
      act(() => mounted!.root.unmount());
      mounted!.container.remove();
      mounted = null;
    }
  });

  it('picks an icon from the file extension', () => {
    expect(iconForFile('Notes/Today.md')).toBe('file-text');
    expect(iconForFile('a/b/diagram.canvas')).toBe('canvas');
    expect(iconForFile('Attachments/scan.PDF')).toBe('pdf');
    expect(iconForFile('Attachments/photo.jpeg')).toBe('image');
    expect(iconForFile('Attachments/talk.mp3')).toBe('audio');
    expect(iconForFile('Attachments/clip.webm')).toBe('video');
    expect(iconForFile('data.bin')).toBe('file');
  });
});

describe('Input', () => {
  it('ties the label, the description and the error to the control', () => {
    const container = render(
      <Input label="Note name" description="Without the extension" error="That name is taken" />,
    );

    const input = container.querySelector('input')!;
    const label = container.querySelector('label')!;
    expect(label.getAttribute('for')).toBe(input.id);
    expect(input.getAttribute('aria-invalid')).toBe('true');

    const describedBy = input.getAttribute('aria-describedby')!.split(' ');
    expect(describedBy).toHaveLength(2);
    for (const id of describedBy) {
      expect(byId(container, id)).not.toBeNull();
    }
    // The error is announced, not merely coloured.
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('That name is taken');
  });

  it('gives two instances of the same field different ids', () => {
    const container = render(
      <>
        <Input label="Name" />
        <Input label="Name" />
      </>,
    );
    const inputs = Array.from(container.querySelectorAll('input'));
    expect(inputs[0]!.id).not.toBe(inputs[1]!.id);
  });
});

describe('SearchInput', () => {
  it('clears on Escape before letting it close anything', () => {
    const onValueChange = vi.fn();
    const onEscape = vi.fn();
    const container = render(
      <SearchInput label="Search" value="draft" onValueChange={onValueChange} onEscape={onEscape} />,
    );

    press(container.querySelector('input'), 'Escape');
    expect(onValueChange).toHaveBeenCalledWith('');
    expect(onEscape).not.toHaveBeenCalled();
  });

  it('passes Escape on once the field is already empty', () => {
    const onEscape = vi.fn();
    const container = render(
      <SearchInput label="Search" value="" onValueChange={() => {}} onEscape={onEscape} />,
    );

    press(container.querySelector('input'), 'Escape');
    expect(onEscape).toHaveBeenCalledOnce();
  });

  it('offers a clear button only when there is something to clear', () => {
    const onValueChange = vi.fn();
    const container = render(
      <SearchInput label="Search" value="" onValueChange={onValueChange} />,
    );
    expect(container.querySelector('.ie-search-input__clear')).toBeNull();

    act(() =>
      mounted!.root.render(
        <SearchInput label="Search" value="x" onValueChange={onValueChange} />,
      ),
    );
    click(container.querySelector('.ie-search-input__clear'));
    expect(onValueChange).toHaveBeenCalledWith('');
  });

  it('reports what the user typed', () => {
    const onValueChange = vi.fn();
    const container = render(
      <SearchInput label="Search" value="" onValueChange={onValueChange} />,
    );
    type(container.querySelector('input')!, 'alpha');
    expect(onValueChange).toHaveBeenCalledWith('alpha');
  });
});

describe('Toggle', () => {
  it('is a switch that announces its state', () => {
    const onChange = vi.fn();
    const container = render(
      <Toggle checked={false} onChange={onChange} label="Show line numbers" />,
    );

    const toggle = container.querySelector('[role="switch"]')!;
    expect(toggle.getAttribute('aria-checked')).toBe('false');
    click(toggle);
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it('has a name even when its label is only a sibling', () => {
    // The label sits beside the button rather than wrapping it, so without an
    // explicit association the switch reads as nameless — text plainly on
    // screen and entirely absent to a screen reader.
    const container = render(
      <Toggle checked onChange={() => {}} label="Only notes with links" />,
    );

    const toggle = container.querySelector('[role="switch"]')!;
    const labelledBy = toggle.getAttribute('aria-labelledby');
    expect(labelledBy).toBeTruthy();
    expect(byId(container, labelledBy!)?.textContent).toBe('Only notes with links');
  });

  it('ties a description to the switch without making it the name', () => {
    const container = render(
      <Toggle
        checked={false}
        onChange={() => {}}
        label="Only notes with links"
        description="Hides the notes nothing connects to."
      />,
    );

    const toggle = container.querySelector('[role="switch"]')!;
    const describedBy = toggle.getAttribute('aria-describedby');
    expect(byId(container, describedBy!)?.textContent).toBe('Hides the notes nothing connects to.');
    // The description must not also be the name, or the switch announces the
    // whole sentence every time it is reached.
    expect(toggle.getAttribute('aria-labelledby')).not.toBe(describedBy);
  });

  it('keeps its name when the text is hidden', () => {
    const container = render(
      <Toggle checked onChange={() => {}} label="Dark mode" hideLabel />,
    );
    expect(container.querySelector('[role="switch"]')?.getAttribute('aria-label')).toBe(
      'Dark mode',
    );
  });
});

describe('Checkbox and Select', () => {
  it('toggles through its label', () => {
    const onChange = vi.fn();
    const container = render(<Checkbox label="Confirm before deleting" onChange={onChange} />);

    click(container.querySelector('label'));
    expect(onChange).toHaveBeenCalled();
  });

  it('renders every option and keeps the label attached', () => {
    const container = render(
      <Select
        label="Theme"
        value="dark"
        onChange={() => {}}
        options={[
          { value: 'system', label: 'System' },
          { value: 'light', label: 'Light' },
          { value: 'dark', label: 'Dark' },
        ]}
      />,
    );

    const select = container.querySelector('select')!;
    expect(select.querySelectorAll('option')).toHaveLength(3);
    expect(container.querySelector('label')?.getAttribute('for')).toBe(select.id);
  });
});

describe('States', () => {
  it('offers the action that would fill an empty panel', () => {
    const onClick = vi.fn();
    const container = render(
      <EmptyState
        icon="file-text"
        title="No notes yet"
        description="Create your first note to start building your knowledge base."
        action={{ label: 'Create note', onClick }}
      />,
    );

    expect(container.textContent).toContain('No notes yet');
    click(container.querySelector('button'));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it('marks a loading region busy without blocking the window', () => {
    const container = render(<LoadingState label="Loading backlinks" rows={3} />);
    const region = container.querySelector('.ie-loading')!;

    expect(region.getAttribute('aria-busy')).toBe('true');
    expect(region.getAttribute('aria-live')).toBe('polite');
    expect(container.querySelectorAll('.ie-loading__row')).toHaveLength(3);
    expect(container.textContent).toContain('Loading backlinks');
  });

  it('states what failed and offers a way out', () => {
    const retry = vi.fn();
    const container = render(
      <ErrorState
        title="Unable to save the note"
        description="The file may have been changed by another application."
        detail="EBUSY: resource busy"
        actions={[{ label: 'Retry', onClick: retry, primary: true }, { label: 'Compare', onClick: () => {} }]}
      />,
    );

    expect(container.querySelector('[role="alert"]')).not.toBeNull();
    expect(container.textContent).toContain('Unable to save the note');
    // Not a code on its own: the detail is there, but under a sentence.
    expect(container.textContent).toContain('The file may have been changed');

    click(container.querySelectorAll('.ie-error-state__actions button')[0]);
    expect(retry).toHaveBeenCalledOnce();
  });
});

describe('Chips', () => {
  it('removes a filter clause', () => {
    const onRemove = vi.fn();
    const container = render(<FilterChip field="tag" value="research" onRemove={onRemove} />);

    expect(container.textContent).toContain('tag:');
    expect(container.textContent).toContain('research');
    click(container.querySelector('.ie-chip__remove'));
    expect(onRemove).toHaveBeenCalledOnce();
  });

  it('shows a tag with its count and follows it when clicked', () => {
    const onClick = vi.fn();
    const container = render(<TagChip name="ai" count={12} onClick={onClick} />);

    expect(container.textContent).toContain('ai');
    expect(container.textContent).toContain('12');
    click(container.querySelector('.ie-chip__main'));
    expect(onClick).toHaveBeenCalledOnce();
  });
});

describe('Navigation', () => {
  it('marks the last breadcrumb segment as where you are', () => {
    const onClick = vi.fn();
    const container = render(
      <Breadcrumb
        segments={[
          { label: 'Projects', onClick },
          { label: 'AI', onClick },
          { label: 'Research.md' },
        ]}
      />,
    );

    const current = container.querySelector('[aria-current="page"]');
    expect(current?.textContent).toBe('Research.md');
    // The place you already are is not a link.
    expect(current?.tagName).toBe('SPAN');

    click(container.querySelector('button'));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it('moves focus with the arrow keys inside a segmented control', () => {
    const container = render(
      <SegmentedControl
        label="View"
        value="edit"
        onChange={() => {}}
        options={[
          { value: 'edit', label: 'Editing' },
          { value: 'read', label: 'Reading' },
        ]}
      />,
    );

    const options = Array.from(container.querySelectorAll<HTMLButtonElement>('[role="radio"]'));
    const first = options[0]!;
    const second = options[1]!;
    // Only the selected option is in the tab order; the rest are reached with
    // the arrow keys, which is what a radio group should do.
    expect(first.tabIndex).toBe(0);
    expect(second.tabIndex).toBe(-1);

    act(() => first.focus());
    press(container.querySelector('[role="radiogroup"]'), 'ArrowRight');
    expect(document.activeElement).toBe(second);

    press(container.querySelector('[role="radiogroup"]'), 'ArrowRight');
    expect(document.activeElement).toBe(first);
  });

  it('selects a rail item and reports which is active', () => {
    const onSelect = vi.fn();
    const container = render(
      <Rail
        side="left"
        activeId="files"
        onSelect={onSelect}
        items={[
          { id: 'files', label: 'Files', icon: 'folder' },
          { id: 'search', label: 'Search', icon: 'search', badge: 3 },
        ]}
      />,
    );

    const tabs = container.querySelectorAll('[role="tab"]');
    expect(tabs[0]!.getAttribute('aria-selected')).toBe('true');
    expect(tabs[1]!.getAttribute('aria-label')).toBe('Search');
    expect(container.querySelector('.ie-rail__badge')?.textContent).toBe('3');

    click(tabs[1]!);
    expect(onSelect).toHaveBeenCalledWith('search');
  });
});

describe('Panel', () => {
  it('names the region and keeps the header out of the scrolling body', () => {
    const container = render(
      <Panel title="Backlinks" icon="corner-down-left" badge={4}>
        <p>content</p>
      </Panel>,
    );

    expect(container.querySelector('section')?.getAttribute('aria-label')).toBe('Backlinks');
    expect(container.querySelector('.ie-panel2__badge')?.textContent).toBe('4');
  });

  it('folds a section and says whether it is open', () => {
    const container = render(
      <CollapsibleSection title="Outline">
        <p>headings</p>
      </CollapsibleSection>,
    );

    const toggle = container.querySelector('[aria-expanded]')!;
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    const body = byId(container, toggle.getAttribute('aria-controls')!)!;
    expect(body.hasAttribute('hidden')).toBe(false);

    click(toggle);
    expect(container.querySelector('[aria-expanded]')?.getAttribute('aria-expanded')).toBe('false');
    expect(byId(container, toggle.getAttribute('aria-controls')!)?.hasAttribute('hidden')).toBe(true);
  });
});

describe('menu()', () => {
  it('drops the separators that are left dangling when an item is omitted', () => {
    const entries = menu(
      { id: 'open', label: 'Open', run: () => {} },
      separator('s1'),
      false, // a command that does not apply right now
      separator('s2'),
      { id: 'copy', label: 'Copy', run: () => {} },
      separator('s3'),
    );

    expect(entries.map((entry) => entry.id)).toEqual(['open', 's1', 'copy']);
  });

  it('keeps a menu that needs no cleaning intact', () => {
    const entries = menu(
      { id: 'a', label: 'A', run: () => {} },
      separator('s'),
      { id: 'b', label: 'B', run: () => {} },
    );
    expect(entries).toHaveLength(3);
  });
});
