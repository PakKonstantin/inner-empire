/**
 * The smallest useful plugin.
 *
 * It asks for one permission and uses it once. Everything else a plugin can do
 * is built from the same shape: declare what you need, receive an API bound to
 * that declaration, and register anything that must be undone later.
 */

export default {
  onload(ctx) {
    ctx.app.ui.notify('success', `Loaded, running on Inner Empire ${ctx.app.version}.`);
  },

  onunload() {
    // Nothing to clean up: the notice is transient and no handler was left
    // behind.
  },
};
