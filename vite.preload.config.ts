import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    minify: false,
  },
  resolve: {
    browserField: false,
    conditions: ['node'],
    mainFields: ['module', 'main'],
  },
});

