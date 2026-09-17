/**
 * What a tab shows.
 *
 * The mode decides: an editor, a reading view, an image, a PDF, a canvas or
 * the graph. Keeping the choice here means a tab can switch views without the
 * pane knowing how any of them work.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';

import { MarkdownEditor } from '@/editor/MarkdownEditor';
import { GraphView, type GraphSettings } from '@/graph/GraphView';
import { renderMarkdown } from '@/markdown/renderer';
import { api } from '@/services/api';
import type { EditorPreferences } from '@/state/settingsStore';
import type { Buffer } from '@/state/workspaceStore';
import type { TabState, VaultPath } from '@/types/domain';
import { pathFileName } from '@/types/domain';

export interface PaneContentProps {
  tab: TabState;
  buffer: Buffer | undefined;
  loading: boolean;
  preferences: EditorPreferences;
  graphSettings: GraphSettings;
  onGraphSettingsChange: (settings: GraphSettings) => void;
  activePath: VaultPath | null;
  onEdit: (content: string) => void;
  onSave: () => void;
  onCursorChange: (position: { line: number; column: number; offset: number }) => void;
  onFollowLink: (target: string, newPane: boolean) => void;
  onFollowTag: (tag: string) => void;
  onOpen: (path: VaultPath, options?: { newPane?: boolean }) => void;
  /** Which link targets exist, so unresolved links look different. */
  resolvedTargets: Set<string>;
  assetUrl: (target: string) => string | null;
}

export function PaneContent(props: PaneContentProps) {
  const { tab, buffer } = props;

  const isResolved = useCallback(
    (target: string) => props.resolvedTargets.has(target.toLowerCase()),
    [props.resolvedTargets],
  );

  switch (tab.mode) {
    case 'graph':
      return (
        <GraphView
          activePath={props.activePath}
          settings={props.graphSettings}
          onSettingsChange={props.onGraphSettingsChange}
          onOpen={props.onOpen}
        />
      );

    case 'image':
      return <ImageViewer path={tab.path} assetUrl={props.assetUrl} />;

    case 'pdf':
      return <PdfViewer path={tab.path} assetUrl={props.assetUrl} />;

    case 'canvas':
      return <CanvasPlaceholder path={tab.path} />;

    case 'read':
      return (
        <ReadingView
          path={tab.path}
          content={buffer?.content ?? ''}
          loading={props.loading}
          onFollowLink={props.onFollowLink}
          onFollowTag={props.onFollowTag}
          assetUrl={props.assetUrl}
        />
      );

    default:
      if (props.loading && !buffer) {
        return <div className="ie-pane__loading">Opening {pathFileName(tab.path)}…</div>;
      }
      if (!buffer) {
        return <div className="ie-empty">This note could not be opened.</div>;
      }
      return (
        <MarkdownEditor
          path={tab.path}
          content={buffer.content}
          preferences={props.preferences}
          initialCursor={tab.cursorOffset}
          onChange={props.onEdit}
          onSave={props.onSave}
          onCursorChange={props.onCursorChange}
          onFollowLink={props.onFollowLink}
          onFollowTag={props.onFollowTag}
          isResolved={isResolved}
          resolveAsset={props.assetUrl}
          placeholder="Start writing. Link to another note with [[double brackets]]."
        />
      );
  }
}

interface ReadingViewProps {
  path: VaultPath;
  content: string;
  loading: boolean;
  onFollowLink: (target: string, newPane: boolean) => void;
  onFollowTag: (tag: string) => void;
  assetUrl: (target: string) => string | null;
}

function ReadingView(props: ReadingViewProps) {
  const rendered = useMemo(
    () =>
      renderMarkdown(props.content, {
        path: props.path,
        onFollowLink: (target, options) => props.onFollowLink(target, options.newPane ?? false),
        onFollowTag: props.onFollowTag,
        onFollowExternal: (url) => void api.openExternal(url),
        resolveAsset: props.assetUrl,
      }),
    [props],
  );

  if (props.loading) return <div className="ie-pane__loading">Opening…</div>;

  return (
    <div className="ie-reading">
      <article className="ie-reading__body">{rendered}</article>
    </div>
  );
}

function ImageViewer({
  path,
  assetUrl,
}: {
  path: VaultPath;
  assetUrl: (target: string) => string | null;
}) {
  const [zoom, setZoom] = useState(1);
  const source = assetUrl(path);

  if (!source) return <div className="ie-empty">This image could not be loaded.</div>;

  return (
    <div className="ie-viewer">
      <div className="ie-viewer__toolbar ie-chrome">
        <button type="button" className="ie-button ie-button--quiet" onClick={() => setZoom((z) => Math.max(0.1, z / 1.25))}>
          Zoom out
        </button>
        <span>{Math.round(zoom * 100)}%</span>
        <button type="button" className="ie-button ie-button--quiet" onClick={() => setZoom((z) => Math.min(8, z * 1.25))}>
          Zoom in
        </button>
        <button type="button" className="ie-button ie-button--quiet" onClick={() => setZoom(1)}>
          Reset
        </button>
      </div>
      <div className="ie-viewer__surface">
        <img src={source} alt={pathFileName(path)} style={{ transform: `scale(${zoom})` }} />
      </div>
    </div>
  );
}

/**
 * The PDF viewer.
 *
 * The webview on both platforms renders PDFs natively, so an `<object>` gives
 * page navigation, zoom and text selection without shipping a renderer. The
 * toolbar adds the zoom control that the embedded viewer does not expose
 * consistently across WebKitGTK and WebView2.
 */
function PdfViewer({
  path,
  assetUrl,
}: {
  path: VaultPath;
  assetUrl: (target: string) => string | null;
}) {
  const [page, setPage] = useState(1);
  const [zoom, setZoom] = useState(100);
  const source = assetUrl(path);

  if (!source) return <div className="ie-empty">This document could not be loaded.</div>;

  // The fragment is the standard PDF open parameter set; both engines honour
  // page and zoom.
  const url = `${source}#page=${page}&zoom=${zoom}`;

  return (
    <div className="ie-viewer">
      <div className="ie-viewer__toolbar ie-chrome">
        <button
          type="button"
          className="ie-button ie-button--quiet"
          onClick={() => setPage((current) => Math.max(1, current - 1))}
        >
          Previous page
        </button>
        <label className="ie-viewer__page">
          Page
          <input
            className="ie-input"
            type="number"
            min={1}
            value={page}
            onChange={(event) => setPage(Math.max(1, Number(event.target.value) || 1))}
          />
        </label>
        <button
          type="button"
          className="ie-button ie-button--quiet"
          onClick={() => setPage((current) => current + 1)}
        >
          Next page
        </button>
        <span className="ie-viewer__spacer" />
        <button
          type="button"
          className="ie-button ie-button--quiet"
          onClick={() => setZoom((current) => Math.max(25, current - 25))}
        >
          Zoom out
        </button>
        <span>{zoom}%</span>
        <button
          type="button"
          className="ie-button ie-button--quiet"
          onClick={() => setZoom((current) => Math.min(400, current + 25))}
        >
          Zoom in
        </button>
      </div>
      <object
        key={url}
        className="ie-viewer__pdf"
        data={url}
        type="application/pdf"
        aria-label={pathFileName(path)}
      >
        <p>
          This document cannot be shown here.{' '}
          <button type="button" className="ie-button" onClick={() => void api.revealInFileManager(path)}>
            Show it in the file manager
          </button>
        </p>
      </object>
    </div>
  );
}

function CanvasPlaceholder({ path }: { path: VaultPath }) {
  const [ready, setReady] = useState(false);
  useEffect(() => setReady(true), []);
  return (
    <div className="ie-empty">
      {ready ? `Canvas: ${pathFileName(path)}` : ''}
    </div>
  );
}
