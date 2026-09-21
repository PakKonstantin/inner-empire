/**
 * A sidebar panel: a header that stays, a body that scrolls.
 *
 * Every panel in the application had its own version of this — a header div,
 * a scroll div, actions crammed in wherever. One component means the header
 * is the same height everywhere, the actions sit in the same place, and a
 * panel that is collapsed reports that it is collapsed.
 */

import type { ReactNode } from 'react';
import { useId, useState } from 'react';

import { IconButton } from './Button';
import { Icon, type IconName } from './icons';

export interface PanelProps {
  title: string;
  icon?: IconName;
  /** Drawn at the right of the header. Icon buttons, typically. */
  actions?: ReactNode;
  /** A count or short status beside the title. */
  badge?: ReactNode;
  children: ReactNode;
  /** Give the body no padding — a virtual list manages its own. */
  flush?: boolean;
  className?: string;
}

export function Panel({ title, icon, actions, badge, children, flush, className }: PanelProps) {
  return (
    <section className={['ie-panel2', className].filter(Boolean).join(' ')} aria-label={title}>
      <header className="ie-panel2__header">
        {icon ? <Icon name={icon} className="ie-panel2__icon" /> : null}
        <h2 className="ie-panel2__title">{title}</h2>
        {badge ? <span className="ie-panel2__badge">{badge}</span> : null}
        {actions ? <div className="ie-panel2__actions">{actions}</div> : null}
      </header>
      <div className={`ie-panel2__body${flush ? ' is-flush' : ''}`}>{children}</div>
    </section>
  );
}

export interface CollapsibleSectionProps {
  title: string;
  badge?: ReactNode;
  defaultOpen?: boolean;
  /** Controlled mode; leave out to let the section own its state. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  actions?: ReactNode;
  children: ReactNode;
}

/**
 * A section that folds.
 *
 * Built from a button and a region rather than `<details>`, because the
 * summary element cannot hold the action buttons a panel header needs
 * without swallowing their clicks.
 */
export function CollapsibleSection({
  title,
  badge,
  defaultOpen = true,
  open,
  onOpenChange,
  actions,
  children,
}: CollapsibleSectionProps) {
  const [internalOpen, setInternalOpen] = useState(defaultOpen);
  const isOpen = open ?? internalOpen;
  const id = useId();

  const toggle = () => {
    const next = !isOpen;
    if (open === undefined) setInternalOpen(next);
    onOpenChange?.(next);
  };

  return (
    <section className={`ie-section${isOpen ? ' is-open' : ''}`}>
      <div className="ie-section__header">
        <button
          type="button"
          className="ie-section__toggle"
          aria-expanded={isOpen}
          aria-controls={id}
          onClick={toggle}
        >
          <Icon name={isOpen ? 'chevron-down' : 'chevron-right'} className="ie-section__chevron" />
          <span className="ie-section__title">{title}</span>
          {badge !== undefined ? <span className="ie-section__badge">{badge}</span> : null}
        </button>
        {actions ? <div className="ie-section__actions">{actions}</div> : null}
      </div>
      <div className="ie-section__body" id={id} hidden={!isOpen}>
        {children}
      </div>
    </section>
  );
}

export interface PanelHostProps {
  /** Panels stacked in a sidebar, each collapsible and each removable. */
  panels: {
    id: string;
    title: string;
    icon?: IconName;
    render: () => ReactNode;
  }[];
  /** Which panels are pinned open, in order. */
  pinned: string[];
  onUnpin: (id: string) => void;
}

/**
 * Several panels stacked in one sidebar.
 *
 * The brief asks for a right sidebar whose contents the user arranges rather
 * than one panel at a time. Pinned panels stack; the rest stay behind the
 * rail, one at a time.
 */
export function PanelStack({ panels, pinned, onUnpin }: PanelHostProps) {
  const ordered = pinned
    .map((id) => panels.find((panel) => panel.id === id))
    .filter((panel): panel is PanelHostProps['panels'][number] => panel !== undefined);

  return (
    <div className="ie-panel-stack">
      {ordered.map((panel) => (
        <CollapsibleSection
          key={panel.id}
          title={panel.title}
          actions={
            <IconButton
              icon="pin"
              label={`Unpin ${panel.title}`}
              size="sm"
              pressed
              onClick={() => onUnpin(panel.id)}
            />
          }
        >
          {panel.render()}
        </CollapsibleSection>
      ))}
    </div>
  );
}
