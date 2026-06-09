import { defineConfig, devices } from '@playwright/test'

// E2E runs against the Vite build (served by vite preview); API calls are mocked
// per-test via page.route, so the Rust backend is not needed. Browser: Chromium
// (Obscura's CDP lacks request interception / title reporting — see CLAUDE.md).
export default defineConfig({
  testDir: './tests',
  use: {
    baseURL: 'http://localhost:4173',
  },
  webServer: {
    command: 'bun run build && bun run preview --port 4173 --strictPort',
    url: 'http://localhost:4173',
    reuseExistingServer: true,
    timeout: 120000,
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
})
