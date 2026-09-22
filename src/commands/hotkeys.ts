/**
 * Key combinations.
 *
 * Combinations are written platform-neutrally — `Mod+P` rather than `Ctrl+P` —
 * and `Mod` resolves to Ctrl on Windows and Linux, and would resolve to Cmd on
 * macOS. That is the whole reason for the indirection: the brief requires the
 * shortcut system not to be tied to one platform, and rewriting every binding
 * later is exactly the technical debt worth avoiding now.
 */

export interface ParsedCombination {
  key: string;
  mod: boolean;
  shift: boolean;
  alt: boolean;
  /** Ctrl specifically, as distinct from `Mod`. */
  ctrl: boolean;
}

/** True on a platform whose primary modifier is Cmd. */
export function usesCommandKey(): boolean {
  if (typeof navigator === 'undefined') return false;
  return /Mac|iPhone|iPad/.test(navigator.platform ?? navigator.userAgent ?? '');
}

export function parseCombination(combination: string): ParsedCombination | null {
  const parts = combination
    .split('+')
    .map((part) => part.trim())
    .filter(Boolean);
  if (parts.length === 0) return null;

  const parsed: ParsedCombination = {
    key: '',
    mod: false,
    shift: false,
    alt: false,
    ctrl: false,
  };

  for (const part of parts) {
    switch (part.toLowerCase()) {
      case 'mod':
      case 'cmdorctrl':
        parsed.mod = true;
        break;
      case 'ctrl':
      case 'control':
        parsed.ctrl = true;
        break;
      case 'shift':
        parsed.shift = true;
        break;
      case 'alt':
      case 'option':
        parsed.alt = true;
        break;
      case 'cmd':
      case 'meta':
      case 'super':
        parsed.mod = true;
        break;
      default:
        parsed.key = normaliseKey(part);
    }
  }

  return parsed.key ? parsed : null;
}

function normaliseKey(key: string): string {
  const lower = key.toLowerCase();
  const aliases: Record<string, string> = {
    esc: 'escape',
    del: 'delete',
    ins: 'insert',
    return: 'enter',
    space: ' ',
    spacebar: ' ',
    plus: '+',
    up: 'arrowup',
    down: 'arrowdown',
    left: 'arrowleft',
    right: 'arrowright',
  };
  return aliases[lower] ?? lower;
}

/** Does this event match the combination? */
export function matches(event: KeyboardEvent, combination: string): boolean {
  const parsed = parseCombination(combination);
  if (!parsed) return false;

  const primary = usesCommandKey() ? event.metaKey : event.ctrlKey;
  if (parsed.mod !== primary) return false;
  // An explicit `Ctrl` means Ctrl even where `Mod` is Cmd.
  if (parsed.ctrl && !event.ctrlKey) return false;
  if (parsed.shift !== event.shiftKey) return false;
  if (parsed.alt !== event.altKey) return false;

  const key = normaliseKey(event.key);
  // Compare `event.code` too, so a binding on a digit still fires on a layout
  // where Shift changes the character.
  const code = event.code.replace(/^(Key|Digit)/, '').toLowerCase();
  return key === parsed.key || code === parsed.key;
}

/** Render a combination the way it should appear in a menu. */
export function formatCombination(combination: string): string {
  const parsed = parseCombination(combination);
  if (!parsed) return combination;

  const mac = usesCommandKey();
  const parts: string[] = [];
  if (parsed.mod) parts.push(mac ? '⌘' : 'Ctrl');
  if (parsed.ctrl && !parsed.mod) parts.push('Ctrl');
  if (parsed.alt) parts.push(mac ? '⌥' : 'Alt');
  if (parsed.shift) parts.push(mac ? '⇧' : 'Shift');

  const labels: Record<string, string> = {
    arrowup: '↑',
    arrowdown: '↓',
    arrowleft: '←',
    arrowright: '→',
    enter: '↵',
    escape: 'Esc',
    ' ': 'Space',
    backspace: '⌫',
    delete: 'Del',
  };
  parts.push(labels[parsed.key] ?? parsed.key.toUpperCase());

  return parts.join(mac ? '' : '+');
}

/** Turn a keydown into the combination string it represents, for the hotkey editor. */
export function combinationFromEvent(event: KeyboardEvent): string | null {
  const key = normaliseKey(event.key);
  if (['control', 'shift', 'alt', 'meta', 'os'].includes(key)) return null;

  const parts: string[] = [];
  if (usesCommandKey() ? event.metaKey : event.ctrlKey) parts.push('Mod');
  if (event.altKey) parts.push('Alt');
  if (event.shiftKey) parts.push('Shift');
  parts.push(key === ' ' ? 'Space' : key);
  return parts.join('+');
}

/** Two combinations that would fire on the same keypress. */
export function conflicts(a: string, b: string): boolean {
  const first = parseCombination(a);
  const second = parseCombination(b);
  if (!first || !second) return false;
  return (
    first.key === second.key &&
    first.mod === second.mod &&
    first.ctrl === second.ctrl &&
    first.shift === second.shift &&
    first.alt === second.alt
  );
}
