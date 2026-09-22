/**
 * Menus attached to a button.
 *
 * A dropdown and a context menu are the same list of choices arriving by
 * different routes, so this opens the one menu surface the application
 * already has, anchored under the button instead of at the pointer. One
 * implementation means one set of dismissal rules, one keyboard model and one
 * place where "off the bottom of the screen" is handled.
 */

import { useCallback } from 'react';

import { useContextMenu, type MenuEntry } from '@/components/ContextMenu';

import { Button, IconButton, type ButtonProps } from './Button';
import type { IconName } from './icons';

export type { MenuEntry };

/** Open a menu anchored to an element the user activated. */
export function useAnchoredMenu() {
  const menu = useContextMenu();

  return useCallback(
    (element: HTMLElement | null, entries: MenuEntry[], align: 'start' | 'end' = 'start') => {
      if (!element) return;
      const box = element.getBoundingClientRect();
      menu.open(
        { clientX: align === 'start' ? box.left : box.right, clientY: box.bottom + 4 },
        entries,
      );
    },
    [menu],
  );
}

export interface MenuButtonProps extends Omit<ButtonProps, 'onClick' | 'trailingIcon'> {
  entries: MenuEntry[] | (() => MenuEntry[]);
  align?: 'start' | 'end';
}

/** A button that opens a menu. */
export function MenuButton({ entries, align, children, ...rest }: MenuButtonProps) {
  const openMenu = useAnchoredMenu();

  return (
    <Button
      {...rest}
      trailingIcon="chevron-down"
      aria-haspopup="menu"
      onClick={(event) => openMenu(event.currentTarget, resolve(entries), align)}
    >
      {children}
    </Button>
  );
}

export interface MenuIconButtonProps {
  icon?: IconName;
  label: string;
  entries: MenuEntry[] | (() => MenuEntry[]);
  align?: 'start' | 'end';
  size?: 'sm' | 'md';
  className?: string;
}

/** The "…" button that opens a menu of actions for a row. */
export function MenuIconButton({
  icon = 'more',
  label,
  entries,
  align,
  size,
  className,
}: MenuIconButtonProps) {
  const openMenu = useAnchoredMenu();

  return (
    <IconButton
      icon={icon}
      label={label}
      size={size}
      className={className}
      aria-haspopup="menu"
      onClick={(event) => {
        event.stopPropagation();
        openMenu(event.currentTarget, resolve(entries), align);
      }}
    />
  );
}

function resolve(entries: MenuEntry[] | (() => MenuEntry[])): MenuEntry[] {
  return typeof entries === 'function' ? entries() : entries;
}

/** A separator, named so a menu definition reads as a list. */
export function separator(id: string): MenuEntry {
  return { id, separator: true };
}

/** Build a menu, dropping anything that is not applicable right now. */
export function menu(...entries: (MenuEntry | false | null | undefined)[]): MenuEntry[] {
  const kept = entries.filter((entry): entry is MenuEntry => Boolean(entry));
  // Two separators in a row, or one at either end, are what remain when an
  // item is dropped; removing them here saves every call site remembering.
  return kept.filter((entry, index) => {
    if (!('separator' in entry)) return true;
    const previous = kept[index - 1];
    const next = kept[index + 1];
    return previous !== undefined && next !== undefined && !('separator' in previous);
  });
}
