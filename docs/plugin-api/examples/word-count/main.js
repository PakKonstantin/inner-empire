/**
 * A status-bar item that follows the active note.
 *
 * Shows two things worth copying: reacting to events rather than polling, and
 * returning the element's own updater so the item can be refreshed without
 * being rebuilt.
 */

export default {
  onload(ctx) {
    let element = null;

    const update = () => {
      if (!element) return;
      const content = ctx.app.workspace.activeContent();
      if (content === null) {
        element.textContent = '';
        return;
      }
      const words = content.split(/\s+/).filter((token) => /[\p{L}\p{N}]/u.test(token)).length;
      element.textContent = `${words.toLocaleString()} words`;
    };

    ctx.app.ui.addStatusBarItem({
      id: 'count',
      render: (container) => {
        element = document.createElement('span');
        element.title = 'Words in the note you are editing';
        container.appendChild(element);
        update();
      },
    });

    // The count changes when the note changes and when a different note is
    // opened, so both are watched. Polling would be simpler and wrong.
    ctx.app.events.on('fileModified', update);
    ctx.app.events.on('activeFileChanged', update);

    // An interval catches edits that have not been saved yet, because those
    // raise no event. One second is often enough for a status bar and cheap.
    const timer = setInterval(update, 1000);
    ctx.register({ dispose: () => clearInterval(timer) });
  },
};
