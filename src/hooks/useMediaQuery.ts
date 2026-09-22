/**
 * Whether a media query matches, as React state.
 *
 * The layout itself is CSS — this is only for the handful of places where a
 * breakpoint changes behaviour rather than appearance, such as a sidebar that
 * becomes a drawer and therefore needs something to dismiss it.
 */

import { useEffect, useState } from 'react';

export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() => read(query));

  useEffect(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return;
    const media = window.matchMedia(query);
    // The query can change between renders, so the first read belongs here
    // too rather than only in the initial state.
    setMatches(media.matches);

    const onChange = (event: MediaQueryListEvent) => setMatches(event.matches);
    media.addEventListener('change', onChange);
    return () => media.removeEventListener('change', onChange);
  }, [query]);

  return matches;
}

function read(query: string): boolean {
  if (typeof window === 'undefined' || !window.matchMedia) return false;
  return window.matchMedia(query).matches;
}

/** The width below which a sidebar overlays the editor instead of sitting beside it. */
export const NARROW_QUERY = '(max-width: 960px)';
