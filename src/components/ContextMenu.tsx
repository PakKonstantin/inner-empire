/**
 * Context menus.
 *
 * One menu exists at a time, opened through a hook rather than mounted by each
 * component, so a right-click in the explorer and one in the editor cannot
 * produce two overlapping menus.
 */

import type { ReactNode } from 'react';
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';

export interface MenuItem {
  id: string;
  label: string;
  hint?: string;
  danger?: boolean;
  disabled?: boolean;
  run: () => void | Promise<void>;
}

export type MenuEntry = MenuItem | { id: string; separator: true };

function isSeparator(entry: MenuEntry): entry is { id: string; separator: true } {
  return 'separator' in entry;
}

interface MenuState {
  x: number;
  y: number;
  entries: MenuEntry[];
}

interface ContextMenuApi {
  open: (event: { clientX: number; clientY: number }, entries: MenuEntry[]) => void;
  close: () => void;
}

const ContextMenuContext = createContext<ContextMenuApi | null>(null);

export function useContextMenu(): ContextMenuApi {
  const api = useContext(ContextMenuContext);
  if (!api) throw new Error('useContextMenu must be used inside ContextMenuProvider');
  return api;
}

export function ContextMenuProvider({ children }: { children: ReactNode }) {
  const [menu, setMenu] = useState<MenuState | null>(null);
  const surface = useRef<HTMLDivElement | null>(null);
  // Where focus was before the menu took it, so closing puts it back on the
  // row the user was standing on rather than at the top of the document.
  const returnFocusTo = useRef<HTMLElement | null>(null);

  const api = useMemo<ContextMenuApi>(
    () => ({
      open: (event, entries) => {
        if (entries.length === 0) return;
        returnFocusTo.current =
          document.activeElement instanceof HTMLElement ? document.activeElement : null;
        setMenu({ x: event.clientX, y: event.clientY, entries });
      },
      close: () => {
        setMenu(null);
        returnFocusTo.current?.focus();
        returnFocusTo.current = null;
      },
    }),
    [],
  );

  useEffect(() => {
    if (!menu) return;
    const dismiss = (event: Event) => {
      // A press inside the menu is the user choosing an item. Dismissing here
      // would unmount the button before its click handler ran, so the item
      // would light up and do nothing.
      if (event.target instanceof Node && surface.current?.contains(event.target)) return;
      api.close();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') api.close();
    };
    // Capture phase, so a press that also does something else still closes the
    // menu first.
    window.addEventListener('pointerdown', dismiss, true);
    window.addEventListener('resize', dismiss);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('pointerdown', dismiss, true);
      window.removeEventListener('resize', dismiss);
      window.removeEventListener('keydown', onKey);
    };
  }, [menu, api]);

  return (
    <ContextMenuContext.Provider value={api}>
      {children}
      {menu ? <ContextMenuSurface menu={menu} onClose={api.close} surfaceRef={surface} /> : null}
    </ContextMenuContext.Provider>
  );
}

function ContextMenuSurface({
  menu,
  onClose,
  surfaceRef,
}: {
  menu: MenuState;
  onClose: () => void;
  surfaceRef: React.MutableRefObject<HTMLDivElement | null>;
}) {
  const [position, setPosition] = useState({ left: menu.x, top: menu.y });
  const measure = useCallback(
    (element: HTMLDivElement | null) => {
    surfaceRef.current = element;
    if (!element) return;
    // Flip the menu when it would run off the edge, so an item near the bottom
    // of the window is still reachable.
    const rect = element.getBoundingClientRect();
    const left = rect.right > window.innerWidth ? Math.max(4, window.innerWidth - rect.width - 4) : rect.left;
    const top = rect.bottom > window.innerHeight ? Math.max(4, window.innerHeight - rect.height - 4) : rect.top;
      setPosition({ left, top });

      // Focus the first item, so the arrow keys work without a click first.
      element.querySelector<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')?.focus();
    },
    [surfaceRef],
  );

  /**
   * Keyboard navigation inside the menu.
   *
   * A menu that can only be driven by the mouse is a menu half the brief's
   * users cannot reach: the context menu is opened by Shift+F10 and the menu
   * key as well as by right-clicking, and both leave the hand on the keyboard.
   */
  const onKeyDown = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    const items = Array.from(
      event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)'),
    );
    if (items.length === 0) return;
    const current = items.indexOf(document.activeElement as HTMLButtonElement);

    let next: number | null = null;
    if (event.key === 'ArrowDown') next = (current + 1) % items.length;
    else if (event.key === 'ArrowUp') next = (current - 1 + items.length) % items.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = items.length - 1;
    if (next === null) return;

    event.preventDefault();
    items[next]?.focus();
  }, []);

  return (
    <div
      className="ie-context-menu"
      role="menu"
      ref={measure}
      tabIndex={-1}
      style={{ left: position.left, top: position.top }}
      onKeyDown={onKeyDown}
      onPointerDown={(event) => event.stopPropagation()}
    >
      {menu.entries.map((entry) =>
        isSeparator(entry) ? (
          <div key={entry.id} className="ie-context-menu__separator" role="separator" />
        ) : (
          <button
            key={entry.id}
            type="button"
            role="menuitem"
            className={`ie-context-menu__item${entry.danger ? ' ie-context-menu__item--danger' : ''}`}
            disabled={entry.disabled}
            onClick={() => {
              onClose();
              void entry.run();
            }}
          >
            <span>{entry.label}</span>
            {entry.hint ? <span className="ie-context-menu__hint">{entry.hint}</span> : null}
          </button>
        ),
      )}
    </div>
  );
}
