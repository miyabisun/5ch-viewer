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
      // VITE_API_TARGET points the dev server at another backend, such as a local
      // itest-server (the real Rust router on an in-memory DB).
      '/api': process.env.VITE_API_TARGET || 'http://localhost:3000',
    },
  },
})
