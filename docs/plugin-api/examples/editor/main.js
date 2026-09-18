/**
 * Changing the text being edited.
 *
 * `setActiveContent` writes to the open buffer rather than to disk, so the
 * change joins the user's undo history and is saved by the same autosave that
 * handles their typing. Writing the file directly would bypass both.
 */

const SMART_QUOTES = [
  [/(^|[\s([{])"/g, '$1“'],
  [/"/g, '”'],
  [/(^|[\s([{])'/g, '$1‘'],
  [/'/g, '’'],
];

export default {
  onload(ctx) {
    const transform = (name, change) =>
      ctx.app.commands.add({
        id: name.toLowerCase().replace(/\s+/g, '-'),
        name,
        run: () => {
          const content = ctx.app.workspace.activeContent();
          if (content === null) {
            ctx.app.ui.notify('warning', 'Open a note first.');
            return;
          }
          const updated = change(content);
          if (updated === content) {
            ctx.app.ui.notify('info', 'Nothing to change.');
            return;
          }
          ctx.app.workspace.setActiveContent(updated);
        },
      });

    transform('Tidy trailing whitespace', (content) =>
      content
        .split('\n')
        // Two trailing spaces are a Markdown line break, so they are kept.
        .map((line) => (line.endsWith('  ') ? line : line.replace(/\s+$/, '')))
        .join('\n'),
    );

    transform('Use curly quotes', (content) => {
      // Leave code blocks alone: a straight quote there is code, not prose.
      const parts = content.split(/(```[\s\S]*?```|`[^`\n]*`)/g);
      return parts
        .map((part, index) => {
          if (index % 2 === 1) return part;
          return SMART_QUOTES.reduce(
            (text, [pattern, replacement]) => text.replace(pattern, replacement),
            part,
          );
        })
        .join('');
    });

    transform('Collapse blank lines', (content) => content.replace(/\n{3,}/g, '\n\n'));
  },
};
