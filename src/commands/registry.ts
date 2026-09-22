/**
 * The command registry.
 *
 * Everything the user can do has an id, a name and a function, and everything
 * that can trigger it — the palette, a hotkey, a menu item, a plugin — goes
 * through this one table. That is what makes "let plugins add commands" a
 * one-line capability rather than a new subsystem, and what lets the hotkey
 * editor list every action without a hard-coded catalogue.
 */

export interface Command {
  id: string;
  /** Shown in the palette. */
  name: string;
  /** Groups commands in the palette and the hotkey editor. */
  category: string;
  /** Default key combination, e.g. `Mod+P`. `Mod` is Ctrl, or Cmd on macOS. */
  defaultHotkey?: string;
  /** Whether the command applies right now. A command that is not available is
   *  hidden from the palette rather than shown and then failing. */
  isAvailable?: () => boolean;
  run: () => void | Promise<void>;
  /** Set for commands a plugin registered, so they can be removed on unload. */
  source?: string;
}

type Listener = () => void;

class CommandRegistry {
  private commands = new Map<string, Command>();
  private listeners = new Set<Listener>();

  register(command: Command): () => void {
    if (this.commands.has(command.id)) {
      console.warn(`The command "${command.id}" was registered twice; the later one wins.`);
    }
    this.commands.set(command.id, command);
    this.notify();
    return () => this.unregister(command.id);
  }

  registerAll(commands: Command[]): () => void {
    const offs = commands.map((command) => this.register(command));
    return () => offs.forEach((off) => off());
  }

  unregister(id: string): void {
    if (this.commands.delete(id)) this.notify();
  }

  /** Remove everything a given plugin registered. */
  unregisterSource(source: string): void {
    let changed = false;
    for (const [id, command] of this.commands) {
      if (command.source === source) {
        this.commands.delete(id);
        changed = true;
      }
    }
    if (changed) this.notify();
  }

  get(id: string): Command | undefined {
    return this.commands.get(id);
  }

  /** Every command, sorted by category then name. */
  list(): Command[] {
    return [...this.commands.values()].sort(
      (a, b) => a.category.localeCompare(b.category) || a.name.localeCompare(b.name),
    );
  }

  available(): Command[] {
    return this.list().filter((command) => command.isAvailable?.() ?? true);
  }

  /**
   * Run a command by id.
   *
   * A command that throws is reported and contained: one misbehaving action,
   * especially one a plugin registered, must not take the window down.
   */
  async execute(id: string): Promise<boolean> {
    const command = this.commands.get(id);
    if (!command) {
      console.warn(`No command with the id "${id}".`);
      return false;
    }
    if (command.isAvailable && !command.isAvailable()) return false;
    try {
      await command.run();
      return true;
    } catch (error) {
      console.error(`The command "${id}" failed:`, error);
      return false;
    }
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private notify(): void {
    for (const listener of [...this.listeners]) {
      try {
        listener();
      } catch (error) {
        console.error('A command-registry listener threw:', error);
      }
    }
  }
}

export const commands = new CommandRegistry();
export type { CommandRegistry };
