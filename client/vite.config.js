import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [svelte()],
  base: './',
  build: { outDir: 'build' },
  server: {
    host: '0.0.0.0',
    port: 5173,
    proxy: {
      // Integration tests point this at the in-memory itest-server (port 3001) via
      // VITE_API_TARGET so the Svelte app talks to the real Rust backend.
      '/api': process.env.VITE_API_TARGET || 'http://localhost:3000',
    },
  },
})
