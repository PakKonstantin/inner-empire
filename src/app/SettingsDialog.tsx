/**
 * Settings.
 *
 * Grouped by what the setting belongs to rather than by where it is stored,
 * which is why vault settings and application preferences sit side by side
 * here even though they are written to different places.
 */

import { useCallback, useEffect, useState } from 'react';

import { commands } from '@/commands/registry';
import { combinationFromEvent, formatCombination } from '@/commands/hotkeys';
import { Modal } from '@/components/Modal';
import { notify } from '@/components/Notifications';
import { api } from '@/services/api';
import { useSettingsStore, type ThemeChoice } from '@/state/settingsStore';
import { useVaultStore } from '@/state/vaultStore';
import type { AppDirectories, TrashEntry, VaultSettings } from '@/types/domain';
import { asVaultPath } from '@/types/domain';

type Section =
  | 'general'
  | 'editor'
  | 'files'
  | 'appearance'
  | 'hotkeys'
  | 'templates'
  | 'dailyNotes'
  | 'trash'
  | 'plugins'
  | 'about';

const SECTIONS: { id: Section; label: string }[] = [
  { id: 'general', label: 'General' },
  { id: 'editor', label: 'Editor' },
  { id: 'files', label: 'Files and links' },
  { id: 'appearance', label: 'Appearance' },
  { id: 'hotkeys', label: 'Keyboard shortcuts' },
  { id: 'templates', label: 'Templates' },
  { id: 'dailyNotes', label: 'Daily notes' },
  { id: 'trash', label: 'Trash' },
  { id: 'plugins', label: 'Plugins' },
  { id: 'about', label: 'About' },
];

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const [section, setSection] = useState<Section>('general');

  return (
    <Modal title="Settings" onClose={onClose} size="large">
      <div className="ie-settings">
        <nav className="ie-settings__nav" aria-label="Settings sections">
          {SECTIONS.map((entry) => (
            <button
              key={entry.id}
              type="button"
              className={`ie-settings__nav-item${section === entry.id ? ' is-active' : ''}`}
              aria-current={section === entry.id}
              onClick={() => setSection(entry.id)}
            >
              {entry.label}
            </button>
          ))}
        </nav>
        <div className="ie-settings__body">
          <SettingsSection section={section} />
        </div>
      </div>
    </Modal>
  );
}

function SettingsSection({ section }: { section: Section }) {
  switch (section) {
    case 'general':
      return <GeneralSettings />;
    case 'editor':
      return <EditorSettings />;
    case 'files':
      return <FileSettings />;
    case 'appearance':
      return <AppearanceSettings />;
    case 'hotkeys':
      return <HotkeySettings />;
    case 'templates':
      return <TemplateSettings />;
    case 'dailyNotes':
      return <DailyNoteSettings />;
    case 'trash':
      return <TrashSettings />;
    case 'plugins':
      return <PluginSettings />;
    default:
      return <AboutSettings />;
  }
}

/**
 * One labelled setting.
 *
 * The whole row is a `<label>`, so the control inside is associated with the
 * text without every caller having to invent an id. A screen reader announces
 * "Theme, combo box" rather than an unnamed control.
 */
function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="ie-field">
      <span className="ie-field__label">
        <span>{label}</span>
        {hint ? <span className="ie-field__hint">{hint}</span> : null}
      </span>
      <span className="ie-field__control">{children}</span>
    </label>
  );
}

function GeneralSettings() {
  const settings = useSettingsStore();
  return (
    <>
      <h3>General</h3>
      <Field label="Reopen the last vault at launch">
        <input
          type="checkbox"
          checked={settings.general.reopenLastVault}
          onChange={(event) =>
            void settings.update('general', { reopenLastVault: event.target.checked })
          }
        />
      </Field>
      <Field label="Ask before moving a file to the trash">
        <input
          type="checkbox"
          checked={settings.general.confirmDelete}
          onChange={(event) => void settings.update('general', { confirmDelete: event.target.checked })}
        />
      </Field>
      <Field
        label="Keep trashed files for"
        hint="Zero keeps them until you empty the trash yourself."
      >
        <input
          className="ie-input"
          type="number"
          min={0}
          value={settings.general.trashRetentionDays}
          onChange={(event) =>
            void settings.update('general', { trashRetentionDays: Number(event.target.value) })
          }
        />
        <span className="ie-field__suffix">days</span>
      </Field>
    </>
  );
}

function EditorSettings() {
  const settings = useSettingsStore();
  const editor = settings.editor;
  return (
    <>
      <h3>Editor</h3>
      <Field label="Font size">
        <input
          className="ie-input"
          type="number"
          min={10}
          max={32}
          value={editor.fontSize}
          onChange={(event) => void settings.update('editor', { fontSize: Number(event.target.value) })}
        />
      </Field>
      <Field
        label="Live preview"
        hint="Hides Markdown symbols except on the line you are editing."
      >
        <input
          type="checkbox"
          checked={editor.livePreview}
          onChange={(event) => void settings.update('editor', { livePreview: event.target.checked })}
        />
      </Field>
      <Field label="Limit line width" hint="Keeps prose readable on a wide window.">
        <input
          type="checkbox"
          checked={editor.readableLineLength}
          onChange={(event) =>
            void settings.update('editor', { readableLineLength: event.target.checked })
          }
        />
      </Field>
      <Field label="Line numbers">
        <input
          type="checkbox"
          checked={editor.lineNumbers}
          onChange={(event) => void settings.update('editor', { lineNumbers: event.target.checked })}
        />
      </Field>
      <Field label="Check spelling">
        <input
          type="checkbox"
          checked={editor.spellcheck}
          onChange={(event) => void settings.update('editor', { spellcheck: event.target.checked })}
        />
      </Field>
      <Field label="Continue lists on Enter">
        <input
          type="checkbox"
          checked={editor.autoContinueLists}
          onChange={(event) =>
            void settings.update('editor', { autoContinueLists: event.target.checked })
          }
        />
      </Field>
      <Field label="Close brackets and quotes automatically">
        <input
          type="checkbox"
          checked={editor.autoCloseBrackets}
          onChange={(event) =>
            void settings.update('editor', { autoCloseBrackets: event.target.checked })
          }
        />
      </Field>
      <Field label="Indent size">
        <input
          className="ie-input"
          type="number"
          min={1}
          max={8}
          value={editor.tabSize}
          onChange={(event) => void settings.update('editor', { tabSize: Number(event.target.value) })}
        />
        <span className="ie-field__suffix">spaces</span>
      </Field>
      <Field label="Indent with spaces" hint="Turn off to indent with tab characters.">
        <input
          type="checkbox"
          checked={editor.indentWithSpaces}
          onChange={(event) =>
            void settings.update('editor', { indentWithSpaces: event.target.checked })
          }
        />
      </Field>
      <button type="button" className="ie-button" onClick={() => void settings.resetSection('editor')}>
        Reset editor settings
      </button>
    </>
  );
}

function FileSettings() {
  const vault = useVaultStore();
  const [draft, setDraft] = useState<VaultSettings | null>(vault.info?.settings ?? null);

  useEffect(() => {
    setDraft(vault.info?.settings ?? null);
  }, [vault.info]);

  const save = useCallback(
    async (next: VaultSettings) => {
      setDraft(next);
      try {
        await vault.updateSettings(next);
      } catch {
        setDraft(vault.info?.settings ?? null);
      }
    },
    [vault],
  );

  if (!draft) return <p>Open a vault to change its settings.</p>;

  return (
    <>
      <h3>Files and links</h3>
      <p className="ie-settings__note">
        These belong to the vault rather than to you, so they travel with the folder.
      </p>

      <Field
        label="Update links when a note is renamed"
        hint="Rewrites every reference so nothing breaks."
      >
        <input
          type="checkbox"
          checked={draft.updateLinksOnRename}
          onChange={(event) => void save({ ...draft, updateLinksOnRename: event.target.checked })}
        />
      </Field>

      <Field label="New links are written as">
        <select
          className="ie-input"
          value={draft.linkStyle}
          onChange={(event) =>
            void save({ ...draft, linkStyle: event.target.value as VaultSettings['linkStyle'] })
          }
        >
          <option value="shortestWikiLink">[[Note]] — shortest form that stays unambiguous</option>
          <option value="absoluteWikiLink">[[Folder/Note]] — always the full path</option>
          <option value="markdownLink">[Note](Folder/Note.md) — plain Markdown</option>
        </select>
      </Field>

      <Field label="New attachments go">
        <select
          className="ie-input"
          value={draft.attachments.mode}
          onChange={(event) => {
            const mode = event.target.value;
            void save({
              ...draft,
              attachments:
                mode === 'nextToNote'
                  ? { mode: 'nextToNote' }
                  : mode === 'subfolderOfNote'
                    ? { mode: 'subfolderOfNote', name: 'attachments' }
                    : { mode: 'vaultFolder', folder: asVaultPath('Attachments') },
            });
          }}
        >
          <option value="vaultFolder">In one folder for the whole vault</option>
          <option value="nextToNote">Beside the note that uses them</option>
          <option value="subfolderOfNote">In a subfolder beside the note</option>
        </select>
      </Field>

      {draft.attachments.mode === 'vaultFolder' ? (
        <Field label="Attachment folder">
          <input
            className="ie-input"
            value={draft.attachments.folder}
            onChange={(event) =>
              void save({
                ...draft,
                attachments: { mode: 'vaultFolder', folder: asVaultPath(event.target.value) },
              })
            }
          />
        </Field>
      ) : null}

      <Field label="Template folder">
        <input
          className="ie-input"
          value={draft.templatesFolder}
          onChange={(event) =>
            void save({ ...draft, templatesFolder: asVaultPath(event.target.value) })
          }
        />
      </Field>
    </>
  );
}

function AppearanceSettings() {
  const settings = useSettingsStore();
  return (
    <>
      <h3>Appearance</h3>
      <Field label="Theme">
        <select
          className="ie-input"
          value={settings.appearance.theme}
          onChange={(event) =>
            void settings.update('appearance', { theme: event.target.value as ThemeChoice })
          }
        >
          <option value="system">Follow the system</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </Field>
      <Field label="Interface size">
        <input
          type="range"
          min={80}
          max={140}
          value={Math.round(settings.appearance.uiScale * 100)}
          onChange={(event) =>
            void settings.update('appearance', { uiScale: Number(event.target.value) / 100 })
          }
        />
        <span className="ie-field__suffix">{Math.round(settings.appearance.uiScale * 100)}%</span>
      </Field>
      <Field label="Show the status bar">
        <input
          type="checkbox"
          checked={settings.appearance.showStatusBar}
          onChange={(event) =>
            void settings.update('appearance', { showStatusBar: event.target.checked })
          }
        />
      </Field>
      <Field label="Show tabs">
        <input
          type="checkbox"
          checked={settings.appearance.showTabBar}
          onChange={(event) => void settings.update('appearance', { showTabBar: event.target.checked })}
        />
      </Field>
    </>
  );
}

function HotkeySettings() {
  const settings = useSettingsStore();
  const [capturing, setCapturing] = useState<string | null>(null);
  const [version, setVersion] = useState(0);

  useEffect(() => commands.subscribe(() => setVersion((current) => current + 1)), []);

  const list = commands.list();
  void version;

  return (
    <>
      <h3>Keyboard shortcuts</h3>
      <p className="ie-settings__note">
        Combinations are written with <code>Mod</code>, which is Ctrl here and would be Cmd on a
        Mac.
      </p>
      <table className="ie-hotkeys">
        <tbody>
          {list.map((command) => {
            const current = settings.hotkeys[command.id] ?? command.defaultHotkey;
            const overridden = settings.hotkeys[command.id] !== undefined;
            return (
              <tr key={command.id}>
                <td className="ie-hotkeys__category">{command.category}</td>
                <td className="ie-hotkeys__name">{command.name}</td>
                <td className="ie-hotkeys__binding">
                  <button
                    type="button"
                    className={`ie-button ie-button--quiet${capturing === command.id ? ' is-capturing' : ''}`}
                    onClick={() => setCapturing(command.id)}
                    onKeyDown={(event) => {
                      if (capturing !== command.id) return;
                      event.preventDefault();
                      if (event.key === 'Escape') {
                        setCapturing(null);
                        return;
                      }
                      const combination = combinationFromEvent(event.nativeEvent);
                      if (!combination) return;
                      void settings.setHotkey(command.id, combination);
                      setCapturing(null);
                    }}
                    onBlur={() => setCapturing(null)}
                  >
                    {capturing === command.id
                      ? 'Press a combination…'
                      : current
                        ? formatCombination(current)
                        : 'Not set'}
                  </button>
                  {overridden ? (
                    <button
                      type="button"
                      className="ie-icon-button"
                      title="Restore the default"
                      aria-label={`Restore the default shortcut for ${command.name}`}
                      onClick={() => void settings.setHotkey(command.id, null)}
                    >
                      ↺
                    </button>
                  ) : null}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </>
  );
}

function TemplateSettings() {
  const [templates, setTemplates] = useState<{ path: string; name: string }[]>([]);
  useEffect(() => {
    api
      .listTemplates()
      .then(setTemplates)
      .catch(() => setTemplates([]));
  }, []);

  return (
    <>
      <h3>Templates</h3>
      <p className="ie-settings__note">
        A template is an ordinary note. These variables are replaced when it is inserted:
      </p>
      <dl className="ie-search__help">
        <dt>{'{{title}}'}</dt>
        <dd>the new note&apos;s name</dd>
        <dt>{'{{date}}'}</dt>
        <dd>today, as 2026-09-17</dd>
        <dt>{'{{time}}'}</dt>
        <dd>now, as 14:05</dd>
        <dt>{'{{date:dddd}}'}</dt>
        <dd>any format, such as Thursday</dd>
        <dt>{'{{yesterday}}'}</dt>
        <dd>and {'{{tomorrow}}'}, for daily-note links</dd>
      </dl>
      <h4>Templates in this vault</h4>
      {templates.length === 0 ? (
        <p>None yet.</p>
      ) : (
        <ul>
          {templates.map((template) => (
            <li key={template.path}>{template.name}</li>
          ))}
        </ul>
      )}
    </>
  );
}

function DailyNoteSettings() {
  const vault = useVaultStore();
  const [draft, setDraft] = useState(vault.info?.settings ?? null);

  useEffect(() => setDraft(vault.info?.settings ?? null), [vault.info]);
  if (!draft) return <p>Open a vault to change its settings.</p>;

  const save = (next: VaultSettings) => {
    setDraft(next);
    void vault.updateSettings(next);
  };

  return (
    <>
      <h3>Daily notes</h3>
      <Field label="Folder">
        <input
          className="ie-input"
          value={draft.dailyNotes.folder}
          onChange={(event) =>
            save({
              ...draft,
              dailyNotes: { ...draft.dailyNotes, folder: asVaultPath(event.target.value) },
            })
          }
        />
      </Field>
      <Field label="Filename format" hint="YYYY, MM, DD, dddd, HH, mm and ww are recognised.">
        <input
          className="ie-input"
          value={draft.dailyNotes.format}
          onChange={(event) =>
            save({ ...draft, dailyNotes: { ...draft.dailyNotes, format: event.target.value } })
          }
        />
      </Field>
      <Field label="Template" hint="Leave empty for a plain note with a heading.">
        <input
          className="ie-input"
          value={draft.dailyNotes.template ?? ''}
          placeholder="Templates/Daily.md"
          onChange={(event) =>
            save({
              ...draft,
              dailyNotes: {
                ...draft.dailyNotes,
                template: event.target.value ? asVaultPath(event.target.value) : null,
              },
            })
          }
        />
      </Field>
      <Field label="Open today's note when the vault opens">
        <input
          type="checkbox"
          checked={draft.dailyNotes.openOnStartup}
          onChange={(event) =>
            save({
              ...draft,
              dailyNotes: { ...draft.dailyNotes, openOnStartup: event.target.checked },
            })
          }
        />
      </Field>
    </>
  );
}

function TrashSettings() {
  const [entries, setEntries] = useState<TrashEntry[]>([]);

  const refresh = useCallback(() => {
    api
      .listTrash()
      .then(setEntries)
      .catch(() => setEntries([]));
  }, []);

  useEffect(refresh, [refresh]);

  return (
    <>
      <h3>Trash</h3>
      <p className="ie-settings__note">
        Deleted files move to a folder inside the vault, so they travel with it and can be
        recovered with any file manager.
      </p>
      {entries.length === 0 ? (
        <p>The trash is empty.</p>
      ) : (
        <>
          <ul className="ie-trash-list">
            {entries.map((entry) => (
              <li key={entry.id}>
                <span className="ie-trash-list__path">{entry.originalPath}</span>
                <span className="ie-trash-list__date">
                  {new Date(entry.trashedMs).toLocaleString()}
                </span>
                <button
                  type="button"
                  className="ie-button ie-button--quiet"
                  onClick={async () => {
                    const restored = await api.restoreFromTrash(entry.id);
                    notify('success', `Restored ${restored}.`);
                    refresh();
                  }}
                >
                  Restore
                </button>
                <button
                  type="button"
                  className="ie-button ie-button--quiet ie-button--danger"
                  onClick={async () => {
                    await api.purgeFromTrash(entry.id);
                    refresh();
                  }}
                >
                  Delete for good
                </button>
              </li>
            ))}
          </ul>
          <button
            type="button"
            className="ie-button ie-button--danger"
            onClick={async () => {
              const count = await api.emptyTrash();
              notify('info', `Removed ${count} ${count === 1 ? 'item' : 'items'}.`);
              refresh();
            }}
          >
            Empty the trash
          </button>
        </>
      )}
    </>
  );
}

function PluginSettings() {
  return (
    <>
      <h3>Plugins</h3>
      <p className="ie-settings__note">
        Plugins are JavaScript modules in the vault&apos;s <code>.inner-empire/plugins</code>{' '}
        folder. Each declares what it needs — reading notes, writing them, adding commands or
        panels — and calls outside that declaration are refused. Network access is off unless you
        grant it.
      </p>
      <p>
        The developer guide is in <code>docs/plugin-api/</code> in the repository.
      </p>
    </>
  );
}

function AboutSettings() {
  const vault = useVaultStore();
  const [directories, setDirectories] = useState<AppDirectories | null>(null);
  const [log, setLog] = useState('');

  useEffect(() => {
    api
      .appDirectories()
      .then(setDirectories)
      .catch(() => setDirectories(null));
  }, []);

  return (
    <>
      <h3>About</h3>
      <p>
        Inner Empire keeps your notes as ordinary Markdown files. The index in{' '}
        <code>.inner-empire</code> is a cache: delete it and the app rebuilds it from your files.
      </p>

      {vault.info ? (
        <>
          <h4>This vault</h4>
          <dl className="ie-about">
            <dt>Folder</dt>
            <dd>{vault.info.root}</dd>
            <dt>Files indexed</dt>
            <dd>{vault.info.fileCount.toLocaleString()}</dd>
            <dt>Filesystem</dt>
            <dd>
              {vault.info.caseSensitive
                ? 'Case-sensitive, so Note.md and note.md are different files'
                : 'Case-insensitive, so Note.md and note.md are the same file'}
            </dd>
          </dl>
          <button type="button" className="ie-button" onClick={() => void vault.rebuildIndex()}>
            Rebuild the index
          </button>
        </>
      ) : null}

      {directories ? (
        <>
          <h4>Where the app keeps its own files</h4>
          <dl className="ie-about">
            <dt>Settings</dt>
            <dd>{directories.config}</dd>
            <dt>Data</dt>
            <dd>{directories.data}</dd>
            <dt>Logs</dt>
            <dd>{directories.logs}</dd>
            <dt>Cache</dt>
            <dd>{directories.cache}</dd>
            <dt>Platform</dt>
            <dd>{directories.platform}</dd>
          </dl>
        </>
      ) : null}

      <h4>Diagnostics</h4>
      <button
        type="button"
        className="ie-button"
        onClick={async () => setLog(await api.readLogTail(200))}
      >
        Show recent log
      </button>
      {log ? <pre className="ie-log">{log}</pre> : null}
    </>
  );
}
