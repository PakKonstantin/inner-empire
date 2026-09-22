/**
 * The three states every panel has and most applications forget.
 *
 * *Empty* is not a blank rectangle: it says what would be here and offers the
 * action that would put something there. *Loading* does not block the window;
 * it shows the shape of what is coming, in the region that is waiting.
 * *Failed* says what failed in a sentence a person can act on, and offers the
 * act — not a hexadecimal code.
 */

import type { ReactNode } from 'react';

import { Button } from './Button';
import { Icon, type IconName } from './icons';

export interface EmptyStateProps {
  icon?: IconName;
  title: string;
  /** One or two sentences. Longer than that and nobody reads it. */
  description?: ReactNode;
  action?: { label: string; onClick: () => void; icon?: IconName };
  secondaryAction?: { label: string; onClick: () => void };
  /** Tighter spacing, for a sidebar panel rather than the whole window. */
  compact?: boolean;
}

export function EmptyState({
  icon,
  title,
  description,
  action,
  secondaryAction,
  compact = false,
}: EmptyStateProps) {
  return (
    <div className={`ie-empty-state${compact ? ' ie-empty-state--compact' : ''}`}>
      {icon ? (
        <span className="ie-empty-state__icon" aria-hidden="true">
          <Icon name={icon} />
        </span>
      ) : null}
      <p className="ie-empty-state__title">{title}</p>
      {description ? <p className="ie-empty-state__body">{description}</p> : null}
      {action || secondaryAction ? (
        <div className="ie-empty-state__actions">
          {action ? (
            <Button variant="primary" size="sm" icon={action.icon} onClick={action.onClick}>
              {action.label}
            </Button>
          ) : null}
          {secondaryAction ? (
            <Button variant="quiet" size="sm" onClick={secondaryAction.onClick}>
              {secondaryAction.label}
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

export interface SkeletonProps {
  /** CSS width: `100%`, `12ch`, `60%`. */
  width?: string;
  height?: number;
  /** Rounded like a line of text rather than a block. */
  text?: boolean;
  className?: string;
}

export function Skeleton({ width = '100%', height, text = false, className }: SkeletonProps) {
  return (
    <span
      className={['ie-skeleton', text ? 'ie-skeleton--text' : '', className]
        .filter(Boolean)
        .join(' ')}
      style={{ width, height: height ? `${height}px` : undefined }}
      aria-hidden="true"
    />
  );
}

export interface LoadingStateProps {
  /** What is loading, for the screen reader. */
  label: string;
  /** How many placeholder rows to draw. */
  rows?: number;
  /** A row that looks like a list entry rather than a paragraph. */
  variant?: 'list' | 'text';
}

/**
 * A region that is waiting.
 *
 * `aria-busy` with a polite live region tells a screen reader that this part
 * is loading without interrupting whatever the user is doing elsewhere — the
 * whole point of not blocking the window for one panel.
 */
export function LoadingState({ label, rows = 4, variant = 'list' }: LoadingStateProps) {
  return (
    <div className="ie-loading" aria-busy="true" aria-live="polite">
      <span className="sr-only">{label}</span>
      {Array.from({ length: rows }, (_, index) => (
        <div className="ie-loading__row" key={index}>
          {variant === 'list' ? <Skeleton width="16px" height={16} /> : null}
          <Skeleton text width={`${90 - (index % 3) * 18}%`} />
        </div>
      ))}
    </div>
  );
}

export interface ErrorStateProps {
  title: string;
  /** What went wrong, in a sentence. Never a bare code. */
  description?: ReactNode;
  /** The underlying message, shown small for a bug report. */
  detail?: string;
  actions?: { label: string; onClick: () => void; primary?: boolean }[];
  compact?: boolean;
}

export function ErrorState({
  title,
  description,
  detail,
  actions = [],
  compact = false,
}: ErrorStateProps) {
  return (
    <div className={`ie-error-state${compact ? ' ie-error-state--compact' : ''}`} role="alert">
      <span className="ie-error-state__icon" aria-hidden="true">
        <Icon name="warning" />
      </span>
      <div className="ie-error-state__body">
        <p className="ie-error-state__title">{title}</p>
        {description ? <p className="ie-error-state__description">{description}</p> : null}
        {detail ? <p className="ie-error-state__detail">{detail}</p> : null}
        {actions.length > 0 ? (
          <div className="ie-error-state__actions">
            {actions.map((action) => (
              <Button
                key={action.label}
                size="sm"
                variant={action.primary ? 'primary' : 'default'}
                onClick={action.onClick}
              >
                {action.label}
              </Button>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

export type BadgeTone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger';

export function Badge({
  children,
  tone = 'neutral',
  title,
}: {
  children: ReactNode;
  tone?: BadgeTone;
  title?: string;
}) {
  return (
    <span className={`ie-badge ie-badge--${tone}`} title={title}>
      {children}
    </span>
  );
}

export interface TagChipProps {
  /** Without the leading hash; it is added for display. */
  name: string;
  count?: number;
  onClick?: () => void;
  onRemove?: () => void;
}

/** A tag, as a chip. Clickable when it leads somewhere, static when it does not. */
export function TagChip({ name, count, onClick, onRemove }: TagChipProps) {
  const body = (
    <>
      <span className="ie-chip__hash" aria-hidden="true">
        #
      </span>
      {name}
      {count !== undefined ? <span className="ie-chip__count">{count}</span> : null}
    </>
  );

  return (
    <span className="ie-chip ie-chip--tag">
      {onClick ? (
        <button type="button" className="ie-chip__main" onClick={onClick}>
          {body}
        </button>
      ) : (
        <span className="ie-chip__main">{body}</span>
      )}
      {onRemove ? (
        <button
          type="button"
          className="ie-chip__remove"
          aria-label={`Remove the tag ${name}`}
          onClick={onRemove}
        >
          <Icon name="close" />
        </button>
      ) : null}
    </span>
  );
}

export interface FilterChipProps {
  /** `tag`, `path`, `ext` — the clause's field, when it has one to show. */
  field?: string;
  value: string;
  /** A mark for the kind of clause, so the shape is readable at a glance. */
  icon?: IconName;
  /** Struck through and dimmed, for a clause that excludes rather than includes. */
  negated?: boolean;
  onRemove: () => void;
}

/**
 * One clause of a search query, shown as a chip.
 *
 * The point is that a person should be able to see *why* a result is in the
 * list. A chip per clause makes the query legible and removable without
 * editing a string by hand.
 */
export function FilterChip({ field, value, icon, negated, onRemove }: FilterChipProps) {
  const described = field ? `${field}: ${value}` : value;
  return (
    <span className={`ie-chip ie-chip--filter${negated ? ' is-negated' : ''}`}>
      {icon ? <Icon name={icon} size={13} className="ie-chip__icon" /> : null}
      {field ? <span className="ie-chip__field">{field}:</span> : null}
      <span className="ie-chip__value">{value}</span>
      <button
        type="button"
        className="ie-chip__remove"
        aria-label={`Remove ${described}`}
        onClick={onRemove}
      >
        <Icon name="close" />
      </button>
    </span>
  );
}
