/**
 * The screen shown when no vault is open.
 *
 * A vault is an ordinary folder, and this says so: there is no import step and
 * no proprietary container, so the two choices are "point at a folder" and
 * "make a new one".
 */

import { useEffect, useState } from 'react';

import { notify } from '@/components/Notifications';
import { pickFolder } from '@/services/dialogs';
import { useVaultStore } from '@/state/vaultStore';

export function VaultChooser() {
  const { recents, status, error, loadRecents, open, create, forget, dismissError } = useVaultStore();
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void loadRecents();
  }, [loadRecents]);

  const openFolder = async () => {
    const folder = await pickFolder({ title: 'Open a vault' });
    if (!folder) return;
    setBusy(true);
    await open(folder);
    setBusy(false);
  };

  const createFolder = async () => {
    const parent = await pickFolder({ title: 'Choose where to put the new vault' });
    if (!parent) return;
    const name = window.prompt('Name for the new vault', 'My Vault');
    if (!name?.trim()) return;

    setBusy(true);
    const separator = parent.includes('\\') ? '\\' : '/';
    const created = await create(`${parent}${separator}${name.trim()}`, name.trim());
    setBusy(false);
    if (created) notify('success', `Created ${name.trim()}.`);
  };

  return (
    <div className="ie-welcome">
      <div className="ie-welcome__panel">
        <h1 className="ie-welcome__title">Inner Empire</h1>
        <p className="ie-welcome__subtitle">
          A vault is an ordinary folder of Markdown files. Open one you already have, or start a
          new one. Nothing is copied into a database and nothing leaves your machine.
        </p>

        <div className="ie-welcome__actions">
          <button
            type="button"
            className="ie-button ie-button--primary"
            onClick={openFolder}
            disabled={busy || status === 'opening'}
          >
            Open a folder
          </button>
          <button type="button" className="ie-button" onClick={createFolder} disabled={busy}>
            Create a vault
          </button>
        </div>

        {error ? (
          <div className="ie-welcome__error" role="alert">
            <span>{error}</span>
            <button type="button" className="ie-icon-button" onClick={dismissError} aria-label="Dismiss">
              ✕
            </button>
          </div>
        ) : null}

        {recents.length > 0 ? (
          <>
            <h2 className="ie-welcome__section">Recent</h2>
            <ul className="ie-welcome__recents">
              {recents.map((vault) => (
                <li key={vault.path}>
                  <button
                    type="button"
                    className="ie-welcome__recent"
                    onClick={() => void open(vault.path)}
                    disabled={busy}
                  >
                    <span className="ie-welcome__recent-name">{vault.name}</span>
                    <span className="ie-welcome__recent-path">{vault.path}</span>
                  </button>
                  <button
                    type="button"
                    className="ie-icon-button"
                    aria-label={`Forget ${vault.name}`}
                    title="Remove from this list"
                    onClick={() => void forget(vault.path)}
                  >
                    ✕
                  </button>
                </li>
              ))}
            </ul>
          </>
        ) : null}
      </div>
    </div>
  );
}
