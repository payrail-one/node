import { defineConfig } from 'vite';

export default defineConfig({
  build: { assetsInlineLimit: 0 },
  server: {
    port: 4178,
    strictPort: true,
    proxy: {
      '/api': { target: 'http://127.0.0.1:18081', changeOrigin: true },
      '/health': { target: 'http://127.0.0.1:18081', changeOrigin: true },
    },
  },
});
