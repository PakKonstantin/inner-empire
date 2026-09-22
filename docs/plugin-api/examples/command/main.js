/**
 * Registering commands.
 *
 * Commands appear in the palette under the plugin's name and can suggest a
 * shortcut. The suggestion is only that: whatever the user sets in Settings
 * wins, so two plugins suggesting the same combination is not a conflict the
 * user cannot resolve.
 */

function timestamp() {
  const now = new Date();
  const pad = (value) => String(value).padStart(2, '0');
  return `${pad(now.getHours())}:${pad(now.getMinutes())}`;
}

export default {
  onload(ctx) {
    ctx.app.commands.add({
      id: 'timestamp',
      name: 'Insert the current time',
      hotkey: 'Mod+Shift+;',
      run: () => {
        ctx.app.workspace.insertAtCursor(timestamp());
      },
    });

    ctx.app.commands.add({
      id: 'capture',
      name: 'Capture a thought',
      run: async () => {
        const note = ctx.app.workspace.activeFile();
        if (!note) {
          ctx.app.ui.notify('warning', 'Open a note first.');
          return;
        }
        ctx.app.workspace.insertAtCursor(`\n- ${timestamp()} `);
      },
    });

    ctx.app.commands.add({
      id: 'inbox',
      name: 'Append to the inbox',
      run: async () => {
        const path = 'Inbox.md';
        const exists = await ctx.app.vault.exists(path);
        if (!exists) {
          await ctx.app.vault.create(path, '# Inbox\n\n');
        }
        const note = await ctx.app.vault.read(path);
        await ctx.app.vault.write(path, `${note.content}- ${timestamp()} New item\n`);
        ctx.app.ui.notify('success', 'Added to the inbox.');
      },
    });
  },
};
