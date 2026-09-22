/**
 * Modal dialogs.
 *
 * Focus moves into the dialog on open and returns to whatever had it on close,
 * and Tab is trapped inside, so a dialog is usable without a mouse and does not
 * strand a keyboard user behind it.
 */

import type { ReactNode } from 'react';
import { useCallback, useEffect, useRef } from 'react';

export interface ModalProps {
  title: string;
  description?: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  /** Wider dialogs for settings and pickers. */
  size?: 'small' | 'medium' | 'large';
}

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function Modal({ title, description, onClose, children, footer, size = 'medium' }: ModalProps) {
  const surface = useRef<HTMLDivElement | null>(null);
  const previouslyFocused = useRef<HTMLElement | null>(null);

  useEffect(() => {
    previouslyFocused.current = document.activeElement as HTMLElement | null;
    const first = surface.current?.querySelector<HTMLElement>(FOCUSABLE);
    (first ?? surface.current)?.focus();

    return () => {
      previouslyFocused.current?.focus?.();
    };
  }, []);

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== 'Tab' || !surface.current) return;

      const focusable = [...surface.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
      if (focusable.length === 0) return;
      const first = focusable[0]!;
      const last = focusable[focusable.length - 1]!;

      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    },
    [onClose],
  );

  return (
    <div className="ie-modal-backdrop" onPointerDown={onClose}>
      <div
        className={`ie-modal ie-modal--${size}`}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        aria-describedby={description ? 'ie-modal-description' : undefined}
        tabIndex={-1}
        ref={surface}
        onPointerDown={(event) => event.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <header className="ie-modal__header">
          <h2 className="ie-modal__title">{title}</h2>
          <button type="button" className="ie-icon-button" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </header>
        {description ? (
          <p className="ie-modal__description" id="ie-modal-description">
            {description}
          </p>
        ) : null}
        <div className="ie-modal__body">{children}</div>
        {footer ? <footer className="ie-modal__footer">{footer}</footer> : null}
      </div>
    </div>
  );
}

export interface ConfirmDialogProps {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  title,
  message,
  confirmLabel = 'Confirm',
  cancelLabel = 'Cancel',
  danger,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  return (
    <Modal
      title={title}
      onClose={onCancel}
      size="small"
      footer={
        <>
          <button type="button" className="ie-button" onClick={onCancel}>
            {cancelLabel}
          </button>
          <button
            type="button"
            className={`ie-button ie-button--primary${danger ? ' ie-button--danger' : ''}`}
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        </>
      }
    >
      <p className="ie-modal__message">{message}</p>
    </Modal>
  );
}
