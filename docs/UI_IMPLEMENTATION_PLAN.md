# UI implementation plan

The desktop application already has a working interface. This plan is about
raising it to something that survives daily use by someone who has never read
its source: a coherent visual language, a design system rather than a
stylesheet, and the states — empty, loading, failed — that an application only
notices it needs once real people use it.

It is written after reading the tree, not before. Section 1 is the inventory.

---

## 1. What is here today

### 1.1 The shell

`src/app/App.tsx` (964 lines) holds the whole shell: layout, dialogs, link
following, plugin bridge, print, export, import. It renders

```
.ie-app
└── .ie-app__main
    ├── Sidebar side="left"    panels: Files, Search, Tags (+ plugin panels)
    ├── main.ie-app__panes     SplitContainer → TabBar + PaneContent per leaf
    └── Sidebar side="right"   panels: Backlinks, Outline, Properties (+ plugins)
```

and a `StatusBar` below. There is **no top toolbar**: the window has no
workspace name, no global search entry, no breadcrumb, no quick actions. The
`--titlebar-height` token exists and is unused.

### 1.2 Components that exist

| File | What it is | Verdict |
|---|---|---|
| `components/Modal.tsx` | Modal + ConfirmDialog, focus trap | Keep, wrap in the new system |
| `components/ContextMenu.tsx` | `useContextMenu`, keyboard-navigable | Keep |
| `components/Notifications.tsx` | `notify()` toasts | Keep |
| `components/VirtualList.tsx` | Fixed-row windowed list | Keep — this is what makes 10k notes viable |
| `components/{Backlinks,Outline,Properties,Tag}Panel.tsx` | Right/left panel bodies | Keep, restyle |
| `workspace/TabBar.tsx` | Drag-reorder, pin, middle-click close, dirty dot | Keep, extend the menu |
| `workspace/SplitContainer.tsx` + `paneTree.ts` | Recursive splits, 303 lines of tests | Keep untouched |
| `explorer/FileExplorer.tsx` + `useFileTree.ts` | Tree with context menu | Keep, virtualize |
| `commands/{registry,hotkeys}.ts` + `CommandPalette.tsx` | Command table, fuzzy palette, quick switcher | Keep — the registry is the right shape |
| `editor/*` | CodeMirror 6, live preview, completion, theme | Keep untouched |
| `graph/GraphView.tsx`, `canvas/CanvasBoard.tsx` | Force graph, canvas | Keep, add the control surfaces |
| `app/SettingsDialog.tsx` (754 lines) | Category list + panes | Keep structure, add search |
| `app/VaultChooser.tsx` | First-run / vault picker | Keep, finish the onboarding |
| `state/{vault,workspace,settings}Store.ts` | Zustand stores | Keep untouched |
| `services/api.ts` | The single IPC facade, ~90 commands | Keep untouched |

### 1.3 Styling

Three stylesheets: `theme.css` (160 lines of tokens, dark + light,
`prefers-reduced-motion`), `base.css` (229 lines: reset, `.ie-button`,
`.ie-input`, `.ie-icon-button`, `.sr-only`), `layout.css` (1798 lines of
component CSS in 25 labelled sections).

The token file is better than most: 42 colours and 15 dimensions, and iOS
generates its Swift tokens from it (`ios/Scripts/generate-tokens.mjs`), so a
colour changed here changes on both platforms. **Any change to `theme.css`
must keep that generator passing** — CI fails if the committed Swift drifts.

### 1.4 Gaps, measured against the brief

1. **No top toolbar, no breadcrumbs, no quick actions** (§3, §70, §71).
2. **Icons are emoji and box-drawing characters** — `🗂 🔍 # ↩ ◆ ✕ ▥ ▤`. They
   render differently on every machine, cannot be recoloured, and are read
   aloud by a screen reader ("card index dividers"). This is the single
   largest visual-quality problem (§64).
3. **No reusable component layer** (§63). Every panel writes its own markup
   against class names. `.ie-button` exists as CSS only, so nothing enforces
   that a button has a focus ring or a disabled state.
4. **Empty states are bare sentences** in `div.ie-empty` (§25); **no loading
   skeletons** (§26); **errors are toasts with a message** and no recovery
   affordance (§27).
5. **Sidebar panel sets are fixed**: left has no Bookmarks, right has no
   linked mentions, and neither can be pinned or reordered (§5, §6).
6. **Tab menu is missing half the brief's entries** (§8): close to the right,
   reopen closed, open in new pane, copy path, copy link.
7. **No settings search** (§32).
8. **No search filter chips** (§18) — the query language supports `tag:`,
   `path:`, `file:`, `ext:`, `section:`, but the user must type them.
9. **Graph has settings in a popover, not a filter panel** (§20); canvas has
   no zoom controls, fit-to-screen or grid toggle (§21).
10. **No responsive behaviour**: below about 900px the three columns crush
    (§66).
11. **No window-state persistence** — `lib.rs` opens a window and never saves
    its size or position (§58–§60).
12. **The explorer renders every row.** `VirtualList` exists but the tree does
    not use it (§61).

---

## 2. What this plan does not do

- Does not touch `ie-core`, `ie-platform`, the Markdown pipeline, the index,
  search, SQLite, the plugin runtime or the vault format.
- Does not change `services/api.ts`'s shape. New UI uses the commands that
  exist; where a command is missing the plan says so explicitly.
- Does not restyle iOS. `theme.css` stays the shared token source; Swift is
  regenerated when a token changes.
- Does not replace CodeMirror, the graph force layout or the canvas model.
- Adds no runtime dependency for icons or components. Icons are inline SVG in
  one module; components are plain React over the existing tokens.

---

## 3. The design system

### 3.1 Token layers

Three layers, so a theme can be short and a component never hard-codes a
value:

```
primitive        --ie-blue-500, --ie-gray-100      (never used by components)
    ↓
semantic         --color-bg-primary, --color-text-muted, --color-accent
    ↓
component        --tab-height, --sidebar-min-width
```

The existing names (`--background-primary`, `--text-normal`, `--accent`) are
the semantic layer already and are **kept**: renaming them would break every
user theme, the iOS token generator and 1798 lines of CSS for no gain. The
brief's names (`--color-bg-primary`, `--space-md`) are added as **aliases**
pointing at them, so both vocabularies work and new code can use either.

Added, because they are missing and components need them:

| Group | Tokens |
|---|---|
| Typography | `--font-ui`, `--font-editor`, `--font-monospace` (aliases of the existing three), `--font-size-h1…h6`, `--font-weight-{normal,medium,semibold}`, `--letter-spacing-tight` |
| Spacing | `--space-{xs,sm,md,lg,xl}` aliasing `--space-1…5`, plus `--space-6: 32px` |
| Elevation | `--shadow-{sm,md,lg}` (only `--shadow-popover` exists) |
| Z-index | `--z-{base,sticky,dropdown,modal,toast,tooltip}` — currently magic numbers |
| Dimensions | `--toolbar-height`, `--sidebar-width-{min,default,max}`, `--control-height-{sm,md}` |
| Motion | `--transition-{fast,medium}` exist; add `--ease-out`, `--ease-spring` |

### 3.2 Components

One file per component under `src/ui/`, each a thin, typed, accessible
wrapper. Nothing clever — the point is that there is exactly one Button.

```
src/ui/
  Button.tsx        variants: primary | default | quiet | danger; sizes sm|md
  IconButton.tsx    requires aria-label; pressed state
  Input.tsx         label, description, error, invalid wiring
  SearchInput.tsx   Input + icon + clear button + Escape
  Select.tsx        native <select>, styled
  Checkbox.tsx      Toggle.tsx
  Tabs.tsx          roving tabindex, arrow keys
  Panel.tsx         header + scroll body + actions
  Dropdown.tsx      anchored popover, Escape, click-outside
  Tooltip.tsx       delay, keyboard-reachable, aria-describedby
  Badge.tsx         Tag.tsx
  EmptyState.tsx    icon + title + body + optional action
  LoadingState.tsx  Skeleton.tsx — shimmering blocks, motion-safe
  ErrorState.tsx    title + cause + actions ([Retry] [Compare])
  Breadcrumb.tsx    clickable segments
  Icon.tsx          the icon set
```

`Modal`, `ContextMenu`, `Notifications`, `VirtualList`, `CommandPalette` and
`FileTree` already exist and are re-exported from `src/ui/` so there is one
import site.

### 3.3 Icons

One set, drawn as inline SVG in `src/ui/icons.tsx`: 24×24 viewBox, 1.5px
stroke, `currentColor`, `stroke-linecap: round`. Roughly 40 glyphs — file,
folder, folder-open, note, search, tag, link, graph, canvas, settings,
close, chevrons, plus, pin, split, sidebar, sun, moon, warning, check,
trash, copy, external, image, pdf, audio, video, more.

`<Icon name="search" />` is the only way a component gets a glyph. Size and
stroke come from the token layer, so they cannot drift. Decorative icons get
`aria-hidden`; an icon that is the whole control gets its label from
`IconButton`.

No icon dependency is added: 40 hand-written paths cost ~6KB and avoid
pulling a 2MB package for a tenth of it.

---

## 4. Shell architecture

```
┌──────────────────────────────────────────────────────────────┐
│ Toolbar   [☰] Vault ▾  ‹ ›  Breadcrumb…      [search] [⚙][◫] │  --toolbar-height
├────────┬────────────────────────────────────┬────────────────┤
│  Rail  │ TabBar                             │ Rail           │
│ Files  │ ┌────────────────────────────────┐ │ Outline        │
│ Search │ │ Editor / Reading / Graph /     │ │ Backlinks      │
│ Book   │ │ Canvas / PDF / Image           │ │ Properties     │
│ Tags   │ └────────────────────────────────┘ │ Links          │
│ Outline│                                    │                │
├────────┴────────────────────────────────────┴────────────────┤
│ Status bar                                                   │
└──────────────────────────────────────────────────────────────┘
```

**Toolbar** (new, `app/Toolbar.tsx`): sidebar toggles, back/forward through
the open history, the breadcrumb of the active note, a search field that
opens the search panel with the query, and quick actions (new note, command
palette, settings). Nothing else — the brief is explicit that the toolbar
must not grow.

**Rails** (reworked `app/Sidebar.tsx`): the icon column becomes a real rail
with tooltips, and the panel set is data-driven so Bookmarks and Outline can
join the left side and linked mentions the right. Panel identity is already
persisted in `WorkspaceSidebar.activePanel`, so this is free.

**Right sidebar stacking** (§6): panels gain a pinned flag. Pinned panels
stack vertically in the sidebar body; unpinned ones show one at a time. The
flag lives in the workspace record under a new optional key, added the same
way `favourites` was — `serde(default)`, carried by the desktop even when it
does not manage it, so iOS is unaffected.

**Responsive** (§66): container queries on `.ie-app`.

| Width | Behaviour |
|---|---|
| ≥ 1280px | Both sidebars, both at their stored width |
| 960–1280px | Right sidebar collapses to its rail; reopens as an overlay |
| < 960px | Both collapse to rails; the editor keeps the window |

Collapsing is presentational: the stored width is untouched, so widening the
window restores exactly what the user arranged.

---

## 5. Area plans

**Explorer** — rows through `VirtualList` (flatten the visible tree, one array,
constant DOM cost); file-type icons; drag and drop; sort (name, modified,
created) and a filter field; the context menu gains reveal-in-file-manager,
copy path and copy link.

**Tabs** — the menu gains close-to-the-right, reopen-closed (a small stack in
the workspace store), open-in-new-pane, copy path, copy link. The dirty dot
keeps its double duty as the close button.

**Editor** — no behavioural change. Typography only: `--font-editor` at 16px
default, a heading scale that is not "bold text", 1.65 line height, 68ch
measure when readable-line-length is on.

**Reading view** — becomes a document rather than an editor without a cursor:
wider heading rhythm, callouts, figure captions, a table wrapper that scrolls
horizontally rather than bursting the column, and print styles that match.

**Search** — filter chips above the results. Each chip is a parsed clause of
the query; removing a chip rewrites the query. A hit shows its snippet with
the matched run marked, which the backend already returns.

**Graph** — a filter panel (depth, tags, folders, link kinds, attachments,
orphans) bound to the existing `GraphSettings`, plus zoom controls and a
search field that highlights matching nodes.

**Canvas** — zoom controls, fit-to-screen, grid toggle, selection tools. The
canvas model and its 188 lines of tests are untouched.

**Settings** — a search field that filters categories and fields, matching on
label, description and keywords. Every field gains a stable id so search can
deep-link to it.

**Onboarding** — `VaultChooser` gains the welcome copy the brief asks for, and
after a vault is created, a first-run panel offering a first note and the
three shortcuts that matter.

---

## 6. Accessibility

Not a phase at the end; a rule each component carries.

- Every interactive element is a real `button`, `a`, `input` or has an ARIA
  role with keyboard handling to match.
- `:focus-visible` is already global; components must not remove it.
- Icon-only controls require `aria-label` in the type signature.
- Menus, tabs and lists use roving tabindex.
- Modals trap focus and restore it on close (Modal already does).
- Colour contrast: body text ≥ 7:1, secondary ≥ 4.5:1, borders ≥ 3:1 in both
  themes, checked with a script rather than by eye.
- `prefers-reduced-motion` already disables transitions globally.

---

## 7. Performance budget

| Thing | Budget | How |
|---|---|---|
| Explorer with 50k files | < 16ms per frame | `VirtualList`, flattened tree |
| Search results | < 16ms | Already virtual |
| Tab switch | < 50ms | Buffers already cached in the store |
| Typing | No dropped frames | CodeMirror owns its own rendering |
| Graph, 5k nodes | Interactive | Canvas, capped node count |

Rules: no synchronous IPC from render; no store subscription that returns a
new object every call; `React.memo` on row components; panel bodies mounted
only when their panel is visible.

---

## 8. Phases

| Phase | Work | Done when |
|---|---|---|
| 2 | Tokens, icons, component library | Every component has a focus state and a test |
| 3 | Shell: toolbar, rails, responsive, window state | Three columns collapse cleanly; window position restored |
| 4 | Explorer | 50k rows scroll smoothly; drag and drop works |
| 5 | Tabs and workspace | Menu complete; reopen-closed works |
| 6 | Editor and reading typography | Reading view looks like a document |
| 7 | Search, palette, quick switcher | Filter chips work |
| 8 | Graph and canvas controls | Filter panel bound to real settings |
| 9 | Settings search, themes, onboarding | Searching "dark" finds the theme setting |
| 10 | Accessibility and performance pass | Contrast script passes; no console errors |

Phases 11–15 are the release work and live in
[RELEASE_IMPLEMENTATION_PLAN.md](RELEASE_IMPLEMENTATION_PLAN.md).

After each phase: `pnpm lint && pnpm typecheck && pnpm test && pnpm build`.
A phase that breaks an existing behaviour is not finished.

---

## 9. Definition of done

- The interface is visually coherent: one icon set, one type scale, one
  spacing rhythm, no hard-coded colours outside the token files.
- Every function reachable by mouse is reachable by keyboard.
- Light and dark are both complete; system follows the OS.
- 10,000 notes do not degrade the explorer, the search or the tab strip.
- No empty panel without an explanation, no spinner that blocks the window,
  no error that shows a code instead of a sentence.
- `pnpm build` produces no console errors and no unused-token warnings.
