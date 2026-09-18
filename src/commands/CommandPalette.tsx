/**
 * The command palette and the quick switcher.
 *
 * One component in two modes, because they are the same interaction over
 * different lists: type to filter, arrow to choose, Enter to run. Keeping them
 * together means the keyboard handling exists once.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { api } from '@/services/api';
import type { FileMatch, VaultPath } from '@/types/domain';

import { commands, type Command } from './registry';
import { formatCombination } from './hotkeys';

export type PaletteMode = 'commands' | 'files';

export interface CommandPaletteProps {
  mode: PaletteMode;
  hotkeys: Record<string, string>;
  onClose: () => void;
  onOpenFile: (path: VaultPath, options?: { newPane?: boolean }) => void;
}

export function CommandPalette({ mode, hotkeys, onClose, onOpenFile }: CommandPaletteProps) {
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState(0);
  const [files, setFiles] = useState<FileMatch[]>([]);
  const input = useRef<HTMLInputElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    input.current?.focus();
  }, [mode]);

  // Fuzzy file matching happens in the backend, over the index, so the list
  // reflects the whole vault without the palette holding any of it.
  useEffect(() => {
    if (mode !== 'files') return;
    let cancelled = false;
    api
      .quickSwitch(query, 60)
      .then((matches) => {
        if (!cancelled) setFiles(matches);
      })
      .catch(() => {
        if (!cancelled) setFiles([]);
      });
    return () => {
      cancelled = true;
    };
  }, [mode, query]);

  const matchingCommands = useMemo(() => {
    if (mode !== 'commands') return [];
    const available = commands.available();
    const needle = query.trim().toLowerCase();
    if (!needle) return available;
    return available
      .map((command) => ({ command, score: fuzzyScore(needle, command) }))
      .filter((entry) => entry.score > 0)
      .sort((a, b) => b.score - a.score)
      .map((entry) => entry.command);
  }, [mode, query]);

  const itemCount = mode === 'commands' ? matchingCommands.length : files.length;

  useEffect(() => {
    setSelected(0);
  }, [query, mode]);

  useEffect(() => {
    // Keep the highlighted row on screen as the arrows move through the list.
    const element = listRef.current?.querySelector<HTMLElement>('[aria-selected="true"]');
    element?.scrollIntoView({ block: 'nearest' });
  }, [selected]);

  const choose = useCallback(
    (index: number, newPane: boolean) => {
      if (mode === 'commands') {
        const command = matchingCommands[index];
        if (!command) return;
        onClose();
        void commands.execute(command.id);
        return;
      }
      const file = files[index];
      if (!file) return;
      onClose();
      onOpenFile(file.path, { newPane });
    },
    [mode, matchingCommands, files, onClose, onOpenFile],
  );

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      switch (event.key) {
        case 'Escape':
          event.preventDefault();
          onClose();
          break;
        case 'ArrowDown':
          event.preventDefault();
          setSelected((current) => (itemCount === 0 ? 0 : (current + 1) % itemCount));
          break;
        case 'ArrowUp':
          event.preventDefault();
          setSelected((current) => (itemCount === 0 ? 0 : (current - 1 + itemCount) % itemCount));
          break;
        case 'Home':
          event.preventDefault();
          setSelected(0);
          break;
        case 'End':
          event.preventDefault();
          setSelected(Math.max(0, itemCount - 1));
          break;
        case 'Enter':
          event.preventDefault();
          choose(selected, event.ctrlKey || event.metaKey);
          break;
        default:
          break;
      }
    },
    [choose, itemCount, onClose, selected],
  );

  return (
    <div className="ie-modal-backdrop ie-palette-backdrop" onPointerDown={onClose}>
      <div
        className="ie-palette"
        role="dialog"
        aria-modal="true"
        aria-label={mode === 'commands' ? 'Command palette' : 'Open note'}
        onPointerDown={(event) => event.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <input
          ref={input}
          className="ie-palette__input"
          placeholder={mode === 'commands' ? 'Type a command' : 'Type part of a note name'}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          role="combobox"
          aria-expanded="true"
          aria-controls="ie-palette-list"
          aria-activedescendant={`ie-palette-item-${selected}`}
        />

        <div className="ie-palette__list" id="ie-palette-list" role="listbox" ref={listRef}>
          {itemCount === 0 ? (
            <div className="ie-empty">
              {mode === 'commands' ? 'No command matches.' : 'No note matches.'}
            </div>
          ) : mode === 'commands' ? (
            matchingCommands.map((command, index) => (
              <div
                key={command.id}
                id={`ie-palette-item-${index}`}
                role="option"
                aria-selected={index === selected}
                className={`ie-palette__item${index === selected ? ' is-selected' : ''}`}
                onPointerEnter={() => setSelected(index)}
                onClick={() => choose(index, false)}
              >
                <span className="ie-palette__category">{command.category}</span>
                <span className="ie-palette__label">{command.name}</span>
                {hotkeyFor(command, hotkeys) ? (
                  <kbd className="ie-palette__hotkey">
                    {formatCombination(hotkeyFor(command, hotkeys)!)}
                  </kbd>
                ) : null}
              </div>
            ))
          ) : (
            files.map((file, index) => (
              <div
                key={file.path}
                id={`ie-palette-item-${index}`}
                role="option"
                aria-selected={index === selected}
                className={`ie-palette__item${index === selected ? ' is-selected' : ''}`}
                onPointerEnter={() => setSelected(index)}
                onClick={(event) => choose(index, event.ctrlKey || event.metaKey)}
              >
                <span className="ie-palette__label">{highlight(file.title, file)}</span>
                <span className="ie-palette__path">{file.path}</span>
              </div>
            ))
          )}
        </div>

        <div className="ie-palette__footer">
          <span>
            <kbd>↑</kbd>
            <kbd>↓</kbd> to move
          </span>
          <span>
            <kbd>↵</kbd> to {mode === 'commands' ? 'run' : 'open'}
          </span>
          {mode === 'files' ? (
            <span>
              <kbd>Ctrl</kbd>+<kbd>↵</kbd> in a split
            </span>
          ) : null}
          <span>
            <kbd>Esc</kbd> to close
          </span>
        </div>
      </div>
    </div>
  );
}

function hotkeyFor(command: Command, overrides: Record<string, string>): string | undefined {
  return overrides[command.id] ?? command.defaultHotkey;
}

/** Highlight the characters the fuzzy matcher landed on. */
function highlight(title: string, file: FileMatch) {
  if (file.positions.length === 0) return title;
  // Positions index the path; only the tail is the title.
  const offset = file.path.length - title.length;
  const marked = new Set(file.positions.map((position) => position - offset));
  return (
    <>
      {[...title].map((character, index) =>
        marked.has(index) ? (
          <mark key={index}>{character}</mark>
        ) : (
          <span key={index}>{character}</span>
        ),
      )}
    </>
  );
}

/** A small subsequence score, enough to order the command list sensibly. */
function fuzzyScore(needle: string, command: Command): number {
  const haystack = `${command.category} ${command.name}`.toLowerCase();
  if (haystack.includes(needle)) return 100 + (needle.length / haystack.length) * 50;

  let index = 0;
  let score = 0;
  for (const character of needle) {
    const found = haystack.indexOf(character, index);
    if (found === -1) return 0;
    score += found === index ? 2 : 1;
    index = found + 1;
  }
  return score;
}
