import path from 'node:path';
import { defineConfig } from 'vite';

export default defineConfig({
  root: path.resolve(__dirname, 'src/renderer/settings'),
  build: {
    outDir: path.resolve(__dirname, '.vite/renderer/settings'),
  },
  clearScreen: false,
});

