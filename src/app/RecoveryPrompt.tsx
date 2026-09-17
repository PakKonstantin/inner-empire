/**
 * Offering back work a crash interrupted.
 *
 * Shown once, after a vault opens, when the journal holds text the file does
 * not. Nothing is restored without being asked for, and where the file has
 * moved on since — someone edited it elsewhere, or a sync brought a newer
 * version — that is said plainly rather than being resolved silently.
 */

import { useCallback, useEffect, useState } from 'react';

import { Modal } from '@/components/Modal';
import { notify } from '@/components/Notifications';
import { api, type RecoveryCandidate } from '@/services/api';
import { useWorkspaceStore } from '@/state/workspaceStore';
import type { VaultPath } from '@/types/domain';
import { pathFileName } from '@/types/domain';

/** Entries older than this are dropped rather than offered. */
const KEEP_FOR_DAYS = 7;

export function RecoveryPrompt({ vaultReady }: { vaultReady: boolean }) {
  const [candidates, setCandidates] = useState<RecoveryCandidate[]>([]);
  const [dismissed, setDismissed] = useState(false);
  const [checked, setChecked] = useState(false);

  useEffect(() => {
    if (!vaultReady || checked) return;
    setChecked(true);

    void (async () => {
      try {
        const found = await api.recoverableNotes();
        // Only entries with something to offer: text the file does not
        // already hold.
        setCandidates(found.filter((candidate) => !candidate.alreadySaved));
      } catch {
        setCandidates([]);
      } finally {
        // Tidy up whatever is stale, whether or not anything was offered.
        void api.pruneJournal(KEEP_FOR_DAYS).catch(() => {});
      }
    })();
  }, [vaultReady, checked]);

  const restore = useCallback(async (candidate: RecoveryCandidate) => {
    try {
      await api.saveNote(candidate.path, candidate.content);
      await useWorkspaceStore.getState().openFile(candidate.path);
      notify('success', `Restored your unsaved changes to ${pathFileName(candidate.path)}.`);
    } catch (error) {
      notify(
        'error',
        `Could not restore ${pathFileName(candidate.path)}: ${describe(error)}`,
      );
    }
    setCandidates((current) => current.filter((entry) => entry.path !== candidate.path));
  }, []);

  const discard = useCallback(async (path: VaultPath) => {
    await api.clearJournal(path).catch(() => {});
    setCandidates((current) => current.filter((entry) => entry.path !== path));
  }, []);

  if (dismissed || candidates.length === 0) return null;

  return (
    <Modal
      title="Unsaved work from last time"
      description={
        candidates.length === 1
          ? 'One note had changes that were never written to disk. Your file is intact; this is the text that had not reached it yet.'
          : `${candidates.length} notes had changes that were never written to disk. Your files are intact; this is the text that had not reached them yet.`
      }
      size="large"
      onClose={() => setDismissed(true)}
      footer={
        <>
          <button type="button" className="ie-button" onClick={() => setDismissed(true)}>
            Decide later
          </button>
          <button
            type="button"
            className="ie-button ie-button--primary"
            onClick={async () => {
              for (const candidate of candidates.filter((entry) => !entry.fileIsNewer)) {
                await restore(candidate);
              }
            }}
          >
            Restore all
          </button>
        </>
      }
    >
      <ul className="ie-recovery">
        {candidates.map((candidate) => (
          <li key={candidate.path} className="ie-recovery__item">
            <div className="ie-recovery__head">
              <span className="ie-recovery__path">{candidate.path}</span>
              <span className="ie-recovery__when">
                {new Date(candidate.savedMs).toLocaleString()}
              </span>
            </div>

            {candidate.fileIsNewer ? (
              <p className="ie-recovery__warning">
                This note has changed since, so restoring would replace the newer version.
              </p>
            ) : null}

            <pre className="ie-recovery__preview">{preview(candidate.content)}</pre>

            <div className="ie-recovery__actions">
              <button
                type="button"
                className="ie-button ie-button--primary"
                onClick={() => void restore(candidate)}
              >
                {candidate.fileIsNewer ? 'Restore anyway' : 'Restore'}
              </button>
              <button
                type="button"
                className="ie-button"
                onClick={() => void discard(candidate.path)}
              >
                Discard
              </button>
            </div>
          </li>
        ))}
      </ul>
    </Modal>
  );
}

/** The opening of the recovered text, so the user can tell what it is. */
function preview(content: string): string {
  const lines = content.split('\n').slice(0, 8);
  const shown = lines.join('\n');
  return content.length > shown.length ? `${shown}\n…` : shown;
}

function describe(error: unknown): string {
  return error instanceof Object && 'message' in error ? String(error.message) : String(error);
}
