import { describe, expect, it } from 'vitest';

import { parseManifest } from './manager';

describe('manifest validation', () => {
  it('accepts a well-formed manifest', () => {
    const manifest = parseManifest(
      JSON.stringify({
        id: 'word-count',
        name: 'Word Count',
        version: '1.0.0',
        description: 'Counts words',
        permissions: ['vault:read', 'ui'],
      }),
    );
    expect(manifest.id).toBe('word-count');
    expect(manifest.permissions).toEqual(['vault:read', 'ui']);
  });

  it('rejects a file that is not JSON', () => {
    expect(() => parseManifest('{ broken')).toThrow(/not valid JSON/);
  });

  it('rejects an id that could collide with a path', () => {
    for (const id of ['../escape', 'Has Spaces', 'UPPER', '', 'has/slash']) {
      expect(() => parseManifest(JSON.stringify({ id, name: 'X', version: '1' }))).toThrow(
        /plugin id/,
      );
    }
  });

  it('requires a name and a version', () => {
    expect(() => parseManifest(JSON.stringify({ id: 'a', version: '1' }))).toThrow(/name/);
    expect(() => parseManifest(JSON.stringify({ id: 'a', name: 'A' }))).toThrow(/version/);
  });

  it('refuses a permission it does not recognise', () => {
    // Silently dropping it would leave the plugin author thinking they had
    // access they do not have.
    expect(() =>
      parseManifest(
        JSON.stringify({ id: 'a', name: 'A', version: '1', permissions: ['filesystem:all'] }),
      ),
    ).toThrow(/Unrecognised permissions/);
  });

  it('treats a missing permission list as no permissions', () => {
    const manifest = parseManifest(JSON.stringify({ id: 'a', name: 'A', version: '1' }));
    expect(manifest.permissions).toEqual([]);
  });

  it('keeps network out unless it is asked for', () => {
    const manifest = parseManifest(
      JSON.stringify({ id: 'a', name: 'A', version: '1', permissions: ['vault:read'] }),
    );
    expect(manifest.permissions).not.toContain('network');
  });
});
