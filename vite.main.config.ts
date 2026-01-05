import { defineConfig } from 'vite';
import { config as appConfig } from './app.config';

// https://vitejs.dev/config
export default defineConfig({
  // Inject app configuration into the main process at build time
  define: {
    '__APP_CONFIG__': JSON.stringify(appConfig),
  },
  build: {
    // Don't minify to help with debugging
    minify: false,
  },
  resolve: {
    // Ensure we're building for Node.js
    browserField: false,
    mainFields: ['module', 'main'],
  },
});
