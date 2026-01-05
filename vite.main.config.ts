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
    rollupOptions: {
      // Keep native notifier out of the bundle (it ships vendor binaries and must stay in node_modules).
      external: ['node-notifier'],
    },
  },
  resolve: {
    // Ensure we're building for Node.js
    browserField: false,
    mainFields: ['module', 'main'],
  },
});
