/**
 * A resizable sidebar holding several panels.
 *
 * The width is dragged and persisted with the workspace, so the layout the
 * user arranges survives a restart.
 */

import type { ReactNode } from 'react';
import { useCallback, useRef } from 'react';

export interface SidebarPanel {
  id: string;
  label: string;
  icon: string;
  render: () => ReactNode;
}

export interface SidebarProps {
  side: 'left' | 'right';
  width: number;
  activePanel: string;
  panels: SidebarPanel[];
  onPanelChange: (panel: string) => void;
  onResize: (width: number) => void;
}

const MIN_WIDTH = 180;
const MAX_WIDTH = 640;

export function Sidebar(props: SidebarProps) {
  const element = useRef<HTMLDivElement | null>(null);

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      const startX = event.clientX;
      const startWidth = props.width;

      const move = (moveEvent: PointerEvent) => {
        const delta = props.side === 'left' ? moveEvent.clientX - startX : startX - moveEvent.clientX;
        props.onResize(Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, startWidth + delta)));
      };
      const up = () => {
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', up);
      };
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', up);
    },
    [props],
  );

  const active = props.panels.find((panel) => panel.id === props.activePanel) ?? props.panels[0];

  return (
    <aside
      className={`ie-sidebar ie-sidebar--${props.side} ie-chrome`}
      style={{ width: props.width }}
      ref={element}
    >
      {props.side === 'right' ? (
        <div
          className="ie-sidebar__handle"
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize sidebar"
          tabIndex={0}
          onPointerDown={onPointerDown}
          onKeyDown={(event) => {
            if (event.key === 'ArrowLeft') props.onResize(Math.min(MAX_WIDTH, props.width + 16));
            if (event.key === 'ArrowRight') props.onResize(Math.max(MIN_WIDTH, props.width - 16));
          }}
        />
      ) : null}

      <div className="ie-sidebar__inner">
        <nav className="ie-sidebar__tabs" role="tablist" aria-label={`${props.side} sidebar`}>
          {props.panels.map((panel) => (
            <button
              key={panel.id}
              type="button"
              role="tab"
              aria-selected={panel.id === active?.id}
              className={`ie-sidebar__tab${panel.id === active?.id ? ' is-active' : ''}`}
              title={panel.label}
              onClick={() => props.onPanelChange(panel.id)}
            >
              <span aria-hidden="true">{panel.icon}</span>
              <span className="sr-only">{panel.label}</span>
            </button>
          ))}
        </nav>
        <div className="ie-sidebar__content">{active?.render()}</div>
      </div>

      {props.side === 'left' ? (
        <div
          className="ie-sidebar__handle"
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize sidebar"
          tabIndex={0}
          onPointerDown={onPointerDown}
          onKeyDown={(event) => {
            if (event.key === 'ArrowRight') props.onResize(Math.min(MAX_WIDTH, props.width + 16));
            if (event.key === 'ArrowLeft') props.onResize(Math.max(MIN_WIDTH, props.width - 16));
          }}
        />
      ) : null}
    </aside>
  );
}
