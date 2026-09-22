/**
 * A tooltip.
 *
 * Deliberately supplementary: it repeats the accessible name a control
 * already has and adds a shortcut, so nothing is lost when it never appears —
 * on a touchpad, for a keyboard user who moves too fast, for a screen reader
 * that reads the label instead.
 *
 * It shows on hover after a delay and on focus at once, because a keyboard
 * user has already committed to the control by the time focus lands.
 */

import type { ReactElement, ReactNode } from 'react';
import { cloneElement, useCallback, useEffect, useId, useRef, useState } from 'react';

export interface TooltipProps {
  content: ReactNode;
  /** A keyboard hint drawn to the right of the text, e.g. `Ctrl+P`. */
  shortcut?: string;
  placement?: 'top' | 'bottom' | 'left' | 'right';
  delayMs?: number;
  children: ReactElement<{
    'aria-describedby'?: string;
    onPointerEnter?: (event: React.PointerEvent) => void;
    onPointerLeave?: (event: React.PointerEvent) => void;
    onFocus?: (event: React.FocusEvent) => void;
    onBlur?: (event: React.FocusEvent) => void;
  }>;
}

export function Tooltip({
  content,
  shortcut,
  placement = 'bottom',
  delayMs = 400,
  children,
}: TooltipProps) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<{ top: number; left: number } | null>(null);
  const anchor = useRef<HTMLElement | null>(null);
  const timer = useRef<number | null>(null);
  const id = useId();

  const cancel = useCallback(() => {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
  }, []);

  const show = useCallback(
    (immediate: boolean) => {
      cancel();
      const reveal = () => {
        const element = anchor.current;
        if (!element) return;
        const box = element.getBoundingClientRect();
        const gap = 8;
        const spot = {
          top: { top: box.top - gap, left: box.left + box.width / 2 },
          bottom: { top: box.bottom + gap, left: box.left + box.width / 2 },
          left: { top: box.top + box.height / 2, left: box.left - gap },
          right: { top: box.top + box.height / 2, left: box.right + gap },
        }[placement];
        setPosition(spot);
        setOpen(true);
      };
      if (immediate) reveal();
      else timer.current = window.setTimeout(reveal, delayMs);
    },
    [cancel, delayMs, placement],
  );

  const hide = useCallback(() => {
    cancel();
    setOpen(false);
  }, [cancel]);

  useEffect(() => cancel, [cancel]);

  // Escape dismisses it, which matters when a tooltip covers what the user is
  // trying to read.
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') hide();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open, hide]);

  const child = cloneElement(children, {
    'aria-describedby': open ? id : undefined,
    ref: (node: HTMLElement | null) => {
      anchor.current = node;
    },
    onPointerEnter: (event: React.PointerEvent) => {
      // Touch has no hover; showing on a tap would fight the control.
      if (event.pointerType !== 'touch') show(false);
      children.props.onPointerEnter?.(event);
    },
    onPointerLeave: (event: React.PointerEvent) => {
      hide();
      children.props.onPointerLeave?.(event);
    },
    onFocus: (event: React.FocusEvent) => {
      show(true);
      children.props.onFocus?.(event);
    },
    onBlur: (event: React.FocusEvent) => {
      hide();
      children.props.onBlur?.(event);
    },
  } as Record<string, unknown>);

  return (
    <>
      {child}
      {open && position ? (
        <div
          role="tooltip"
          id={id}
          className={`ie-tooltip ie-tooltip--${placement}`}
          style={{ top: position.top, left: position.left }}
        >
          {content}
          {shortcut ? <kbd className="ie-tooltip__key">{shortcut}</kbd> : null}
        </div>
      ) : null}
    </>
  );
}
