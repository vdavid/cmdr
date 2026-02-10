import { describe, it, expect } from 'vitest'
import {
    showOverlay,
    updateOverlay,
    hideOverlay,
    getOverlayVisible,
    getOverlayX,
    getOverlayY,
    getOverlayFileInfos,
    getOverlayTotalCount,
    getOverlayTargetName,
    getOverlayOperation,
    getOverlayCanDrop,
    buildOverlayNameLines,
    formatActionLine,
    type OverlayFileInfo,
} from './drag-overlay.svelte'

function fileInfo(name: string, isDirectory = false, iconUrl?: string): OverlayFileInfo {
    return { name, isDirectory, iconUrl }
}

describe('buildOverlayNameLines', () => {
    it('shows all names when 20 or fewer', () => {
        const infos = [fileInfo('a.txt'), fileInfo('b.txt'), fileInfo('c.txt')]
        const lines = buildOverlayNameLines(infos, 3)

        expect(lines).toHaveLength(3)
        expect(lines[0].text).toBe('a.txt')
        expect(lines[0].isSummary).toBe(false)
        expect(lines[0].isDirectory).toBe(false)
    })

    it('shows exactly 20 names when count is 20', () => {
        const infos = Array.from({ length: 20 }, (_, i) => fileInfo(`file${String(i)}.txt`))
        const lines = buildOverlayNameLines(infos, 20)

        expect(lines).toHaveLength(20)
        expect(lines.every((l) => !l.isSummary)).toBe(true)
    })

    it('truncates to first 10 + "and N more" when more than 20', () => {
        const infos = Array.from({ length: 20 }, (_, i) => fileInfo(`file${String(i)}.txt`))
        const lines = buildOverlayNameLines(infos, 50)

        // First 10 + "and 40 more"
        expect(lines).toHaveLength(11)
        expect(lines[9].text).toBe('file9.txt')
        expect(lines[9].isSummary).toBe(false)
        expect(lines[10].text).toBe('and 40 more')
        expect(lines[10].isSummary).toBe(true)
    })

    it('handles empty list', () => {
        const lines = buildOverlayNameLines([], 0)
        expect(lines).toEqual([])
    })

    it('preserves icon URLs in output lines', () => {
        const infos = [fileInfo('doc.pdf', false, 'data:image/png;base64,abc')]
        const lines = buildOverlayNameLines(infos, 1)

        expect(lines[0].iconUrl).toBe('data:image/png;base64,abc')
    })

    it('marks directories correctly', () => {
        const infos = [fileInfo('Documents', true)]
        const lines = buildOverlayNameLines(infos, 1)

        expect(lines[0].isDirectory).toBe(true)
    })

    it('summary line has no icon URL and is not a directory', () => {
        const infos = Array.from({ length: 20 }, (_, i) => fileInfo(`file${String(i)}.txt`))
        const lines = buildOverlayNameLines(infos, 25)

        const summary = lines[lines.length - 1]
        expect(summary.isSummary).toBe(true)
        expect(summary.iconUrl).toBeUndefined()
        expect(summary.isDirectory).toBe(false)
    })
})

describe('formatActionLine', () => {
    it('shows copy with target name', () => {
        expect(formatActionLine('copy', 'Documents', true)).toBe('Copy to Documents')
    })

    it('shows move with target name', () => {
        expect(formatActionLine('move', 'Downloads', true)).toBe('Move to Downloads')
    })

    it('shows just the verb when no target name', () => {
        expect(formatActionLine('copy', null, true)).toBe('Copy')
    })

    it('shows cannot drop when canDrop is false', () => {
        expect(formatActionLine('copy', 'Documents', false)).toBe("Can't drop here")
    })

    it('shows cannot drop even with move operation when not allowed', () => {
        expect(formatActionLine('move', null, false)).toBe("Can't drop here")
    })
})

describe('overlay state management', () => {
    it('starts hidden', () => {
        hideOverlay() // Reset
        expect(getOverlayVisible()).toBe(false)
    })

    it('becomes visible after showOverlay', () => {
        showOverlay([fileInfo('file.txt')], 1)
        expect(getOverlayVisible()).toBe(true)
        expect(getOverlayFileInfos()).toEqual([fileInfo('file.txt')])
        expect(getOverlayTotalCount()).toBe(1)
        hideOverlay()
    })

    it('updates position and target on updateOverlay', () => {
        showOverlay([fileInfo('file.txt')], 1)
        updateOverlay(100, 200, 'Documents', true, 'copy')

        expect(getOverlayX()).toBe(100)
        expect(getOverlayY()).toBe(200)
        expect(getOverlayTargetName()).toBe('Documents')
        expect(getOverlayCanDrop()).toBe(true)
        expect(getOverlayOperation()).toBe('copy')
        hideOverlay()
    })

    it('resets all state on hideOverlay', () => {
        showOverlay([fileInfo('file.txt')], 1)
        updateOverlay(100, 200, 'Documents', true, 'move')
        hideOverlay()

        expect(getOverlayVisible()).toBe(false)
        expect(getOverlayX()).toBe(0)
        expect(getOverlayY()).toBe(0)
        expect(getOverlayFileInfos()).toEqual([])
        expect(getOverlayTotalCount()).toBe(0)
        expect(getOverlayTargetName()).toBeNull()
        expect(getOverlayCanDrop()).toBe(false)
    })

    it('slices file infos to max display count', () => {
        const infos = Array.from({ length: 30 }, (_, i) => fileInfo(`file${String(i)}.txt`))
        showOverlay(infos, 30)

        // Should only store first 20
        expect(getOverlayFileInfos()).toHaveLength(20)
        expect(getOverlayTotalCount()).toBe(30)
        hideOverlay()
    })
})
