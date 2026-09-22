/**
 * A resizable sidebar.
 *
 * It used to own its own icon column. That column is now a `Rail`, rendered
 * beside the sidebar rather than inside it, for a reason that only shows up
 * on a narrow window: the rail has to survive the sidebar collapsing. With
 * the icons inside, collapsing took away the only way to bring the panel
 * back.
 *
 * So this is now just the panel body and the drag handle, and the width it
 * carries is the width the user set — never a width the layout decided on
 * their behalf when the window got small.
 */

import type { ReactNode } from 'react';
import { useCallback } from 'react';

export interface SidebarPanel {
  id: string;
  label: string;
  render: () => ReactNode;
}

export interface SidebarProps {
  side: 'left' | 'right';
  width: number;
  activePanel: string;
  panels: SidebarPanel[];
  onResize: (width: number) => void;
}

const MIN_WIDTH = 180;
const MAX_WIDTH = 640;
const DEFAULT_WIDTH = { left: 260, right: 300 } as const;
const KEYBOARD_STEP = 16;

export function Sidebar(props: SidebarProps) {
  const clamp = (width: number) => Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, width));

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      event.preventDefault();
      const startX = event.clientX;
      const startWidth = props.width;

      const move = (moveEvent: PointerEvent) => {
        const delta =
          props.side === 'left' ? moveEvent.clientX - startX : startX - moveEvent.clientX;
        props.onResize(clamp(startWidth + delta));
      };
      const up = () => {
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', up);
        document.body.classList.remove('is-resizing');
      };
      // While dragging, the cursor belongs to the divider wherever it wanders.
      document.body.classList.add('is-resizing');
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', up);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [props.side, props.width, props.onResize],
  );

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      const grow = props.side === 'left' ? 'ArrowRight' : 'ArrowLeft';
      const shrink = props.side === 'left' ? 'ArrowLeft' : 'ArrowRight';
      if (event.key === grow) props.onResize(clamp(props.width + KEYBOARD_STEP));
      else if (event.key === shrink) props.onResize(clamp(props.width - KEYBOARD_STEP));
      else if (event.key === 'Home') props.onResize(MIN_WIDTH);
      else if (event.key === 'End') props.onResize(MAX_WIDTH);
      else return;
      event.preventDefault();
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [props.side, props.width, props.onResize],
  );

  const active =
    props.panels.find((panel) => panel.id === props.activePanel) ?? props.panels[0] ?? null;

  const handle = (
    <div
      className="ie-sidebar__handle"
      role="separator"
      aria-orientation="vertical"
      aria-label={`Resize the ${props.side} sidebar`}
      aria-valuenow={props.width}
      aria-valuemin={MIN_WIDTH}
      aria-valuemax={MAX_WIDTH}
      tabIndex={0}
      onPointerDown={onPointerDown}
      // A double click restores the width the sidebar started at, which is
      // faster than dragging back to something that looks about right.
      onDoubleClick={() => props.onResize(DEFAULT_WIDTH[props.side])}
      onKeyDown={onKeyDown}
    />
  );

  return (
    <aside
      className={`ie-sidebar ie-sidebar--${props.side} ie-chrome`}
      style={{ width: props.width }}
      aria-label={`${props.side === 'left' ? 'Left' : 'Right'} sidebar`}
    >
      {props.side === 'right' ? handle : null}
      <div className="ie-sidebar__content">{active?.render()}</div>
      {props.side === 'left' ? handle : null}
    </aside>
  );
}
