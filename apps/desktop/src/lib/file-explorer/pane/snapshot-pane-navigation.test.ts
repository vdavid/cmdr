/**
 * Unit tests for `isCrossVolumeNavigation` (R4 bug-fix helper).
 *
 * Pins the decision that drives the production fix in
 * `FilePane.handleNavigate` and `DualPaneExplorer.navigateToPath`: when the
 * current pane is on the snapshot volume and the target path is a real
 * filesystem path, we MUST route through the volume-change machinery instead
 * of doing a bare `loadDirectory()` (which would leave `volumeId === 'search-results'`
 * with a real `path` and trigger the "Search results no longer available" pane).
 */

import { describe, it, expect } from 'vitest'
import { isCrossVolumeNavigation } from './snapshot-pane-navigation'

describe('isCrossVolumeNavigation', () => {
    it('returns true when leaving the snapshot volume for a real absolute path', () => {
        expect(isCrossVolumeNavigation('search-results', '/Users/me/Documents')).toBe(true)
    })

    it('returns true for the exact path that reproduced the user bug (Library/Developer/CommandLineTools/...)', () => {
        const realPath = '/Library/Developer/CommandLineTools/SDKs/MacOSX15.4.sdk/System/Library/PrivateFrameworks'
        expect(isCrossVolumeNavigation('search-results', realPath)).toBe(true)
    })

    it('returns false on an internal snapshot to snapshot navigation', () => {
        expect(isCrossVolumeNavigation('search-results', 'search-results://sr-2')).toBe(false)
    })

    it('returns false on a normal local volume', () => {
        expect(isCrossVolumeNavigation('root', '/Users/me/Documents')).toBe(false)
    })

    it('returns false on the network virtual volume', () => {
        expect(isCrossVolumeNavigation('network', 'smb://server/share')).toBe(false)
    })

    it('returns false on an MTP volume', () => {
        expect(isCrossVolumeNavigation('mtp-1234:storage', '/DCIM/100ANDRO')).toBe(false)
    })
})
