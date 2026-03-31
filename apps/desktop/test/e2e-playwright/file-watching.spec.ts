/**
 * E2E test for file watching (inotify on Linux, FSEvents on macOS).
 *
 * Verifies that external filesystem changes are detected by the app's
 * file watcher and the pane listing refreshes automatically.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { ensureAppReady, getFixtureRoot, fileExistsInFocusedPane, pollUntil } from './helpers.js'

test.describe('File watching', () => {
  const createdDirs: string[] = []

  test.afterEach(() => {
    for (const dir of createdDirs) {
      try {
        fs.rmSync(dir, { recursive: true, force: true })
      } catch {
        // Best-effort cleanup
      }
    }
    createdDirs.length = 0
  })

  test('detects an externally created directory', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()

    const dirName = `watch-test-${Date.now()}`
    const dirPath = path.join(fixtureRoot, 'left', dirName)

    // Verify the directory does not exist in the pane yet
    expect(await fileExistsInFocusedPane(tauriPage, dirName)).toBe(false)

    // Create the directory externally (not through the app)
    fs.mkdirSync(dirPath)
    createdDirs.push(dirPath)

    // Wait for the file watcher to pick up the change and the pane to refresh
    await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, dirName), 8000)
  })
})
