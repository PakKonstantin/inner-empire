/**
 * Full-text search.
 *
 * The query goes to the backend exactly as typed and is parsed there, so the
 * language the help text describes is the one that actually runs. Results
 * arrive with their snippets already highlighted by SQLite.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { VirtualList } from '@/components/VirtualList';
import { api } from '@/services/api';
import type { SearchHit, SearchResults, VaultPath } from '@/types/domain';
import { EmptyState, ErrorState, IconButton, SearchInput, Skeleton, Tooltip } from '@/ui';

const ROW_HEIGHT = 62;
const DEBOUNCE_MS = 180;

export interface SearchPanelProps {
  /** Pre-fill the box, used when a tag is clicked elsewhere. */
  initialQuery?: string;
  onOpen: (path: VaultPath, options?: { newPane?: boolean }) => void;
}

export function SearchPanel({ initialQuery, onOpen }: SearchPanelProps) {
  const [query, setQuery] = useState(initialQuery ?? '');
  const [results, setResults] = useState<SearchResults | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [searching, setSearching] = useState(false);
  const [showHelp, setShowHelp] = useState(false);
  const input = useRef<HTMLInputElement | null>(null);
  const requestId = useRef(0);

  useEffect(() => {
    if (initialQuery === undefined) return;
    setQuery(initialQuery);
    input.current?.focus();
  }, [initialQuery]);

  const run = useCallback(async (text: string) => {
    const id = (requestId.current += 1);
    if (!text.trim()) {
      setResults(null);
      setError(null);
      return;
    }

    setSearching(true);
    try {
      const found = await api.searchVault(text, 200);
      // Ignore a response that a newer keystroke has already superseded.
      if (id !== requestId.current) return;
      setResults(found);
      setError(null);
    } catch (caught) {
      if (id !== requestId.current) return;
      setResults(null);
      setError(caught instanceof Object && 'message' in caught ? String(caught.message) : String(caught));
    } finally {
      if (id === requestId.current) setSearching(false);
    }
  }, []);

  // Debounced, so typing a word does not run one search per letter.
  useEffect(() => {
    const timer = setTimeout(() => void run(query), DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [query, run]);

  const hits = results?.hits ?? [];

  return (
    <div className="ie-panel ie-search">
      <div className="ie-panel-header">
        <span>Search</span>
        <Tooltip content="Query syntax">
          <IconButton
            icon="help"
            label="Query syntax"
            size="sm"
            pressed={showHelp}
            onClick={() => setShowHelp((current) => !current)}
          />
        </Tooltip>
      </div>

      <div className="ie-explorer__controls">
        <SearchInput
          ref={input}
          label="Search notes"
          placeholder="Search notes"
          value={query}
          onValueChange={setQuery}
          aria-invalid={error !== null}
        />
      </div>

      {showHelp ? (
        <dl className="ie-search__help">
          <dt>"exact phrase"</dt>
          <dd>only this sequence of words</dd>
          <dt>-word</dt>
          <dd>notes without it</dd>
          <dt>tag:AI</dt>
          <dd>tagged #AI, including nested tags</dd>
          <dt>path:Projects</dt>
          <dd>inside a folder</dd>
          <dt>file:readme</dt>
          <dd>by filename</dd>
          <dt>status:active</dt>
          <dd>a frontmatter property</dd>
          <dt>rating&gt;7</dt>
          <dd>compares numerically</dd>
          <dt>section:"Design notes"</dt>
          <dd>notes with a matching heading</dd>
          <dt>is:orphan</dt>
          <dd>also unresolved, untagged, dead-end</dd>
        </dl>
      ) : null}

      {error ? (
        <div className="ie-search__error">
          <ErrorState
            compact
            title="That query could not be read"
            description="Fix the part below and the search runs again as you type."
            detail={error}
          />
        </div>
      ) : null}

      {results ? (
        <div className="ie-search__summary">
          {results.total === 0
            ? 'No matches.'
            : `${results.total} ${results.total === 1 ? 'match' : 'matches'}${
                results.truncated ? `, showing the first ${hits.length}` : ''
              }`}
        </div>
      ) : null}

      <VirtualList
        className="ie-search__results"
        items={hits}
        rowHeight={ROW_HEIGHT}
        keyOf={(hit) => hit.path}
        renderRow={(hit) => <SearchResultRow hit={hit} onOpen={onOpen} />}
        emptyState={
          searching ? (
            <div className="ie-panel__loading" aria-busy="true" aria-live="polite">
              <span className="sr-only">Searching</span>
              <Skeleton height={16} width="60%" />
              <Skeleton height={12} width="85%" />
              <Skeleton height={16} width="45%" />
              <Skeleton height={12} width="75%" />
            </div>
          ) : error ? null : query.trim() ? (
            <EmptyState
              compact
              icon="search"
              title="No matches"
              description="No note in this vault matches that query."
              action={{ label: 'Show the query syntax', onClick: () => setShowHelp(true) }}
            />
          ) : (
            <EmptyState
              compact
              icon="search"
              title="Search the vault"
              description={'Type to search every note. Words, "exact phrases", tag:, path: and property comparisons all work.'}
              action={{ label: 'Show the query syntax', onClick: () => setShowHelp(true) }}
            />
          )
        }
      />
    </div>
  );
}

function SearchResultRow({
  hit,
  onOpen,
}: {
  hit: SearchHit;
  onOpen: (path: VaultPath, options?: { newPane?: boolean }) => void;
}) {
  return (
    <button
      type="button"
      className="ie-search-result"
      onClick={(event) => onOpen(hit.path, { newPane: event.ctrlKey || event.metaKey })}
      title={hit.path}
    >
      <span className="ie-search-result__title">{hit.title}</span>
      <span className="ie-search-result__path">{hit.path}</span>
      {hit.snippet ? (
        <span
          className="ie-search-result__snippet"
          // The only markup here is the <mark> pair SQLite's snippet function
          // added; it escaped the note text around them before returning it.
          dangerouslySetInnerHTML={{ __html: hit.snippet }}
        />
      ) : null}
    </button>
  );
}
