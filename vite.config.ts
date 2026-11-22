import { cloudflare } from '@cloudflare/vite-plugin';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig(({ mode }) => {
  const useCloudflare =
    !process.env.VITEST &&
    process.env.TAURI_DEV !== '1' &&
    !process.env.TAURI_ENV_PLATFORM &&
    mode !== 'desktop' &&
    mode !== 'local';
  return {
    plugins: [useCloudflare ? cloudflare() : null, react()].filter(Boolean),
    clearScreen: false,
    server: {
      port: 5173,
      strictPort: false,
      proxy: {
        '/api': { target: 'http://127.0.0.1:3420', changeOrigin: true, ws: true },
      },
      watch: {
        ignored: ['**/src-tauri/**', '**/worker/**', '**/convex/**'],
      },
    },
  };
});
