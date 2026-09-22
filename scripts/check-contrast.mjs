#!/usr/bin/env node
//
// Whether the themes are legible.
//
// Colour choices are the part of a design system that goes wrong silently:
// nothing breaks, nothing warns, and a theme that looked fine to whoever
// picked it turns out to be unreadable to someone else. A ratio is the one
// part of that a machine can settle.
//
// This reads `src/styles/theme.css`, resolves each theme's custom properties,
// and checks the pairs that actually meet on screen against WCAG 2.1:
//
//   * body text       4.5:1   (AA for text below 18.66px bold / 24px)
//   * large text      3:1     (headings)
//   * interface       3:1     (borders, focus rings, icons — AA non-text)
//
// A pass means no pair is below its threshold, not that the theme is good.
// Whether the accent reads as a link, whether a colour-blind reader can tell
// the error colour from the success one, and whether any of it is pleasant
// are not here.
//
//   node scripts/check-contrast.mjs

import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const source = readFileSync(join(root, 'src/styles/theme.css'), 'utf8');

/** The pairs that meet on screen, and the rule each has to clear. */
const PAIRS = [
  // Body text on every surface it can land on.
  ['--text-normal', '--background-primary', 'text'],
  ['--text-normal', '--background-primary-alt', 'text'],
  ['--text-normal', '--background-secondary', 'text'],
  ['--text-normal', '--background-secondary-alt', 'text'],
  ['--text-muted', '--background-primary', 'text'],
  ['--text-muted', '--background-secondary', 'text'],
  ['--code-text', '--code-background', 'text'],

  // Semantic text, which carries meaning and so has to be readable, not
  // merely visible.
  ['--text-error', '--background-primary', 'text'],
  ['--text-error', '--background-secondary', 'text'],
  ['--text-success', '--background-primary', 'text'],
  ['--text-warning', '--background-primary', 'text'],
  ['--link-color', '--background-primary', 'text'],
  ['--link-unresolved-color', '--background-primary', 'text'],
  ['--text-on-accent', '--accent', 'text'],
  ['--tag-color', '--background-primary', 'text'],

  // The faint tier is deliberately quiet — timestamps, counts, placeholder
  // text — so it is held to the large-text rule rather than the body one.
  ['--text-faint', '--background-primary', 'large'],
  ['--text-faint', '--background-secondary', 'large'],
  ['--h6-color', '--background-primary', 'large'],

  // Things that are not text but still have to be seen: 1.4.11 covers what is
  // needed to identify a control or its state.
  ['--accent', '--background-primary', 'ui'],
  ['--accent', '--background-secondary', 'ui'],
  ['--interactive-border', '--background-primary', 'ui'],
  ['--interactive-border', '--background-secondary', 'ui'],
  ['--interactive-border', '--background-secondary-alt', 'ui'],
];

// `--border-strong` is deliberately absent. It draws a container's edge — the
// outline of a modal already separated from the page by a backdrop and a
// shadow — which 1.4.11 exempts as decorative. Anything that has to be found
// rather than merely suggested uses `--interactive-border`, which is checked.


const THRESHOLDS = { text: 4.5, large: 3, ui: 3 };
const RULE_NAMES = { text: 'body text', large: 'large text', ui: 'interface' };

// ---- reading the stylesheet ------------------------------------------------

/** The custom properties in one `:root` block, in declaration order. */
function block(selector) {
  const at = source.indexOf(selector);
  if (at === -1) throw new Error(`${selector} is not in theme.css`);
  const open = source.indexOf('{', at);
  const close = source.indexOf('\n}', open);
  const body = source.slice(open + 1, close);

  const values = {};
  for (const match of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    values[match[1]] = match[2].trim();
  }
  return values;
}

const dark = block(':root {');
const light = { ...dark, ...block(":root[data-theme='light']") };

/** Resolve a value, following `var(--other)` and stripping comments. */
function resolve_(theme, name, seen = new Set()) {
  if (seen.has(name)) throw new Error(`${name} refers to itself`);
  seen.add(name);

  const raw = theme[name];
  if (raw === undefined) throw new Error(`${name} is not defined`);

  const indirect = /^var\(\s*(--[\w-]+)\s*\)$/.exec(raw);
  if (indirect) return resolve_(theme, indirect[1], seen);
  return raw;
}

// ---- colour ---------------------------------------------------------------

/** Parse `#rgb`, `#rrggbb`, `rgb()` or `rgba()` into channels plus alpha. */
function parse(value) {
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(value);
  if (hex) {
    const digits =
      hex[1].length === 3
        ? [...hex[1]].map((digit) => digit + digit)
        : [hex[1].slice(0, 2), hex[1].slice(2, 4), hex[1].slice(4, 6)];
    const [r, g, b] = digits.map((pair) => Number.parseInt(pair, 16));
    return { r, g, b, a: 1 };
  }

  const rgb = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:[\s,/]+([\d.]+))?\s*\)$/i.exec(
    value,
  );
  if (rgb) {
    return {
      r: Number(rgb[1]),
      g: Number(rgb[2]),
      b: Number(rgb[3]),
      a: rgb[4] === undefined ? 1 : Number(rgb[4]),
    };
  }

  throw new Error(`cannot read the colour ${value}`);
}

/**
 * Flatten a translucent colour over what is behind it.
 *
 * A ratio against a colour with alpha is meaningless on its own — what the
 * eye sees is the blend, so that is what gets measured.
 */
function over(top, bottom) {
  if (top.a >= 1) return top;
  return {
    r: top.r * top.a + bottom.r * (1 - top.a),
    g: top.g * top.a + bottom.g * (1 - top.a),
    b: top.b * top.a + bottom.b * (1 - top.a),
    a: 1,
  };
}

/** Relative luminance, per WCAG 2.1. */
function luminance({ r, g, b }) {
  const channel = (value) => {
    const c = value / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function ratio(foreground, background) {
  const a = luminance(foreground);
  const b = luminance(background);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

// ---- the check ------------------------------------------------------------

const problems = [];
const checked = [];

for (const [themeName, theme] of [
  ['dark', dark],
  ['light', light],
]) {
  // Every surface a translucent colour might sit on, for the blend.
  const base = parse(resolve_(theme, '--background-primary'));

  for (const [fg, bg, rule] of PAIRS) {
    let foreground;
    let background;
    try {
      background = over(parse(resolve_(theme, bg)), base);
      foreground = over(parse(resolve_(theme, fg)), background);
    } catch (error) {
      problems.push(`${themeName}: ${fg} on ${bg} — ${error.message}`);
      continue;
    }

    const found = ratio(foreground, background);
    const need = THRESHOLDS[rule];
    checked.push({ themeName, fg, bg, rule, found });

    if (found < need) {
      problems.push(
        `${themeName}: ${fg} on ${bg} is ${found.toFixed(2)}:1, ` +
          `below the ${need}:1 needed for ${RULE_NAMES[rule]}`,
      );
    }
  }
}

if (problems.length > 0) {
  console.error('Contrast problems:\n');
  for (const problem of problems) console.error(`  ${problem}`);
  console.error(
    `\n${problems.length} of ${checked.length} pairs fail. Adjust the colour in ` +
      'src/styles/theme.css, or move the pair to a lower rule if the text really is ' +
      'decorative.',
  );
  process.exit(1);
}

// The tightest few are worth seeing even on a pass: they are the ones a small
// colour change would push under.
const tightest = [...checked].sort((a, b) => a.found - b.found).slice(0, 3);
console.log(`All ${checked.length} colour pairs meet WCAG AA. Closest to the line:`);
for (const row of tightest) {
  console.log(
    `  ${row.themeName}: ${row.fg} on ${row.bg} — ${row.found.toFixed(2)}:1 ` +
      `(needs ${THRESHOLDS[row.rule]})`,
  );
}
