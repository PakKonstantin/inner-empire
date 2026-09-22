/**
 * Autocomplete inside the editor.
 *
 * Three sources, each triggered by what the user has just typed:
 * `[[` offers notes, `[[Note#` offers that note's headings and blocks, and
 * `#` offers tags. Every list comes from the index, so suggestions reflect the
 * whole vault without the editor holding any of it.
 */

import type { CompletionContext, CompletionResult } from '@codemirror/autocomplete';

import { api } from '@/services/api';
import type { VaultPath } from '@/types/domain';

export interface CompletionDeps {
  /** The note being edited, so heading completion knows where to look. */
  currentPath: () => VaultPath;
}

/** Notes, headings and blocks after `[[`. */
export function wikiLinkCompletion(deps: CompletionDeps) {
  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    // Match from the opening brackets to the cursor, refusing anything with a
    // closing bracket or a line break between.
    const match = context.matchBefore(/\[\[[^\]\n]*/);
    if (!match) return null;
    if (match.from === match.to && !context.explicit) return null;

    const typed = match.text.slice(2);
    const hash = typed.indexOf('#');

    if (hash !== -1) {
      return completeAnchor(deps, typed, hash, match.from);
    }

    const matches = await api.quickSwitch(typed, 30);
    return {
      from: match.from + 2,
      options: matches.map((hit) => ({
        label: linkTextFor(hit.path),
        detail: hit.path === linkTextFor(hit.path) ? undefined : hit.path,
        type: hit.kind === 'note' ? 'text' : 'variable',
        apply: (view, _completion, from, to) => {
          // Close the brackets as part of accepting, and put the cursor after
          // them, which is what the writer wants next.
          const insert = `${linkTextFor(hit.path)}]]`;
          view.dispatch({
            changes: { from, to, insert },
            selection: { anchor: from + insert.length },
          });
        },
      })),
      validFor: /^[^\]\n]*$/,
    };
  };
}

/** Headings and block ids after `[[Note#`. */
async function completeAnchor(
  deps: CompletionDeps,
  typed: string,
  hash: number,
  from: number,
): Promise<CompletionResult | null> {
  const targetText = typed.slice(0, hash);
  const fragment = typed.slice(hash + 1);
  const anchorFrom = from + 2 + hash + 1;

  // An empty target means the note being edited, which is how `[[#Heading]]`
  // works.
  const resolvedPath = targetText.trim()
    ? await resolveTargetPath(deps, targetText)
    : deps.currentPath();
  if (!resolvedPath) return null;

  if (fragment.startsWith('^')) {
    const blocks = await api.completeBlocks(resolvedPath, 50);
    return {
      from: anchorFrom + 1,
      options: blocks.map((id) => ({ label: id, type: 'constant' })),
      validFor: /^[^\]\n]*$/,
    };
  }

  const headings = await api.completeHeadings(resolvedPath, fragment, 50);
  return {
    from: anchorFrom,
    options: headings.map((heading) => ({ label: heading, type: 'property' })),
    validFor: /^[^\]\n]*$/,
  };
}

async function resolveTargetPath(
  deps: CompletionDeps,
  target: string,
): Promise<VaultPath | null> {
  try {
    const resolution = await api.resolveLink(deps.currentPath(), target);
    return resolution.outcome === 'resolved' ? resolution.path : null;
  } catch {
    return null;
  }
}

/** Tags after `#`. */
export function tagCompletion() {
  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    const match = context.matchBefore(/#[\p{L}\p{N}_/-]*/u);
    if (!match) return null;
    if (match.from === match.to && !context.explicit) return null;

    // A `#` at the start of a line followed by a space is a heading.
    const line = context.state.doc.lineAt(context.pos);
    if (match.from === line.from && /^#{1,6}\s/.test(line.text)) return null;

    const typed = match.text.slice(1);
    const tags = await api.completeTags(typed, 30);
    if (tags.length === 0) return null;

    return {
      from: match.from + 1,
      options: tags.map(([name, count]) => ({
        label: name,
        detail: `${count}`,
        type: 'keyword',
      })),
      validFor: /^[\p{L}\p{N}_/-]*$/u,
    };
  };
}

/**
 * The text a link to this path should use.
 *
 * Drops the `.md`, because `[[Folder/Note]]` is the conventional spelling and
 * the resolver accepts it.
 */
export function linkTextFor(path: VaultPath): string {
  const withoutExtension = path.replace(/\.(md|markdown)$/i, '');
  const lastSlash = withoutExtension.lastIndexOf('/');
  return lastSlash === -1 ? withoutExtension : withoutExtension.slice(lastSlash + 1);
}
