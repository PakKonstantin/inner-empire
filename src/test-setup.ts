// Vitest runs in jsdom, which has no ResizeObserver or matchMedia.
// Both are used for layout and theming, so they are stubbed rather than
// guarded at every call site.

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

globalThis.ResizeObserver ??= ResizeObserverStub as unknown as typeof ResizeObserver;

if (typeof window !== 'undefined' && !window.matchMedia) {
  window.matchMedia = ((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  })) as typeof window.matchMedia;
}

export {};

// CodeMirror measures layout on every update, which needs geometry APIs jsdom
// does not implement. These tests exercise document transformations rather
// than layout, so the measurements are stubbed to zero rather than the editor
// being mocked out — that way the real CodeMirror state machine still runs.
if (typeof Range !== 'undefined' && !Range.prototype.getClientRects) {
  const emptyRectList = {
    length: 0,
    item: () => null,
    [Symbol.iterator]: function* () {},
  } as unknown as DOMRectList;

  const zeroRect = {
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    bottom: 0,
    right: 0,
    width: 0,
    height: 0,
    toJSON: () => ({}),
  } as DOMRect;

  Range.prototype.getClientRects = () => emptyRectList;
  Range.prototype.getBoundingClientRect = () => zeroRect;
}
