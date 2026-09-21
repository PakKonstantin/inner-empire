/**
 * Navigation primitives: breadcrumbs, a segmented control, and a rail.
 *
 * All three are lists of choices, and all three are keyboard-operable by the
 * same rule: one stop in the tab order for the group, arrow keys within it.
 * That is what "roving tabindex" means, and doing it by hand in three places
 * is how one of them ends up without it.
 */

import type { ReactNode } from 'react';
import { useCallback, useRef } from 'react';

import { Icon, type IconName } from './icons';
import { Tooltip } from './Tooltip';

/** Move focus with the arrow keys inside a group of controls. */
function useRovingFocus(orientation: 'horizontal' | 'vertical') {
  const container = useRef<HTMLDivElement | null>(null);

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      const next = orientation === 'horizontal' ? 'ArrowRight' : 'ArrowDown';
      const previous = orientation === 'horizontal' ? 'ArrowLeft' : 'ArrowUp';
      if (!['Home', 'End', next, previous].includes(event.key)) return;

      const items = Array.from(
        container.current?.querySelectorAll<HTMLElement>('[data-roving]') ?? [],
      ).filter((item) => !item.hasAttribute('disabled'));
      if (items.length === 0) return;

      const current = items.indexOf(document.activeElement as HTMLElement);
      let index = current;
      if (event.key === next) index = (current + 1) % items.length;
      else if (event.key === previous) index = (current - 1 + items.length) % items.length;
      else if (event.key === 'Home') index = 0;
      else if (event.key === 'End') index = items.length - 1;

      event.preventDefault();
      items[index]?.focus();
    },
    [orientation],
  );

  return { container, onKeyDown };
}

export interface BreadcrumbSegment {
  label: string;
  /** Omitted for the last segment, which is where you already are. */
  onClick?: () => void;
  title?: string;
}

export function Breadcrumb({ segments }: { segments: BreadcrumbSegment[] }) {
  if (segments.length === 0) return null;

  return (
    <nav className="ie-breadcrumb" aria-label="Location">
      <ol>
        {segments.map((segment, index) => {
          const last = index === segments.length - 1;
          return (
            <li key={`${segment.label}-${index}`}>
              {segment.onClick && !last ? (
                <button
                  type="button"
                  className="ie-breadcrumb__link"
                  title={segment.title ?? segment.label}
                  onClick={segment.onClick}
                >
                  {segment.label}
                </button>
              ) : (
                <span
                  className="ie-breadcrumb__current"
                  title={segment.title ?? segment.label}
                  aria-current={last ? 'page' : undefined}
                >
                  {segment.label}
                </span>
              )}
              {last ? null : (
                <Icon name="chevron-right" className="ie-breadcrumb__separator" />
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  icon?: IconName;
  /** Shown in the tooltip. */
  shortcut?: string;
}

export interface SegmentedControlProps<T extends string> {
  options: SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
  label: string;
  /** Icons only, with the label in a tooltip. */
  iconOnly?: boolean;
  size?: 'sm' | 'md';
}

/** Editing / Reading, and anywhere else a small set of exclusive choices sits. */
export function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
  label,
  iconOnly = false,
  size = 'md',
}: SegmentedControlProps<T>) {
  const roving = useRovingFocus('horizontal');

  return (
    <div
      className={`ie-segmented ie-segmented--${size}`}
      role="radiogroup"
      aria-label={label}
      ref={roving.container}
      onKeyDown={roving.onKeyDown}
    >
      {options.map((option) => {
        const selected = option.value === value;
        const button = (
          <button
            key={option.value}
            type="button"
            role="radio"
            data-roving=""
            aria-checked={selected}
            aria-label={iconOnly ? option.label : undefined}
            tabIndex={selected ? 0 : -1}
            className={`ie-segmented__option${selected ? ' is-selected' : ''}`}
            onClick={() => onChange(option.value)}
          >
            {option.icon ? <Icon name={option.icon} /> : null}
            {iconOnly ? null : <span>{option.label}</span>}
          </button>
        );

        return iconOnly ? (
          <Tooltip key={option.value} content={option.label} shortcut={option.shortcut}>
            {button}
          </Tooltip>
        ) : (
          button
        );
      })}
    </div>
  );
}

export interface RailItem {
  id: string;
  label: string;
  icon: IconName;
  shortcut?: string;
  badge?: number;
}

export interface RailProps {
  items: RailItem[];
  activeId: string | null;
  onSelect: (id: string) => void;
  side: 'left' | 'right';
  /** Drawn at the bottom of the rail, below the flexible gap. */
  footer?: ReactNode;
}

/**
 * The icon column beside a sidebar.
 *
 * Selecting the active item again collapses the sidebar, which is the
 * behaviour people expect from a rail and saves a separate toggle.
 */
export function Rail({ items, activeId, onSelect, side, footer }: RailProps) {
  const roving = useRovingFocus('vertical');

  return (
    <div
      className={`ie-rail ie-rail--${side}`}
      role="tablist"
      aria-orientation="vertical"
      aria-label={`${side === 'left' ? 'Left' : 'Right'} sidebar sections`}
      ref={roving.container}
      onKeyDown={roving.onKeyDown}
    >
      {items.map((item) => {
        const selected = item.id === activeId;
        return (
          <Tooltip
            key={item.id}
            content={item.label}
            shortcut={item.shortcut}
            placement={side === 'left' ? 'right' : 'left'}
          >
            <button
              type="button"
              role="tab"
              data-roving=""
              aria-selected={selected}
              aria-label={item.label}
              tabIndex={selected ? 0 : -1}
              className={`ie-rail__item${selected ? ' is-active' : ''}`}
              onClick={() => onSelect(item.id)}
            >
              <Icon name={item.icon} />
              {item.badge ? <span className="ie-rail__badge">{item.badge}</span> : null}
            </button>
          </Tooltip>
        );
      })}
      {footer ? <div className="ie-rail__footer">{footer}</div> : null}
    </div>
  );
}
