import { defineConfig } from 'vite';

export default defineConfig({
  build: { assetsInlineLimit: 0 },
  server: {
    port: 4175,
    strictPort: true,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:18080',
        changeOrigin: true,
        ws: true,
      },
    },
  },
});
