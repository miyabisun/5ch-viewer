import { defineConfig } from 'vitest/config'

// Unit tests live next to the source (src/**/*.test.js). The Playwright E2E
// specs under tests/ are excluded so vitest does not try to collect them.
export default defineConfig({
  test: {
    include: ['src/**/*.test.js'],
  },
})
