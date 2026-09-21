/**
 * Buttons.
 *
 * There is one Button because there was none: the interface had a
 * `.ie-button` class, which meant nothing stopped a control from being a
 * `<div>` with a click handler, or from losing its focus ring, or from
 * looking enabled while doing nothing. A component can enforce those; a class
 * name cannot.
 *
 * `IconButton` takes its label in the type signature rather than hoping for
 * one, because an icon-only control with no accessible name is read out as
 * "button" and nothing else.
 */

import type { ButtonHTMLAttributes, ReactNode, Ref } from 'react';

import { Icon, type IconName } from './icons';

export type ButtonVariant = 'primary' | 'default' | 'quiet' | 'danger';
export type ButtonSize = 'sm' | 'md';

export interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  children?: ReactNode;
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Drawn before the label. */
  icon?: IconName;
  /** Drawn after the label — a chevron on a menu button, say. */
  trailingIcon?: IconName;
  /** Shows a spinner and blocks the click without changing the width. */
  loading?: boolean;
  fullWidth?: boolean;
  ref?: Ref<HTMLButtonElement>;
}

export function Button({
  children,
  variant = 'default',
  size = 'md',
  icon,
  trailingIcon,
  loading = false,
  fullWidth = false,
  className,
  disabled,
  type = 'button',
  ...rest
}: ButtonProps) {
  return (
    <button
      {...rest}
      type={type}
      // A loading button is still focusable — removing it from the tab order
      // mid-interaction would move focus somewhere the user did not ask for.
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      className={[
        'ie-btn',
        `ie-btn--${variant}`,
        `ie-btn--${size}`,
        fullWidth ? 'ie-btn--full' : '',
        loading ? 'is-loading' : '',
        className,
      ]
        .filter(Boolean)
        .join(' ')}
    >
      {loading ? (
        <Icon name="spinner" className="ie-spin" />
      ) : icon ? (
        <Icon name={icon} />
      ) : null}
      {children ? <span className="ie-btn__label">{children}</span> : null}
      {trailingIcon ? <Icon name={trailingIcon} className="ie-btn__trailing" /> : null}
    </button>
  );
}

export interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  icon: IconName;
  /** Required: this is the control's only name. */
  label: string;
  size?: ButtonSize;
  variant?: 'default' | 'quiet' | 'danger';
  /** Renders as a toggle, reporting its state to assistive technology. */
  pressed?: boolean;
  ref?: Ref<HTMLButtonElement>;
}

export function IconButton({
  icon,
  label,
  size = 'md',
  variant = 'quiet',
  pressed,
  className,
  type = 'button',
  ...rest
}: IconButtonProps) {
  return (
    <button
      {...rest}
      type={type}
      // The title gives a mouse user the same text the screen reader gets,
      // which is cheaper than a tooltip for a control in a dense strip.
      title={rest.title ?? label}
      aria-label={label}
      aria-pressed={pressed}
      className={[
        'ie-icon-btn',
        `ie-icon-btn--${variant}`,
        `ie-icon-btn--${size}`,
        className,
      ]
        .filter(Boolean)
        .join(' ')}
    >
      <Icon name={icon} />
    </button>
  );
}

/** A row of buttons with consistent spacing, right-aligned by default. */
export function ButtonGroup({
  children,
  align = 'end',
}: {
  children: ReactNode;
  align?: 'start' | 'center' | 'end' | 'between';
}) {
  return <div className={`ie-btn-group ie-btn-group--${align}`}>{children}</div>;
}
