import { defineConfig } from 'vite';

export default defineConfig({
  root: 'src/renderer/settings',
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: '../../../dist',
    emptyOutDir: true,
    target: 'es2020',
  },
});

