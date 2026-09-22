/**
 * Global key handling.
 *
 * One listener on the window, matching against the command registry. A single
 * listener rather than one per component means a shortcut works from anywhere
 * that is not a text field, and that the hotkey editor's changes take effect
 * immediately without remounting anything.
 */

import { useEffect } from 'react';

import { commands } from '@/commands/registry';
import { matches } from '@/commands/hotkeys';

/** Elements that should receive keystrokes rather than the command system. */
function isTextEntry(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
}

/**
 * Shortcuts that work even while typing.
 *
 * Anything with a modifier is safe: it cannot collide with ordinary text
 * entry. A bare key such as Escape or F2 would, so it is only honoured outside
 * a text field.
 */
function hasModifier(combination: string): boolean {
  return /\b(mod|ctrl|alt|shift|cmd|meta)\b/i.test(combination);
}

export function useHotkeys(overrides: Record<string, string>, enabled = true): void {
  useEffect(() => {
    if (!enabled) return;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;

      const inTextEntry = isTextEntry(event.target);

      for (const command of commands.list()) {
        const combination = overrides[command.id] ?? command.defaultHotkey;
        if (!combination) continue;
        if (inTextEntry && !hasModifier(combination)) continue;
        if (!matches(event, combination)) continue;
        if (command.isAvailable && !command.isAvailable()) continue;

        event.preventDefault();
        event.stopPropagation();
        void commands.execute(command.id);
        return;
      }
    };

    // Capture phase, so an application shortcut wins over a control that would
    // otherwise swallow the key.
    window.addEventListener('keydown', onKeyDown, true);
    return () => window.removeEventListener('keydown', onKeyDown, true);
  }, [overrides, enabled]);
}

/** Warn about two commands bound to the same combination. */
export function findHotkeyConflicts(overrides: Record<string, string>): [string, string][] {
  const byCombination = new Map<string, string[]>();
  for (const command of commands.list()) {
    const combination = overrides[command.id] ?? command.defaultHotkey;
    if (!combination) continue;
    const key = combination.toLowerCase();
    const existing = byCombination.get(key);
    if (existing) existing.push(command.id);
    else byCombination.set(key, [command.id]);
  }

  const conflicts: [string, string][] = [];
  for (const [combination, ids] of byCombination) {
    if (ids.length > 1) {
      for (const id of ids.slice(1)) conflicts.push([combination, id]);
    }
  }
  return conflicts;
}
