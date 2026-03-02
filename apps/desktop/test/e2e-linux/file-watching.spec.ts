/**
 * E2E test for inotify-based file watching on Linux.
 *
 * Verifies that the `notify` crate's inotify backend picks up external
 * filesystem changes and refreshes the pane listing automatically.
 */

import fs from 'fs'
import path from 'path'
import { ensureAppReady, fileExistsInFocusedPane } from '../e2e-shared/helpers.js'

/** Returns the fixture root path from the environment variable. */
function getFixtureRoot(): string {
    const root = process.env.CMDR_E2E_START_PATH
    if (!root) throw new Error('CMDR_E2E_START_PATH env var is not set')
    return root
}

describe('File watching', () => {
    const createdDirs: string[] = []

    afterEach(() => {
        for (const dir of createdDirs) {
            try {
                fs.rmSync(dir, { recursive: true, force: true })
            } catch {
                // Best-effort cleanup
            }
        }
        createdDirs.length = 0
    })

    it('detects an externally created directory via inotify', async () => {
        await ensureAppReady()
        const fixtureRoot = getFixtureRoot()

        const dirName = `inotify-test-${Date.now()}`
        const dirPath = path.join(fixtureRoot, 'left', dirName)

        // Verify the directory does not exist in the pane yet
        const existsBefore = await fileExistsInFocusedPane(dirName)
        expect(existsBefore).toBe(false)

        // Create the directory externally (not through the app)
        fs.mkdirSync(dirPath)
        createdDirs.push(dirPath)

        // Wait for inotify to pick up the change and the pane to refresh
        await browser.waitUntil(async () => fileExistsInFocusedPane(dirName), {
            timeout: 8000,
            interval: 500,
            timeoutMsg: `${dirName} did not appear in the file listing after external mkdir (inotify not working?)`,
        })
    })
})
