/**
 * A sidebar panel.
 *
 * `render` is handed an element to fill and may return a cleanup function,
 * which runs when the panel is hidden or the plugin unloads. Building the DOM
 * by hand keeps the plugin free of a framework dependency.
 */

export default {
  async onload(ctx) {
    ctx.app.ui.addPanel({
      id: 'tags',
      label: 'Tag Browser',
      icon: '◆',
      side: 'right',
      render: (container) => {
        container.classList.add('ie-panel');

        const heading = document.createElement('div');
        heading.className = 'ie-panel-header';
        heading.textContent = 'Notes by tag';
        container.appendChild(heading);

        const select = document.createElement('select');
        select.className = 'ie-input';
        container.appendChild(select);

        const list = document.createElement('div');
        list.className = 'ie-panel__body';
        container.appendChild(list);

        const showNotes = async (tag) => {
          list.replaceChildren();
          if (!tag) return;
          const files = await ctx.app.metadata.filesWithTag(tag);
          for (const file of files) {
            const button = document.createElement('button');
            button.type = 'button';
            button.className = 'ie-outline__item';
            button.textContent = file.title;
            button.addEventListener('click', () => {
              void ctx.app.workspace.openFile(file.path);
            });
            list.appendChild(button);
          }
        };

        select.addEventListener('change', () => {
          void ctx.app.settings.set('lastTag', select.value);
          void showNotes(select.value);
        });

        void ctx.app.metadata.tags().then((tags) => {
          const remembered = ctx.app.settings.get('lastTag', '');
          for (const tag of tags) {
            const option = document.createElement('option');
            option.value = tag.name;
            option.textContent = `${tag.name} (${tag.totalCount})`;
            select.appendChild(option);
          }
          if (remembered) select.value = remembered;
          void showNotes(select.value);
        });

        // Returned cleanup: the listeners go with the element, but anything
        // with a life of its own would be stopped here.
        return () => {
          container.replaceChildren();
        };
      },
    });
  },
};
