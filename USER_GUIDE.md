# Using Inner Empire

## Your vault is a folder

When you open a vault, you are pointing the app at a directory. Everything in
it that ends in `.md` becomes a note. Nothing is copied anywhere, nothing is
converted, and you can keep editing the same files in any other editor while
the app is running — it notices and updates.

The app adds one folder of its own, `.inner-empire`, holding the search index,
your layout, the trash and any plugins. You can delete the index at any time;
it rebuilds itself from your notes.

To start: **Open a folder** to use notes you already have, or **Create a vault**
to start fresh with a few conventional folders and a welcome note.

## Writing

The editor shows Markdown as you type it, hiding the symbols on lines your
cursor is not on. Move the cursor onto a line and its markup reappears, ready
to edit. The document is always the real Markdown, so copying, undo and every
keyboard shortcut behave the way they do in a plain text editor.

Everything standard works: headings, bold, italic, strikethrough, inline code,
fenced code blocks, quotes, ordered and unordered lists, task checkboxes,
tables, horizontal rules, links, images and footnotes.

A few conveniences:

- Enter inside a list starts the next item. Enter on an empty item ends the
  list.
- Tab and Shift+Tab indent and outdent a list item.
- Ctrl+B, Ctrl+I and Ctrl+E wrap the selection in bold, italic or code.
- Ctrl+1 through Ctrl+4 set a heading level; pressing the same one again
  removes it.
- Ctrl+Enter ticks or unticks a task.

Your work is saved automatically a moment after you stop typing, and whenever
you switch tabs or close the window. The status bar says whether the note on
screen matches the file on disk.

## Linking notes

Type `[[` and start typing a note's name. The suggestions come from your whole
vault.

```
[[Project Plan]]                  a link
[[Project Plan|the plan]]         showing different text
[[Project Plan#Scope]]            to a heading
[[Project Plan#^key-point]]       to a specific paragraph
![[Project Plan]]                 showing the note inside this one
![[diagram.png]]                  showing an image
![[diagram.png|400]]              at a set width
```

Continue `[[Project Plan#` and the suggestions become that note's headings.

**Linking to a note that does not exist yet** is not a mistake — it is how you
write. The link appears in a different colour, and clicking it creates the note
with that name and opens it. Until then it shows up under unresolved links and
as a faint node in the graph, so you can see what you have been meaning to
write.

**Renaming a note updates every link to it.** Aliases, heading references and
block references all survive, and links inside code blocks are left alone. If
shortening a link would make it ambiguous with another note of the same name,
the full path is written instead, so a rename can never quietly point a link
at the wrong file.

To reference one paragraph, put `^some-id` at the end of it, then link with
`[[Note#^some-id]]`.

## Tags

Write `#AI` anywhere in a note, or list tags in the frontmatter. Tags nest with
a slash: `#AI/LLM` sits under `#AI`, and the tag panel shows the hierarchy with
counts that include everything beneath.

Typing `#` in the editor suggests tags already used in your vault, so your
taxonomy stays consistent without you maintaining it.

## Properties

A note can open with a block of YAML:

```markdown
---
title: Quarterly Review
tags:
  - planning
status: active
rating: 8
due: 2026-09-17
done: false
---
```

The Properties panel gives each value an editor suited to its type — a date
picker for a date, a checkbox for a boolean, a list editor for a list. Editing
here rewrites only the frontmatter; the body of your note is returned exactly
as you left it, down to the whitespace.

Types are recognised as you would expect: `8` is a number, `"8"` is text,
`2026-09-17` is a date, `true` is a checkbox. That matters because searching
for `rating>7` compares numerically rather than alphabetically.

## Finding things

**Ctrl+O** opens the quick switcher. Type any part of a note's name; `prjpl`
finds "Project Plan".

**Ctrl+Shift+F** searches the text of every note.

```
machine learning         notes containing both words
"machine learning"       that exact phrase
neural -draft            containing one, without the other
tag:AI                   tagged #AI, including #AI/LLM and friends
path:Projects            inside a folder
file:readme              by filename
ext:png                  by extension
status:active            a frontmatter property
rating>7                 compared as a number
section:"Design notes"   notes with a matching heading
is:orphan                nothing links to it
is:unresolved            contains a link to a note that does not exist
is:untagged              has no tags
```

These combine: `tag:AI status:active -draft` is a perfectly good query.

## Backlinks

Open a note and the Backlinks panel shows every note that links to it, with the
sentence around each link. It updates as you and the app change things.

Below that, **unlinked mentions** are notes that write this note's title
without linking to it — the raw material for your next link.

## Tabs and panes

Tabs behave the way you expect: middle-click closes, dragging reorders, and
right-clicking offers pin, duplicate and close-others. A pinned tab survives
"close others" and sorts to the front. A tab with unsaved changes shows a dot
where its close button would be.

**Ctrl+\\** splits the pane to the right, **Ctrl+Shift+\\** splits downward.
Splits nest, dividers drag, and each pane navigates on its own. Drag a tab onto
another pane to move it there.

Your layout is saved with the vault, so closing the app and opening it again
puts everything back — the same tabs, the same splits, the same sizes, the
cursor where you left it.

## The graph

**Ctrl+G** opens the graph of your whole vault. Each note is a node, sized by
how connected it is; each link is an edge. Drag to pan, scroll to zoom, drag a
node to move it, and click one to open that note. Pointing at a node highlights
what it connects to.

The controls let you include attachments, tags and notes that do not exist yet,
and adjust how the layout spaces things out.

The **local graph**, in the right sidebar, shows only the neighbourhood of the
note you are reading, out to a depth you choose.

## Canvas

A canvas is a board for arranging things in space rather than in sequence.
Create one by making a file ending in `.canvas`.

- **Add card** for a text card, which accepts Markdown.
- Drag a note from the file tree onto the board to add a card showing it.
- **Add group** to draw a region; moving a group moves what is inside it.
- Select a card and drag from one of the four dots on its edges to draw an
  arrow to another card. Double-click an arrow to remove it.
- Drag to select several cards, or Ctrl+A for all of them.
- Arrow keys nudge the selection; hold Shift to move further.
- Ctrl+scroll zooms, and **Fit** frames everything.

The board is saved as JSON in your vault, so it travels with your notes and
reads cleanly in version control.

## Attachments

Drag an image, a PDF, a sound file or a video into a note, or paste one. It is
copied into your attachment folder and a link is inserted — images embed so you
see them, everything else links.

Where attachments go is up to you: one folder for the whole vault, beside the
note that uses them, or in a subfolder next to it. That is in Settings, under
Files and links.

PDFs open in a tab with page navigation and zoom. Images open with zoom.

## Templates and daily notes

Put Markdown files in your `Templates` folder and they become templates.
**Ctrl+Shift+T** inserts one. These variables are replaced:

```
{{title}}         the note's name
{{date}}          2026-09-17
{{time}}          14:05
{{datetime}}      both
{{date:dddd}}     any format you like, here "Thursday"
{{yesterday}}     and {{tomorrow}}, for linking days together
```

Formats use `YYYY`, `MM`, `DD`, `dddd`, `HH`, `mm` and `ww`.

**Ctrl+Shift+D** opens today's daily note, creating it from your template if it
does not exist. The folder and filename format are in Settings.

## Deleting and recovering

Deleting moves a file to a trash folder inside your vault. It is still there,
under `.inner-empire/trash`, and you can restore it from Settings → Trash or
just with your file manager. Nothing is removed for good until you say so.

If something already occupies the place a restored file came from, the restored
copy gets a numbered name rather than overwriting what is there.

## Keyboard shortcuts

| | |
|---|---|
| Ctrl+P | Command palette |
| Ctrl+O | Open a note |
| Ctrl+N | New note |
| Ctrl+S | Save now |
| Ctrl+Shift+F | Search the vault |
| Ctrl+F | Find in this note |
| Ctrl+H | Find and replace |
| Ctrl+G | Graph |
| Ctrl+B | Toggle the left sidebar |
| Ctrl+Shift+B | Toggle the right sidebar |
| Ctrl+\\ | Split right |
| Ctrl+Shift+\\ | Split down |
| Ctrl+W | Close tab |
| Ctrl+Tab | Next tab |
| Ctrl+Shift+D | Today's note |
| Ctrl+Shift+T | Insert a template |
| Ctrl+Shift+R | Switch between editing and reading |
| Ctrl+Shift+P | Print, or save as PDF |
| F2 | Rename |
| Ctrl+, | Settings |

Every one of these is editable in Settings → Keyboard shortcuts, and everything
in the command palette can be given a shortcut whether or not it has one now.

## Getting notes in and out

**Import** copies files into the vault, sanitising any name that would be
illegal on Windows so the vault stays portable. It is in the command palette
under "Import files into this vault".

**Export** writes a note out as HTML with its links rewritten to point at the
exported files, or as Markdown unchanged. Both are in the command palette.

**PDF** goes through Print (Ctrl+Shift+P), which offers "save as PDF" on both
platforms. What you get is what you were reading: the same renderer, laid out
for paper. This reuses the printing the system already does well rather than
shipping a second PDF engine that would render your notes slightly differently.

## Moving a vault between machines

Copy the folder. That is the whole procedure.

A vault opens identically on Windows and Linux: every path stored anywhere uses
forward slashes and is relative to the vault, and names are checked against the
stricter of the two platforms' rules, so a vault written on Linux cannot
contain a name Windows would mangle.

Two things are worth knowing. The index does not travel usefully — it is
rebuilt on first open, which takes seconds. And if your vault somehow contains
two files whose names differ only by capitalisation, the app tells you: that is
fine on Linux and destructive on Windows, so it is flagged rather than
discovered later.

## Themes

Light and dark, or follow the system. Settings → Appearance.

A theme is a CSS file that redefines a set of variables. Drop one in
`.inner-empire/themes/` in your vault and it becomes available. Because
everything including the graph reads its colours from those variables, a short
file restyles the whole application.

## Plugins

Plugins live in `.inner-empire/plugins/` in your vault, so they travel with it.
Each declares what it needs — reading notes, writing them, adding commands or
panels — and the app refuses anything it did not declare. Network access is off
unless you grant it, which means a plugin without it has no way to send your
notes anywhere.

If a plugin fails, it is disabled and you are told which one; the app carries
on.

See the [plugin guide](PLUGIN_API.md) to write one.

## When something goes wrong

**The index looks wrong.** Settings → About → Rebuild the index. It cannot lose
anything: your notes are the source, and the index is only a cache.

**A note will not open.** The status bar shows a count of scan notices; click
it. Unreadable files, malformed frontmatter and name conflicts are all reported
there, with what the app did about each.

**The app closed unexpectedly.** Reopen it. Every write is atomic, so a note is
either its old content or its new content, never half of each. Any leftover
temporary file is reported as an interrupted write rather than being silently
removed.

**Something changed outside the app while you were editing.** Your unsaved
version is kept and you are told, because it is the one copy the disk does not
already have. Save to keep yours, or close the tab without saving to take the
version on disk.
