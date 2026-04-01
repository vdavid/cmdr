/**
 * Playwright test fixtures for Cmdr E2E testing.
 *
 * Uses tauri-playwright in Tauri mode: the test runner communicates with
 * the real Tauri app via a Unix socket, and commands are injected directly
 * into the webview via `webview.eval()`. No WebDriver, no HTTP server.
 *
 * Fixture lifecycle:
 * - globalSetup: creates the fixture directory tree (~170 MB)
 * - beforeEach: recreates small text files (keeps bulk .dat files)
 * - globalTeardown: deletes the fixture directory
 */

import { createTauriTest } from '@srsholmes/tauri-playwright'

export const { test, expect } = createTauriTest({
  // No devUrl — in Tauri mode, the app is already running with its built
  // frontend. Setting devUrl would redirect the webview to a nonexistent
  // dev server. devUrl is only used in browser mode (not applicable here).
  devUrl: '',

  // Tauri mode config
  mcpSocket: '/tmp/tauri-playwright.sock',
})
