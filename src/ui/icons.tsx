/**
 * The icon set.
 *
 * One vocabulary, drawn once. Before this the interface used emoji and
 * box-drawing characters — 🗂 for files, ▥ for a split, ✕ for close. They
 * render differently on every machine, cannot be recoloured or aligned, and a
 * screen reader says "card index dividers" out loud.
 *
 * These are plain paths on a 24×24 grid, stroked in `currentColor` at the
 * weight the token layer sets, so an icon inherits the colour of the thing it
 * sits in and changes with the theme for free. Around fifty glyphs cost a few
 * kilobytes; a package would cost megabytes for the same fifty.
 *
 * Drawing rules, so a later addition matches: 24×24 viewBox, strokes only
 * (no fills), 2px of visual margin, round caps and joins. A dot is a
 * zero-length stroke — `M12 12h.01` — which the round cap turns into a circle.
 */

import type { ReactElement } from 'react';

export type IconName = keyof typeof paths;

const paths = {
  // ---- Files and folders ----
  file: <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5" />,
  'file-text': (
    <>
      <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5" />
      <path d="M9 13h6M9 17h4" />
    </>
  ),
  folder: <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />,
  'folder-open': (
    <>
      <path d="M3 19V6a1 1 0 0 1 1-1h4.5l2 2H17a1 1 0 0 1 1 1v2" />
      <path d="M3.5 19l2.2-7.3A1 1 0 0 1 6.7 11H21l-2.2 7.3a1 1 0 0 1-1 .7z" />
    </>
  ),
  'folder-plus': (
    <>
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
      <path d="M12 10.5v5M9.5 13h5" />
    </>
  ),
  image: (
    <>
      <path d="M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z" />
      <path d="M8.5 11a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM21 15l-5-4.5L6 19" />
    </>
  ),
  pdf: (
    <>
      <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5" />
      <path d="M9 18v-5h1.4a1.3 1.3 0 0 1 0 2.6H9M14 18v-5h1.6a1.4 1.4 0 0 1 1.4 1.4v2.2a1.4 1.4 0 0 1-1.4 1.4z" />
    </>
  ),
  audio: (
    <>
      <path d="M9 17V5l11-2v12" />
      <path d="M7 20a2 2 0 1 0 0-4 2 2 0 0 0 0 4zM18 18a2 2 0 1 0 0-4 2 2 0 0 0 0 4z" />
    </>
  ),
  video: (
    <>
      <path d="M3 6h11a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1z" />
      <path d="M15 10.5l6-3.5v10l-6-3.5z" />
    </>
  ),
  paperclip: <path d="M20.5 11.5l-8.6 8.6a5 5 0 0 1-7.1-7.1l9-9a3.4 3.4 0 0 1 4.8 4.8l-8.8 8.8a1.8 1.8 0 0 1-2.5-2.5l8.1-8.1" />,
  vault: (
    <>
      <path d="M12 3l9 4.8v8.4L12 21l-9-4.8V7.8z" />
      <path d="M3.3 7.6L12 12.3l8.7-4.7M12 12.3V21" />
    </>
  ),

  // ---- Navigation ----
  'chevron-right': <path d="M9.5 5.5l6.5 6.5-6.5 6.5" />,
  'chevron-left': <path d="M14.5 5.5L8 12l6.5 6.5" />,
  'chevron-down': <path d="M5.5 9.5L12 16l6.5-6.5" />,
  'chevron-up': <path d="M18.5 14.5L12 8l-6.5 6.5" />,
  'chevrons-up': <path d="M18 17l-6-6-6 6M18 11l-6-6-6 6" />,
  'arrow-left': <path d="M19 12H5M11 6l-6 6 6 6" />,
  'arrow-right': <path d="M5 12h14M13 6l6 6-6 6" />,
  'corner-down-left': <path d="M20 4v7a4 4 0 0 1-4 4H5M9 11l-4 4 4 4" />,
  undo: <path d="M4 9h11a5 5 0 0 1 0 10H9M4 9l4-4M4 9l4 4" />,
  home: <path d="M3 11l9-8 9 8M6 10v10h12V10" />,

  // ---- Actions ----
  plus: <path d="M12 5.5v13M5.5 12h13" />,
  close: <path d="M6.5 6.5l11 11M17.5 6.5l-11 11" />,
  check: <path d="M5 12.5l4.5 4.5L19 7" />,
  search: (
    <>
      <path d="M10.5 17.5a7 7 0 1 0 0-14 7 7 0 0 0 0 14z" />
      <path d="M20.5 20.5l-5-5" />
    </>
  ),
  trash: (
    <>
      <path d="M4 7h16M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2" />
      <path d="M6 7l.9 12a2 2 0 0 0 2 1.9h6.2a2 2 0 0 0 2-1.9L18 7M10 11.5v5M14 11.5v5" />
    </>
  ),
  copy: (
    <>
      <path d="M9.5 9.5h9a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1z" />
      <path d="M5.5 14.5h-1a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1h9a1 1 0 0 1 1 1v1" />
    </>
  ),
  pencil: <path d="M4 20h4.2L20 8.2a2.3 2.3 0 0 0-3.2-3.2L5 16.8zM15.5 6.5l2 2" />,
  pin: <path d="M12 16.5V22M8 3.5h8l-1.2 6.4 2.7 3.1H6.5l2.7-3.1z" />,
  'external-link': <path d="M14 4h6v6M20 4l-8.5 8.5M18 14.5V19a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h4.5" />,
  more: <path d="M5.5 12h.01M12 12h.01M18.5 12h.01" />,
  refresh: <path d="M20.5 12a8.5 8.5 0 1 1-2.5-6M20.5 3v5.5H15" />,
  filter: <path d="M3.5 5h17l-6.7 7.6V19l-3.6-2v-4.4z" />,
  sort: <path d="M7 4.5v15M4 16.5l3 3 3-3M17 19.5v-15M14 7.5l3-3 3 3" />,
  star: <path d="M12 3.5l2.7 5.6 6 .8-4.4 4.3 1.1 6-5.4-2.9-5.4 2.9 1.1-6L3.3 9.9l6-.8z" />,
  bookmark: <path d="M6 3.5h12v17l-6-4.2-6 4.2z" />,
  download: <path d="M12 3.5v12M7 11l5 5 5-5M4 20.5h16" />,

  // ---- Views and layout ----
  'sidebar-left': (
    <>
      <path d="M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z" />
      <path d="M9.5 5v14" />
    </>
  ),
  'sidebar-right': (
    <>
      <path d="M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z" />
      <path d="M14.5 5v14" />
    </>
  ),
  'split-vertical': (
    <>
      <path d="M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z" />
      <path d="M12 5v14" />
    </>
  ),
  'split-horizontal': (
    <>
      <path d="M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z" />
      <path d="M3 12h18" />
    </>
  ),
  graph: (
    <>
      <path d="M17.5 7.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5zM6.5 14.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5zM17.5 21.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z" />
      <path d="M8.8 10.8l6.4-2.6M8.8 13.2l6.4 2.6" />
    </>
  ),
  canvas: (
    <>
      <path d="M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z" />
      <path d="M3 10h18M10 10v9" />
    </>
  ),
  'book-open': <path d="M12 7.5v12.5M12 7.5A4 4 0 0 0 8 4.5H3v13h5a4 4 0 0 1 4 2.5M12 7.5a4 4 0 0 1 4-3h5v13h-5a4 4 0 0 0-4 2.5" />,
  eye: (
    <>
      <path d="M2.5 12S6 6.5 12 6.5 21.5 12 21.5 12 18 17.5 12 17.5 2.5 12 2.5 12z" />
      <path d="M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z" />
    </>
  ),
  list: <path d="M8 6h13M8 12h13M8 18h13M3.5 6h.01M3.5 12h.01M3.5 18h.01" />,
  grid: <path d="M3 9h18M3 15h18M9 3v18M15 3v18" />,
  maximize: <path d="M8 3.5H5a1.5 1.5 0 0 0-1.5 1.5v3M16 3.5h3A1.5 1.5 0 0 1 20.5 5v3M20.5 16v3a1.5 1.5 0 0 1-1.5 1.5h-3M3.5 16v3A1.5 1.5 0 0 0 5 20.5h3" />,
  'zoom-in': (
    <>
      <path d="M10.5 17.5a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20.5 20.5l-5-5" />
      <path d="M10.5 7.5v6M7.5 10.5h6" />
    </>
  ),
  'zoom-out': (
    <>
      <path d="M10.5 17.5a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20.5 20.5l-5-5" />
      <path d="M7.5 10.5h6" />
    </>
  ),

  // ---- Semantics ----
  tag: (
    <>
      <path d="M11.5 3.5H4a.5.5 0 0 0-.5.5v7.5l9 9 8-8z" />
      <path d="M7.5 7.5h.01" />
    </>
  ),
  hash: <path d="M4 9.5h16M4 14.5h16M10 3.5L8 20.5M16 3.5l-2 17" />,
  link: (
    <>
      <path d="M10 13.5a4.5 4.5 0 0 0 6.4 0l2.6-2.6a4.5 4.5 0 0 0-6.4-6.4l-1 1" />
      <path d="M14 10.5a4.5 4.5 0 0 0-6.4 0L5 13.1a4.5 4.5 0 0 0 6.4 6.4l1-1" />
    </>
  ),
  clock: (
    <>
      <path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z" />
      <path d="M12 7v5.2l3.2 2" />
    </>
  ),
  info: (
    <>
      <path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z" />
      <path d="M12 11.5v5M12 8h.01" />
    </>
  ),
  warning: <path d="M12 4l9.2 16H2.8zM12 10.5v4M12 17.5h.01" />,
  error: (
    <>
      <path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z" />
      <path d="M12 7.5v5.5M12 16.5h.01" />
    </>
  ),
  success: (
    <>
      <path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z" />
      <path d="M8 12.2l2.8 2.8L16.5 9" />
    </>
  ),

  // ---- Settings and input ----
  settings: <path d="M4 20.5v-6M4 10.5v-7M12 20.5v-8M12 8.5v-5M20 20.5v-4M20 12.5v-9M1.5 14.5h5M9.5 8.5h5M17.5 16.5h5" />,
  sun: (
    <>
      <path d="M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8z" />
      <path d="M12 2v2M12 20v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M2 12h2M20 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4" />
    </>
  ),
  moon: <path d="M20.5 14.2A8.5 8.5 0 0 1 9.8 3.5a8.5 8.5 0 1 0 10.7 10.7z" />,
  monitor: (
    <>
      <path d="M4 4.5h16a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1z" />
      <path d="M8.5 20.5h7M12 15.5v5" />
    </>
  ),
  keyboard: (
    <>
      <path d="M3 6.5h18a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1z" />
      <path d="M6.5 10h.01M10 10h.01M13.5 10h.01M17 10h.01M8 14h8" />
    </>
  ),
  command: <path d="M9 9V6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3z" />,
  spinner: <path d="M12 3a9 9 0 0 1 9 9" />,
  dot: <path d="M12 12h.01" />,
} as const;

export interface IconProps {
  name: IconName;
  /** Overrides `--icon-size-md`, in pixels. */
  size?: number;
  className?: string;
  /**
   * A label makes the icon meaningful to a screen reader. Without one it is
   * hidden, which is right when the surrounding control is already named —
   * and that is the common case, so it is the default.
   */
  label?: string;
}

export function Icon({ name, size, className, label }: IconProps): ReactElement {
  return (
    <svg
      className={['ie-icon', className].filter(Boolean).join(' ')}
      width={size ?? undefined}
      height={size ?? undefined}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="var(--icon-stroke, 1.6)"
      strokeLinecap="round"
      strokeLinejoin="round"
      role={label ? 'img' : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      focusable="false"
    >
      {paths[name]}
    </svg>
  );
}

/** Every name, for tests and for the icon gallery in settings. */
export const iconNames = Object.keys(paths) as IconName[];

/** The icon for a file, chosen by extension. */
export function iconForFile(path: string): IconName {
  const extension = path.slice(path.lastIndexOf('.') + 1).toLowerCase();
  if (extension === 'md' || extension === 'markdown') return 'file-text';
  if (extension === 'canvas') return 'canvas';
  if (extension === 'pdf') return 'pdf';
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'avif'].includes(extension)) {
    return 'image';
  }
  if (['mp3', 'wav', 'ogg', 'flac', 'm4a', 'aac'].includes(extension)) return 'audio';
  if (['mp4', 'webm', 'mov', 'mkv', 'avi'].includes(extension)) return 'video';
  return 'file';
}
