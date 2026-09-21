/**
 * Form controls.
 *
 * Each one owns the wiring that is easy to forget by hand: a label tied to
 * its control, a description tied by `aria-describedby`, an error that sets
 * `aria-invalid` as well as turning something red. Colour alone is not a
 * message.
 *
 * The ids are generated with `useId`, so a control can appear twice on a page
 * — the same setting in a panel and in the settings window — without two
 * labels pointing at one field.
 */

import type { InputHTMLAttributes, ReactNode, Ref, SelectHTMLAttributes } from 'react';
import { useId } from 'react';

import { Icon } from './icons';

interface FieldShellProps {
  label?: ReactNode;
  description?: ReactNode;
  error?: string | null;
  /** Lay the label beside the control rather than above it. */
  inline?: boolean;
  className?: string;
  children: (ids: { controlId: string; describedBy: string | undefined }) => ReactNode;
}

function FieldShell({ label, description, error, inline, className, children }: FieldShellProps) {
  const base = useId();
  const controlId = `${base}-control`;
  const descriptionId = description ? `${base}-description` : undefined;
  const errorId = error ? `${base}-error` : undefined;
  const describedBy = [descriptionId, errorId].filter(Boolean).join(' ') || undefined;

  return (
    <div
      className={['ie-field', inline ? 'ie-field--inline' : '', error ? 'is-invalid' : '', className]
        .filter(Boolean)
        .join(' ')}
    >
      {label ? (
        <label className="ie-field__label" htmlFor={controlId}>
          {label}
        </label>
      ) : null}
      <div className="ie-field__control">
        {children({ controlId, describedBy })}
        {description ? (
          <p className="ie-field__description" id={descriptionId}>
            {description}
          </p>
        ) : null}
        {error ? (
          <p className="ie-field__error" id={errorId} role="alert">
            <Icon name="error" />
            {error}
          </p>
        ) : null}
      </div>
    </div>
  );
}

export interface InputProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, 'id' | 'size'> {
  label?: ReactNode;
  description?: ReactNode;
  error?: string | null;
  inline?: boolean;
  fieldClassName?: string;
  ref?: Ref<HTMLInputElement>;
}

export function Input({
  label,
  description,
  error,
  inline,
  fieldClassName,
  className,
  ...rest
}: InputProps) {
  return (
    <FieldShell
      label={label}
      description={description}
      error={error}
      inline={inline}
      className={fieldClassName}
    >
      {({ controlId, describedBy }) => (
        <input
          {...rest}
          id={controlId}
          aria-describedby={describedBy}
          aria-invalid={error ? true : undefined}
          className={['ie-input', className].filter(Boolean).join(' ')}
        />
      )}
    </FieldShell>
  );
}

export interface SearchInputProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type' | 'value' | 'onChange'> {
  value: string;
  onValueChange: (value: string) => void;
  /** Label for assistive technology; the placeholder is not a label. */
  label: string;
  /** Escape clears the field before it bubbles, if there is anything to clear. */
  onEscape?: () => void;
  ref?: Ref<HTMLInputElement>;
}

export function SearchInput({
  value,
  onValueChange,
  label,
  onEscape,
  className,
  onKeyDown,
  ...rest
}: SearchInputProps) {
  return (
    <div className={['ie-search-input', className].filter(Boolean).join(' ')}>
      <Icon name="search" className="ie-search-input__icon" />
      <input
        {...rest}
        type="search"
        role="searchbox"
        aria-label={label}
        value={value}
        className="ie-input ie-input--search"
        onChange={(event) => onValueChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            if (value) {
              // Clearing is the first thing Escape does; only an already empty
              // field lets it through to close the panel.
              event.stopPropagation();
              onValueChange('');
            } else {
              onEscape?.();
            }
          }
          onKeyDown?.(event);
        }}
      />
      {value ? (
        <button
          type="button"
          className="ie-search-input__clear"
          aria-label="Clear the search"
          onClick={() => onValueChange('')}
        >
          <Icon name="close" />
        </button>
      ) : null}
    </div>
  );
}

export interface SelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export interface SelectProps
  extends Omit<SelectHTMLAttributes<HTMLSelectElement>, 'id' | 'children'> {
  options: SelectOption[];
  label?: ReactNode;
  description?: ReactNode;
  error?: string | null;
  inline?: boolean;
  ref?: Ref<HTMLSelectElement>;
}

export function Select({
  options,
  label,
  description,
  error,
  inline,
  className,
  ...rest
}: SelectProps) {
  return (
    <FieldShell label={label} description={description} error={error} inline={inline}>
      {({ controlId, describedBy }) => (
        <div className="ie-select">
          <select
            {...rest}
            id={controlId}
            aria-describedby={describedBy}
            className={['ie-select__control', className].filter(Boolean).join(' ')}
          >
            {options.map((option) => (
              <option key={option.value} value={option.value} disabled={option.disabled}>
                {option.label}
              </option>
            ))}
          </select>
          <Icon name="chevron-down" className="ie-select__chevron" />
        </div>
      )}
    </FieldShell>
  );
}

export interface CheckboxProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type' | 'id'> {
  label: ReactNode;
  description?: ReactNode;
  ref?: Ref<HTMLInputElement>;
}

export function Checkbox({ label, description, className, ...rest }: CheckboxProps) {
  const base = useId();
  const id = `${base}-checkbox`;
  const descriptionId = description ? `${base}-description` : undefined;

  return (
    <div className={['ie-checkbox', className].filter(Boolean).join(' ')}>
      <input {...rest} type="checkbox" id={id} aria-describedby={descriptionId} />
      <label htmlFor={id}>
        <span className="ie-checkbox__box" aria-hidden="true">
          <Icon name="check" />
        </span>
        <span className="ie-checkbox__text">
          {label}
          {description ? (
            <span className="ie-checkbox__description" id={descriptionId}>
              {description}
            </span>
          ) : null}
        </span>
      </label>
    </div>
  );
}

export interface ToggleProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  /** Hide the text and use it as the accessible name only. */
  hideLabel?: boolean;
  description?: ReactNode;
  disabled?: boolean;
  id?: string;
}

/**
 * A switch.
 *
 * A `button` with `role="switch"` rather than a styled checkbox: the state is
 * announced as on or off, and the control is operable with Space and Enter
 * without any extra handling.
 */
export function Toggle({
  checked,
  onChange,
  label,
  hideLabel = false,
  description,
  disabled,
  id,
}: ToggleProps) {
  const base = useId();
  const descriptionId = description ? `${base}-description` : undefined;

  return (
    <div className="ie-toggle">
      <button
        type="button"
        id={id}
        role="switch"
        aria-checked={checked}
        aria-label={hideLabel ? label : undefined}
        aria-describedby={descriptionId}
        disabled={disabled}
        className={`ie-toggle__track${checked ? ' is-on' : ''}`}
        onClick={() => onChange(!checked)}
      >
        <span className="ie-toggle__thumb" aria-hidden="true" />
      </button>
      {hideLabel ? null : (
        <span className="ie-toggle__text">
          {label}
          {description ? (
            <span className="ie-toggle__description" id={descriptionId}>
              {description}
            </span>
          ) : null}
        </span>
      )}
    </div>
  );
}
