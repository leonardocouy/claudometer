import { viteStaticCopy } from 'vite-plugin-static-copy';
import { defineConfig } from 'vite';
import { config as appConfig } from './app.config';

// https://vitejs.dev/config
export default defineConfig({
  plugins: [
    viteStaticCopy({
      targets: [
        {
          src: 'src/ui/settings-window/settings.html',
          dest: 'ui/settings-window',
        },
      ],
    }),
  ],
  // Inject app configuration into the main process at build time
  define: {
    '__APP_CONFIG__': JSON.stringify(appConfig),
  },
  build: {
    // Don't minify to help with debugging
    minify: false,
    rollupOptions: {
      // Externalize native modules that can't be bundled
      external: ['keytar'],
    },
  },
  resolve: {
    // Ensure we're building for Node.js
    browserField: false,
    mainFields: ['module', 'main'],
  },
});
