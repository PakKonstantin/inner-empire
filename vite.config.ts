import { fileURLToPath, URL } from 'node:url';

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Tauri serves the dev build from a fixed port and needs a fixed host, so the
// webview can reach it. Failing loudly when the port is taken is deliberate:
// silently moving to another port would leave the desktop window pointing at
// nothing.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  resolve: {
    alias: {
      // Mirrors the `paths` entry in tsconfig.json so imports read the same way
      // in the editor and at build time.
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: '127.0.0.1',
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    // Matches the WebView2 and WebKitGTK versions we support.
    target: 'es2022',
    sourcemap: true,
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks: {
          // The editor and the graph are the two heavy dependencies. Splitting
          // them means opening a vault does not pay for the graph view.
          codemirror: [
            '@codemirror/state',
            '@codemirror/view',
            '@codemirror/commands',
            '@codemirror/language',
            '@codemirror/lang-markdown',
            '@codemirror/search',
            '@codemirror/autocomplete',
          ],
          graph: ['d3-force'],
        },
      },
    },
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
    globals: true,
    setupFiles: ['./src/test-setup.ts'],
  },
});
