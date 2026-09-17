# Writing a plugin

A plugin is a folder in your vault at `.inner-empire/plugins/<id>/` containing
two files:

```
.inner-empire/plugins/word-count/
├── manifest.json
└── main.js
```

Because plugins live inside the vault, they travel with it. Copying the vault
to another machine brings the plugins along, and nothing needs installing.

## The manifest

```json
{
  "id": "word-count",
  "name": "Word Count",
  "version": "1.0.0",
  "description": "Shows the word count of the note you are reading.",
  "author": "You",
  "permissions": ["vault:read", "ui", "commands"]
}
```

The `id` must be lowercase letters, digits and hyphens. It becomes the folder
name and the prefix of every command the plugin registers, so it cannot contain
a path separator.

## Permissions

A plugin declares what it needs and gets nothing else. Calling a method outside
the declaration throws a `PermissionError` rather than being quietly allowed,
which means a plugin that never asked for write access cannot modify a note
even by mistake.

| Permission | What it allows |
|---|---|
| `vault:read` | Reading notes, listing folders, reading the active buffer |
| `vault:write` | Creating, writing, renaming and trashing files |
| `metadata:read` | Backlinks, links, headings, tags, search, events |
| `workspace` | Knowing and changing which note is open |
| `ui` | Adding panels and status-bar items, showing notices |
| `commands` | Registering commands and running them |
| `settings` | Storing the plugin's own settings |
| `network` | Making network requests. Off unless the user grants it. |

Nothing else reaches the network. A plugin without `network` has no way to send
your notes anywhere, which is the point.

## The module

`main.js` is an ES module whose default export has an `onload` function:

```js
export default {
  async onload(ctx) {
    ctx.app.ui.notify('info', 'Hello from a plugin.');
  },

  async onunload() {
    // Optional. Anything registered through ctx.register is cleaned up for you.
  },
};
```

`onload` receives a context with three things:

- `ctx.app` — the API, described below
- `ctx.manifest` — your own manifest, already parsed
- `ctx.register(disposable)` — hand it anything that needs undoing on unload

Everything you register through `ctx.app` is disposed automatically. Use
`ctx.register` for your own resources: intervals, listeners, DOM you created.

## The API

### `app.vault`

```ts
list(limit?): Promise<FileEntry[]>
listFolder(path): Promise<DirectoryListing>
read(path): Promise<Note>          // content plus parsed metadata
exists(path): Promise<boolean>
write(path, content): Promise<void>
create(path, content): Promise<VaultPath>
createFolder(path): Promise<void>
trash(path): Promise<void>         // always recoverable
rename(from, to): Promise<VaultPath>   // rewrites every link
setProperties(path, properties): Promise<void>   // leaves the body untouched
```

Paths are `VaultPath` values: relative to the vault, `/`-separated on every
platform, and unable to address anything outside the vault. Build one with a
plain string; the backend validates it.

### `app.metadata`

```ts
backlinks(path): Promise<Backlink[]>
outgoingLinks(path): Promise<ResolvedLink[]>
headings(path): Promise<Heading[]>
tags(): Promise<TagSummary[]>
filesWithTag(tag): Promise<FileEntry[]>
search(query, limit?): Promise<SearchResults>
resolveLink(from, target): Promise<VaultPath | null>
```

`search` takes the same query language the search panel uses, so
`tag:AI status:active neural` works.

### `app.workspace`

```ts
activeFile(): VaultPath | null
activeContent(): string | null
setActiveContent(content): void
insertAtCursor(text): void
openFile(path, { newPane? }): Promise<void>
```

### `app.ui`

```ts
addPanel({ id, label, icon, side, render }): Disposable
addStatusBarItem({ id, render }): Disposable
notify(level, message): void
confirm(title, message): Promise<boolean>
```

`render` receives a DOM element to fill and may return a cleanup function.

### `app.commands`

```ts
add({ id, name, hotkey?, run }): Disposable
run(id): Promise<boolean>
```

A command appears in the palette under your plugin's name. The `hotkey` is a
suggestion; a binding the user sets in Settings always wins.

### `app.events`

```ts
on(event, handler): Disposable
```

Events: `fileCreated`, `fileModified`, `fileDeleted`, `fileRenamed`,
`activeFileChanged`, `vaultOpened`, `vaultClosed`, `workspaceChanged`.

### `app.settings`

```ts
get(key, fallback): T
set(key, value): Promise<void>
all(): Record<string, unknown>
```

Stored in `data.json` beside your plugin.

## What happens when a plugin fails

Every entry point is wrapped. A plugin that throws during `onload` is disabled
and the user is told which one and why; the window carries on. A handler that
throws is logged and the other handlers still run. This is why one broken
plugin cannot take the application down with it.

## Isolation, honestly stated

Plugins run in the same JavaScript context as the application. The containment
is real but it is capability-based, not a sandbox: a plugin receives only the
object graph handed to `onload`, its paths cannot leave the vault, and its
permissions are checked on every call.

A determined plugin could still reach for browser globals. Moving the host into
a Worker is the next step, and the API is asynchronous throughout so that
becomes a transport change rather than a redesign. Until then, treat installing
a plugin the way you would treat running any program: from a source you trust.

## Examples

| Example | Shows |
|---|---|
| [hello-world](examples/hello-world/) | The smallest working plugin |
| [word-count](examples/word-count/) | A status-bar item that tracks the active note |
| [command](examples/command/) | Registering commands with shortcuts |
| [sidebar](examples/sidebar/) | A panel that lists notes by tag |
| [metadata](examples/metadata/) | Reading links and writing frontmatter |
| [editor](examples/editor/) | Transforming the text being edited |

Copy any of them into `.inner-empire/plugins/` and reload the app.
