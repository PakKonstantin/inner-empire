/**
 * The status bar.
 *
 * Deliberately quiet: counts, position, and the save state — the facts a
 * writer glances at, not a place for the app to talk about itself.
 */

import { useEffect, useState } from 'react';

import { events } from '@/services/events';
import type { Buffer } from '@/state/workspaceStore';
import type { VaultPath } from '@/types/domain';

export interface StatusBarProps {
  path: VaultPath | null;
  buffer: Buffer | undefined;
  cursor: { line: number; column: number };
  indexing: { scanned: number; total: number | null; done: boolean };
  vaultName: string | null;
  fileCount: number;
  onOpenDiagnostics: () => void;
  diagnosticCount: number;
}

export function StatusBar(props: StatusBarProps) {
  const [transient, setTransient] = useState<string | null>(null);

  useEffect(() => {
    const off = events.on('indexUpdated', ({ indexed, removed }) => {
      if (indexed + removed === 0) return;
      setTransient(`Indexed ${indexed} changed ${indexed === 1 ? 'file' : 'files'}`);
      const timer = setTimeout(() => setTransient(null), 2000);
      return () => clearTimeout(timer);
    });
    return off;
  }, []);

  const words = props.buffer ? countWords(props.buffer.content) : 0;
  const characters = props.buffer?.content.length ?? 0;

  return (
    <footer className="ie-statusbar ie-chrome">
      <div className="ie-statusbar__left">
        {props.vaultName ? <span title="Open vault">{props.vaultName}</span> : null}
        <span title="Files in the index">{props.fileCount.toLocaleString()} files</span>
        {props.diagnosticCount > 0 ? (
          <button
            type="button"
            className="ie-statusbar__warning"
            onClick={props.onOpenDiagnostics}
            title="Problems found while scanning"
          >
            {props.diagnosticCount} {props.diagnosticCount === 1 ? 'notice' : 'notices'}
          </button>
        ) : null}
      </div>

      <div className="ie-statusbar__center">
        {!props.indexing.done ? (
          <span className="ie-statusbar__progress">
            Indexing {props.indexing.scanned.toLocaleString()}
            {props.indexing.total ? ` of ${props.indexing.total.toLocaleString()}` : ''}…
          </span>
        ) : (
          transient
        )}
      </div>

      <div className="ie-statusbar__right">
        {props.path ? (
          <>
            <span title="Words in this note">{words.toLocaleString()} words</span>
            <span title="Characters in this note">{characters.toLocaleString()} characters</span>
            <span title="Cursor position">
              Line {props.cursor.line}, column {props.cursor.column}
            </span>
            <span className="ie-statusbar__save" title={saveTitle(props.buffer)}>
              {saveLabel(props.buffer)}
            </span>
          </>
        ) : (
          <span>No note open</span>
        )}
      </div>
    </footer>
  );
}

function saveLabel(buffer: Buffer | undefined): string {
  if (!buffer) return '';
  if (buffer.error) return 'Not saved';
  if (buffer.saving) return 'Saving…';
  return buffer.dirty ? 'Unsaved' : 'Saved';
}

function saveTitle(buffer: Buffer | undefined): string {
  if (buffer?.error) return buffer.error;
  if (buffer?.dirty) return 'Changes are written automatically a moment after you stop typing.';
  return 'Everything is written to disk.';
}

function countWords(text: string): number {
  return text.split(/\s+/).filter((token) => /[\p{L}\p{N}]/u.test(token)).length;
}
