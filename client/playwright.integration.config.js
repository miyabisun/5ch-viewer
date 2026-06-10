import { defineConfig, devices } from '@playwright/test'

// 総合テスト (full-stack integration): the Svelte app talks to the REAL Rust backend
// through Vite's /api proxy. Three processes are started:
//   1. itest-server (Rust)  — app on :3001 (in-memory SQLite) + mock 5ch on :3002.
//   2. vite dev (:5174)     — serves the app and proxies /api -> :3001.
// Tests drive scenarios via the control endpoints on :3001 (/_control/*) and the mock
// (/_control/thread is exposed on :3002). See docs/testing.md.
//
// This is separate from the fast page.route suite (playwright.config.js) so that one
// stays mock-only and quick; this one verifies the real reload/dat/DB flow end to end.
const APP_PORT = 3001
const MOCK_PORT = 3002
const WEB_PORT = 5174

export const APP_URL = `http://127.0.0.1:${APP_PORT}`
export const MOCK_URL = `http://127.0.0.1:${MOCK_PORT}`

export default defineConfig({
  testDir: './integration',
  use: {
    baseURL: `http://localhost:${WEB_PORT}`,
  },
  webServer: [
    {
      // Real Rust backend + mock 5ch. Run from the repo root (one level up).
      command: `APP_PORT=${APP_PORT} MOCK_PORT=${MOCK_PORT} cargo run --quiet --bin itest-server`,
      cwd: '..',
      url: `${APP_URL}/api/favorites`,
      reuseExistingServer: !process.env.CI,
      timeout: 180000,
    },
    {
      command: `VITE_API_TARGET=${APP_URL} bun run dev --port ${WEB_PORT} --strictPort`,
      url: `http://localhost:${WEB_PORT}`,
      reuseExistingServer: !process.env.CI,
      timeout: 120000,
    },
  ],
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
})
