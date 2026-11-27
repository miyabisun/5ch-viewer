import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vitest/config'

// Unit tests live next to the source (src/lib/*.test.js); the tests of the page, in jsdom,
// in src/page/*.test.ts. What only a browser's engine shows is tested in tests/page.rs.
export default defineConfig({
  plugins: [svelte()],
  resolve: {
    conditions: ['browser'],
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.{js,ts}'],
    setupFiles: ['src/testing/setup.ts'],
  },
})
