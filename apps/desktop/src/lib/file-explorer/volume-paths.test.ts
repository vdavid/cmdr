import { describe, it, expect, vi, beforeEach } from 'vitest'

/**
 * Tests for the per-volume path saving logic.
 * These tests verify that volume switching correctly saves and restores paths.
 */

// We'll test the logic by creating mock versions of the volume switch functions
// that mirror the actual implementation in DualPaneExplorer.svelte

// Types matching the app
type VolumePathMap = Record<string, string>

interface MockState {
    leftVolumeId: string
    leftPath: string
    rightVolumeId: string
    rightPath: string
    lastUsedPaths: VolumePathMap
}

interface OtherPaneState {
    otherPaneVolumeId: string
    otherPanePath: string
}

// Helper to simulate the determineNavigationPath logic
async function determineNavigationPath(
    volumeId: string,
    volumePath: string,
    targetPath: string,
    lastUsedPaths: VolumePathMap,
    pathExists: (p: string) => Promise<boolean>,
    defaultVolumeId: string,
    otherPane: OtherPaneState,
): Promise<string> {
    // User chose a favorite - go to the favorite's path directly
    if (targetPath !== volumePath) {
        return targetPath
    }

    // If the other pane is on the same volume, use its path (allows copying paths between panes)
    if (otherPane.otherPaneVolumeId === volumeId && (await pathExists(otherPane.otherPanePath))) {
        return otherPane.otherPanePath
    }

    // Look up the last used path for this volume
    const lastUsedPath = lastUsedPaths[volumeId]
    if (lastUsedPath && (await pathExists(lastUsedPath))) {
        return lastUsedPath
    }

    // Default: ~ for main volume (root), volume path for others
    if (volumeId === defaultVolumeId) {
        return '~'
    }
    return volumePath
}

// Simulates handleLeftVolumeChange logic
async function handleLeftVolumeChange(
    state: MockState,
    volumeId: string,
    volumePath: string,
    targetPath: string,
    pathExists: (p: string) => Promise<boolean>,
    defaultVolumeId: string,
): Promise<{ savedVolumeId: string; savedPath: string; newVolumeId: string; newPath: string }> {
    // First, save the current path for the old volume (before switching)
    const savedVolumeId = state.leftVolumeId
    const savedPath = state.leftPath
    state.lastUsedPaths[savedVolumeId] = savedPath

    // Determine where to navigate (passing correct pane state)
    const pathToNavigate = await determineNavigationPath(
        volumeId,
        volumePath,
        targetPath,
        state.lastUsedPaths,
        pathExists,
        defaultVolumeId,
        { otherPaneVolumeId: state.rightVolumeId, otherPanePath: state.rightPath },
    )

    // Update state
    state.leftVolumeId = volumeId
    state.leftPath = pathToNavigate

    return {
        savedVolumeId,
        savedPath,
        newVolumeId: volumeId,
        newPath: pathToNavigate,
    }
}

describe('Volume path saving logic', () => {
    const DEFAULT_VOLUME_ID = 'root'
    const pathExists = vi.fn().mockResolvedValue(true)

    beforeEach(() => {
        vi.clearAllMocks()
    })

    describe('when switching volumes', () => {
        it('saves the old path under the OLD volume ID', async () => {
            const state: MockState = {
                leftVolumeId: 'installer-vol',
                leftPath: '/Volumes/InstallerDisk/SomeApp',
                rightVolumeId: 'root',
                rightPath: '~',
                lastUsedPaths: {},
            }

            const result = await handleLeftVolumeChange(state, 'root', '/', '/', pathExists, DEFAULT_VOLUME_ID)

            expect(result.savedVolumeId).toBe('installer-vol')
            expect(result.savedPath).toBe('/Volumes/InstallerDisk/SomeApp')
            expect(state.lastUsedPaths['installer-vol']).toBe('/Volumes/InstallerDisk/SomeApp')
        })

        it('does NOT save new path under old volume ID (the reported bug)', async () => {
            const state: MockState = {
                leftVolumeId: 'root',
                leftPath: '~',
                rightVolumeId: 'installer-vol',
                rightPath: '/Volumes/InstallerDisk',
                lastUsedPaths: {},
            }

            // Switch from Mac HD (~) to Dropbox
            await handleLeftVolumeChange(
                state,
                'dropbox',
                '/Users/test/Library/CloudStorage/Dropbox',
                '/Users/test/Library/CloudStorage/Dropbox',
                pathExists,
                DEFAULT_VOLUME_ID,
            )

            // The bug was: root would get the Dropbox path
            // Correct behavior: root should have ~ (the path BEFORE switching)
            expect(state.lastUsedPaths['root']).toBe('~')
            expect(state.lastUsedPaths['dropbox']).toBeUndefined() // Not yet saved for new volume
        })

        it('defaults to ~ for main volume when no lastUsedPath exists and other pane is on different volume', async () => {
            const state: MockState = {
                leftVolumeId: 'installer-vol',
                leftPath: '/Volumes/InstallerDisk',
                rightVolumeId: 'dropbox',
                rightPath: '/Users/test/Library/CloudStorage/Dropbox',
                lastUsedPaths: {},
            }

            const result = await handleLeftVolumeChange(state, 'root', '/', '/', pathExists, DEFAULT_VOLUME_ID)

            expect(result.newPath).toBe('~')
        })

        it('defaults to volume root for non-main volumes when no lastUsedPath exists', async () => {
            const state: MockState = {
                leftVolumeId: 'root',
                leftPath: '~',
                rightVolumeId: 'root',
                rightPath: '/Users/test/Documents',
                lastUsedPaths: {},
            }

            const result = await handleLeftVolumeChange(
                state,
                'dropbox',
                '/Users/test/Library/CloudStorage/Dropbox',
                '/Users/test/Library/CloudStorage/Dropbox',
                pathExists,
                DEFAULT_VOLUME_ID,
            )

            expect(result.newPath).toBe('/Users/test/Library/CloudStorage/Dropbox')
        })

        it('restores lastUsedPath when switching back to a volume (and other pane on different volume)', async () => {
            const state: MockState = {
                leftVolumeId: 'dropbox',
                leftPath: '/Users/test/Library/CloudStorage/Dropbox/WorkFolder',
                rightVolumeId: 'installer-vol',
                rightPath: '/Volumes/InstallerDisk',
                lastUsedPaths: {
                    root: '/Users/test/Documents',
                },
            }

            const result = await handleLeftVolumeChange(state, 'root', '/', '/', pathExists, DEFAULT_VOLUME_ID)

            expect(result.newPath).toBe('/Users/test/Documents')
        })

        it('navigates directly to favorite path without looking up lastUsedPath or other pane', async () => {
            const state: MockState = {
                leftVolumeId: 'root',
                leftPath: '/Users/test/Documents',
                rightVolumeId: 'root',
                rightPath: '/Users/test/Projects', // Other pane is on same volume with different path
                lastUsedPaths: {
                    root: '/some/other/path',
                },
            }

            // Selecting a favorite: targetPath !== volumePath
            const result = await handleLeftVolumeChange(
                state,
                'root', // Favorites resolve to their containing volume
                '/',
                '/Users/test/Downloads', // Favorite's path
                pathExists,
                DEFAULT_VOLUME_ID,
            )

            // Should go to the favorite's path, not the lastUsedPath or other pane
            expect(result.newPath).toBe('/Users/test/Downloads')
        })
    })

    describe('other pane path copying', () => {
        it('uses other pane path when switching to the same volume the other pane is on', async () => {
            const state: MockState = {
                leftVolumeId: 'dropbox',
                leftPath: '/Users/test/Library/CloudStorage/Dropbox',
                rightVolumeId: 'root',
                rightPath: '/Users/test/Documents/ProjectA',
                lastUsedPaths: {
                    root: '/Users/test/Desktop', // This should be ignored in favor of other pane
                },
            }

            const result = await handleLeftVolumeChange(state, 'root', '/', '/', pathExists, DEFAULT_VOLUME_ID)

            // Should use the right pane's path, not lastUsedPaths
            expect(result.newPath).toBe('/Users/test/Documents/ProjectA')
        })

        it('allows copying path by re-selecting the same volume when other pane is on same volume', async () => {
            const state: MockState = {
                leftVolumeId: 'root',
                leftPath: '/Users/test/Desktop',
                rightVolumeId: 'root',
                rightPath: '/Users/test/Documents/ImportantFolder',
                lastUsedPaths: {},
            }

            // User re-selects root volume - should copy from right pane
            const result = await handleLeftVolumeChange(state, 'root', '/', '/', pathExists, DEFAULT_VOLUME_ID)

            expect(result.newPath).toBe('/Users/test/Documents/ImportantFolder')
        })

        it('falls back to lastUsedPath when other pane is on different volume', async () => {
            const state: MockState = {
                leftVolumeId: 'installer-vol',
                leftPath: '/Volumes/InstallerDisk',
                rightVolumeId: 'dropbox',
                rightPath: '/Users/test/Library/CloudStorage/Dropbox',
                lastUsedPaths: {
                    root: '/Users/test/Projects',
                },
            }

            const result = await handleLeftVolumeChange(state, 'root', '/', '/', pathExists, DEFAULT_VOLUME_ID)

            // Other pane is on dropbox, so we use lastUsedPaths for root
            expect(result.newPath).toBe('/Users/test/Projects')
        })

        it('prioritizes favorite path over other pane path', async () => {
            const state: MockState = {
                leftVolumeId: 'dropbox',
                leftPath: '/Users/test/Library/CloudStorage/Dropbox',
                rightVolumeId: 'root',
                rightPath: '/Users/test/Documents', // Other pane is on root
                lastUsedPaths: {},
            }

            // Selecting a favorite (Downloads) on root volume
            const result = await handleLeftVolumeChange(
                state,
                'root',
                '/',
                '/Users/test/Downloads', // Favorite path
                pathExists,
                DEFAULT_VOLUME_ID,
            )

            // Favorite takes priority over other pane
            expect(result.newPath).toBe('/Users/test/Downloads')
        })
    })

    describe('full user flow', () => {
        it('correctly tracks paths through multiple volume switches', async () => {
            const state: MockState = {
                leftVolumeId: 'installer-vol',
                leftPath: '/Volumes/qBittorrent/App',
                rightVolumeId: 'dropbox',
                rightPath: '/Users/test/Library/CloudStorage/Dropbox',
                lastUsedPaths: {},
            }

            // Step 1: Start on installer volume, switch to Mac HD
            await handleLeftVolumeChange(state, 'root', '/', '/', pathExists, DEFAULT_VOLUME_ID)

            expect(state.lastUsedPaths).toEqual({
                'installer-vol': '/Volumes/qBittorrent/App',
            })
            expect(state.leftVolumeId).toBe('root')
            expect(state.leftPath).toBe('~') // Default for root when other pane is on different volume

            // Step 2: Switch from Mac HD to Dropbox
            await handleLeftVolumeChange(
                state,
                'dropbox',
                '/Users/test/Library/CloudStorage/Dropbox',
                '/Users/test/Library/CloudStorage/Dropbox',
                pathExists,
                DEFAULT_VOLUME_ID,
            )

            expect(state.lastUsedPaths).toEqual({
                'installer-vol': '/Volumes/qBittorrent/App',
                root: '~',
            })
            expect(state.leftVolumeId).toBe('dropbox')
            // Should copy from right pane since both are now on dropbox
            expect(state.leftPath).toBe('/Users/test/Library/CloudStorage/Dropbox')

            // Step 3: Simulate navigation within Dropbox (left pane)
            state.leftPath = '/Users/test/Library/CloudStorage/Dropbox/Work'

            // Step 4: Switch back to Mac HD (right pane is still on Dropbox)
            state.rightVolumeId = 'dropbox'
            state.rightPath = '/Users/test/Library/CloudStorage/Dropbox/Personal'
            await handleLeftVolumeChange(state, 'root', '/', '/', pathExists, DEFAULT_VOLUME_ID)

            expect(state.lastUsedPaths).toEqual({
                'installer-vol': '/Volumes/qBittorrent/App',
                root: '~',
                dropbox: '/Users/test/Library/CloudStorage/Dropbox/Work',
            })
            expect(state.leftVolumeId).toBe('root')
            expect(state.leftPath).toBe('~') // Restored from lastUsedPaths (other pane on different volume)
        })

        it('correctly uses other pane path when switching between panes on same volume', async () => {
            const state: MockState = {
                leftVolumeId: 'root',
                leftPath: '/Users/test/Desktop',
                rightVolumeId: 'root',
                rightPath: '/Users/test/Documents/Work',
                lastUsedPaths: {},
            }

            // Left pane switches to Dropbox
            await handleLeftVolumeChange(
                state,
                'dropbox',
                '/Users/test/Library/CloudStorage/Dropbox',
                '/Users/test/Library/CloudStorage/Dropbox',
                pathExists,
                DEFAULT_VOLUME_ID,
            )

            expect(state.leftVolumeId).toBe('dropbox')
            expect(state.lastUsedPaths['root']).toBe('/Users/test/Desktop')

            // Left pane switches back to root - should get right pane's path
            await handleLeftVolumeChange(state, 'root', '/', '/', pathExists, DEFAULT_VOLUME_ID)

            expect(state.leftPath).toBe('/Users/test/Documents/Work') // Copied from right pane
        })
    })
})
