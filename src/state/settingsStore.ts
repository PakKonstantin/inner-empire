/**
 * Application preferences: things that belong to the person, not to the vault.
 *
 * Theme, hotkeys and editor behaviour live in the OS config directory so they
 * follow the user between vaults. Anything that describes a *vault* — where
 * attachments go, the daily-note format — lives in the vault instead, and is
 * handled by `vaultStore`.
 */

import { create } from 'zustand';

import { api } from '@/services/api';

export type ThemeChoice = 'system' | 'light' | 'dark';

export interface EditorPreferences {
  fontSize: number;
  /** Cap the text column so long lines stay readable on a wide window. */
  readableLineLength: boolean;
  lineNumbers: boolean;
  spellcheck: boolean;
  /** Conceal Markdown syntax on lines the cursor is not on. */
  livePreview: boolean;
  /** Insert the next bullet or number when Enter is pressed in a list. */
  autoContinueLists: boolean;
  /** Close brackets, quotes and Markdown emphasis as they are typed. */
  autoCloseBrackets: boolean;
  /** Spaces per indent level. */
  tabSize: number;
  /** Indent with spaces rather than a tab character. */
  indentWithSpaces: boolean;
  /** Show whitespace and line-ending markers. */
  showInvisibles: boolean;
}

export interface AppearancePreferences {
  theme: ThemeChoice;
  /** Interface scale, as a multiplier. */
  uiScale: number;
  /** A CSS file in `.inner-empire/themes/` to layer on top of the base theme. */
  customTheme: string | null;
  showStatusBar: boolean;
  showTabBar: boolean;
}

export interface GeneralPreferences {
  /** Reopen the last vault at launch. */
  reopenLastVault: boolean;
  /** Confirm before moving a file to the trash. */
  confirmDelete: boolean;
  /** Days before a trashed file is purged. Zero keeps it forever. */
  trashRetentionDays: number;
}

export interface AppSettings {
  general: GeneralPreferences;
  appearance: AppearancePreferences;
  editor: EditorPreferences;
  /** Command id to key combination, overriding the default. */
  hotkeys: Record<string, string>;
}

export const DEFAULT_SETTINGS: AppSettings = {
  general: {
    reopenLastVault: true,
    confirmDelete: true,
    trashRetentionDays: 30,
  },
  appearance: {
    theme: 'system',
    uiScale: 1,
    customTheme: null,
    showStatusBar: true,
    showTabBar: true,
  },
  editor: {
    fontSize: 16,
    readableLineLength: true,
    lineNumbers: false,
    spellcheck: true,
    livePreview: true,
    autoContinueLists: true,
    autoCloseBrackets: true,
    tabSize: 2,
    indentWithSpaces: true,
    showInvisibles: false,
  },
  hotkeys: {},
};

interface SettingsState extends AppSettings {
  loaded: boolean;
  load: () => Promise<void>;
  update: <K extends keyof AppSettings>(section: K, patch: Partial<AppSettings[K]>) => Promise<void>;
  setHotkey: (commandId: string, combination: string | null) => Promise<void>;
  resetSection: <K extends keyof AppSettings>(section: K) => Promise<void>;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  ...DEFAULT_SETTINGS,
  loaded: false,

  async load() {
    try {
      const stored = await api.loadAppSettings();
      set({ ...mergeSettings(stored), loaded: true });
    } catch {
      // A missing or damaged settings file means defaults, not a failure to
      // start: preferences are a convenience, not data the user would miss.
      set({ ...DEFAULT_SETTINGS, loaded: true });
    }
    applyAppearance(get());
  },

  async update(section, patch) {
    set((state) => ({ [section]: { ...state[section], ...patch } }) as Partial<SettingsState>);
    applyAppearance(get());
    await persist(get());
  },

  async setHotkey(commandId, combination) {
    set((state) => {
      const hotkeys = { ...state.hotkeys };
      if (combination === null) delete hotkeys[commandId];
      else hotkeys[commandId] = combination;
      return { hotkeys };
    });
    await persist(get());
  },

  async resetSection(section) {
    set({ [section]: DEFAULT_SETTINGS[section] } as Partial<SettingsState>);
    applyAppearance(get());
    await persist(get());
  },
}));

async function persist(state: AppSettings): Promise<void> {
  try {
    await api.saveAppSettings({
      general: state.general,
      appearance: state.appearance,
      editor: state.editor,
      hotkeys: state.hotkeys,
    });
  } catch {
    // Settings that fail to save are still applied for this session; the next
    // change retries.
  }
}

/**
 * Merge a stored settings object over the defaults.
 *
 * Per-section rather than wholesale, so a settings file written by an older
 * build — missing a section added since — still loads with sensible values
 * instead of leaving that section undefined.
 */
export function mergeSettings(stored: Record<string, unknown>): AppSettings {
  const section = <K extends keyof AppSettings>(key: K): AppSettings[K] => {
    const value = stored[key];
    if (typeof value !== 'object' || value === null) return DEFAULT_SETTINGS[key];
    return { ...DEFAULT_SETTINGS[key], ...(value as object) } as AppSettings[K];
  };
  return {
    general: section('general'),
    appearance: section('appearance'),
    editor: section('editor'),
    hotkeys: typeof stored.hotkeys === 'object' && stored.hotkeys !== null
      ? (stored.hotkeys as Record<string, string>)
      : {},
  };
}

/** Which theme `system` currently resolves to. */
export function systemTheme(): 'light' | 'dark' {
  if (typeof window === 'undefined' || !window.matchMedia) return 'dark';
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

export function resolveTheme(choice: ThemeChoice): 'light' | 'dark' {
  return choice === 'system' ? systemTheme() : choice;
}

/** Push appearance settings onto the document. */
export function applyAppearance(settings: AppSettings): void {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  root.dataset.theme = resolveTheme(settings.appearance.theme);
  root.style.setProperty('--font-size-editor', `${settings.editor.fontSize}px`);
  root.style.fontSize = `${Math.round(16 * settings.appearance.uiScale)}px`;
}

/** Re-apply the theme when the OS switches, while the choice is `system`. */
export function watchSystemTheme(): () => void {
  if (typeof window === 'undefined' || !window.matchMedia) return () => {};
  const media = window.matchMedia('(prefers-color-scheme: light)');
  const handler = () => {
    const state = useSettingsStore.getState();
    if (state.appearance.theme === 'system') applyAppearance(state);
  };
  media.addEventListener('change', handler);
  return () => media.removeEventListener('change', handler);
}
