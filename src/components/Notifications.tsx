/**
 * Transient messages.
 *
 * Everything the app wants to tell the user without interrupting them arrives
 * here: a rename that updated forty links, a save that failed, a scan that
 * found a problem. Errors stay until dismissed; everything else fades.
 */

import { useEffect, useState } from 'react';

import { events } from '@/services/events';

type Level = 'info' | 'success' | 'warning' | 'error';

interface Notice {
  id: number;
  level: Level;
  message: string;
}

const DISMISS_AFTER: Record<Level, number> = {
  info: 4000,
  success: 3000,
  warning: 8000,
  // Errors do not disappear on their own: the user needs to be able to read
  // and act on them.
  error: 0,
};

let nextId = 0;

export function Notifications() {
  const [notices, setNotices] = useState<Notice[]>([]);

  useEffect(() => {
    const off = events.on('notice', ({ level, message }) => {
      const id = (nextId += 1);
      setNotices((current) => [...current.slice(-4), { id, level, message }]);

      const timeout = DISMISS_AFTER[level];
      if (timeout > 0) {
        setTimeout(() => {
          setNotices((current) => current.filter((notice) => notice.id !== id));
        }, timeout);
      }
    });
    return off;
  }, []);

  if (notices.length === 0) return null;

  return (
    <div className="ie-notifications" role="status" aria-live="polite">
      {notices.map((notice) => (
        <div key={notice.id} className={`ie-notification ie-notification--${notice.level}`}>
          <span className="ie-notification__message">{notice.message}</span>
          <button
            type="button"
            className="ie-icon-button"
            aria-label="Dismiss"
            onClick={() => setNotices((current) => current.filter((n) => n.id !== notice.id))}
          >
            ✕
          </button>
        </div>
      ))}
    </div>
  );
}

/** Raise a notice from anywhere. */
export function notify(level: Level, message: string): void {
  events.emit('notice', { level, message });
}
