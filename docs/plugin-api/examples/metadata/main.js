/**
 * Reading the index and writing frontmatter.
 *
 * Note what this does *not* do: it never rewrites the note's body. Setting a
 * property replaces only the frontmatter block, so the prose is returned
 * byte-for-byte as the user left it.
 */

export default {
  onload(ctx) {
    ctx.app.commands.add({
      id: 'audit-current',
      name: 'Record this note’s link counts',
      run: async () => {
        const path = ctx.app.workspace.activeFile();
        if (!path) {
          ctx.app.ui.notify('warning', 'Open a note first.');
          return;
        }

        const [incoming, outgoing, note] = await Promise.all([
          ctx.app.metadata.backlinks(path),
          ctx.app.metadata.outgoingLinks(path),
          ctx.app.vault.read(path),
        ]);

        const unresolved = outgoing.filter((link) => link.targetPath === null).length;

        // Keep the properties the note already has, replacing only ours.
        const kept = note.metadata.properties.filter(
          (property) => !['inbound', 'outbound', 'unresolved'].includes(property.key),
        );

        await ctx.app.vault.setProperties(path, [
          ...kept,
          { key: 'inbound', value: { kind: 'number', value: incoming.length } },
          { key: 'outbound', value: { kind: 'number', value: outgoing.length } },
          { key: 'unresolved', value: { kind: 'number', value: unresolved } },
        ]);

        ctx.app.ui.notify(
          'success',
          `${incoming.length} in, ${outgoing.length} out, ${unresolved} pointing nowhere.`,
        );
      },
    });

    ctx.app.commands.add({
      id: 'find-orphans',
      name: 'List notes nothing links to',
      run: async () => {
        const results = await ctx.app.metadata.search('is:orphan', 500);
        ctx.app.ui.notify(
          'info',
          `${results.total} ${results.total === 1 ? 'note has' : 'notes have'} no incoming links.`,
        );
      },
    });
  },
};
