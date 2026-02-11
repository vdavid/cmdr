import { describe, it, expect, beforeEach, vi } from 'vitest'
import { resolveDropTarget } from './drop-target-hit-testing'

// jsdom doesn't implement elementFromPoint — stub it so we can mock per-test
function mockElementFromPoint(el: Element | null) {
    document.elementFromPoint = vi.fn().mockReturnValue(el)
}

function getElement(id: string): HTMLElement {
    const el = document.getElementById(id)
    if (!el) throw new Error(`Element #${id} not found`)
    return el
}

describe('resolveDropTarget', () => {
    let leftPane: HTMLDivElement
    let rightPane: HTMLDivElement

    beforeEach(() => {
        document.body.innerHTML = ''

        // Build left pane with a directory row, a file row, and a ".." row
        leftPane = document.createElement('div')
        leftPane.className = 'pane-wrapper'

        const dirEntry = document.createElement('div')
        dirEntry.className = 'file-entry'
        dirEntry.setAttribute('data-drop-target-path', '/Users/test/Documents')
        dirEntry.id = 'left-dir'
        // Add a nested child inside the directory entry
        const nestedSpan = document.createElement('span')
        nestedSpan.className = 'col-name'
        nestedSpan.textContent = 'Documents'
        nestedSpan.id = 'left-dir-name'
        dirEntry.appendChild(nestedSpan)
        leftPane.appendChild(dirEntry)

        const fileEntry = document.createElement('div')
        fileEntry.className = 'file-entry'
        // No data-drop-target-path — it's a file
        fileEntry.id = 'left-file'
        leftPane.appendChild(fileEntry)

        const parentEntry = document.createElement('div')
        parentEntry.className = 'file-entry'
        // ".." entry has no data-drop-target-path
        parentEntry.id = 'left-parent'
        leftPane.appendChild(parentEntry)

        // Pane background area (not inside any file-entry)
        const bgArea = document.createElement('div')
        bgArea.className = 'pane-bg'
        bgArea.id = 'left-bg'
        leftPane.appendChild(bgArea)

        document.body.appendChild(leftPane)

        // Build right pane with one directory
        rightPane = document.createElement('div')
        rightPane.className = 'pane-wrapper'

        const rightDir = document.createElement('div')
        rightDir.className = 'file-entry'
        rightDir.setAttribute('data-drop-target-path', '/Users/test/Downloads')
        rightDir.id = 'right-dir'
        rightPane.appendChild(rightDir)

        document.body.appendChild(rightPane)
    })

    it('returns folder target when cursor is over a directory row', () => {
        const el = getElement('left-dir')
        mockElementFromPoint(el)

        const result = resolveDropTarget(100, 50, leftPane, rightPane)

        expect(result).toEqual({
            type: 'folder',
            path: '/Users/test/Documents',
            element: el,
            paneId: 'left',
        })
    })

    it('walks up from nested child to find .file-entry with data-drop-target-path', () => {
        const nestedEl = getElement('left-dir-name')
        mockElementFromPoint(nestedEl)

        const result = resolveDropTarget(100, 50, leftPane, rightPane)

        expect(result).toEqual({
            type: 'folder',
            path: '/Users/test/Documents',
            element: getElement('left-dir'),
            paneId: 'left',
        })
    })

    it('returns pane target when cursor is over a file row', () => {
        mockElementFromPoint(getElement('left-file'))

        const result = resolveDropTarget(100, 50, leftPane, rightPane)

        expect(result).toEqual({ type: 'pane', paneId: 'left' })
    })

    it('returns pane target when cursor is over ".." entry', () => {
        mockElementFromPoint(getElement('left-parent'))

        const result = resolveDropTarget(100, 50, leftPane, rightPane)

        expect(result).toEqual({ type: 'pane', paneId: 'left' })
    })

    it('returns pane target when cursor is over pane background', () => {
        mockElementFromPoint(getElement('left-bg'))

        const result = resolveDropTarget(100, 50, leftPane, rightPane)

        expect(result).toEqual({ type: 'pane', paneId: 'left' })
    })

    it('returns folder target in the right pane', () => {
        const el = getElement('right-dir')
        mockElementFromPoint(el)

        const result = resolveDropTarget(500, 50, leftPane, rightPane)

        expect(result).toEqual({
            type: 'folder',
            path: '/Users/test/Downloads',
            element: el,
            paneId: 'right',
        })
    })

    it('returns null when cursor is outside both panes', () => {
        const outsideEl = document.createElement('div')
        document.body.appendChild(outsideEl)
        mockElementFromPoint(outsideEl)

        const result = resolveDropTarget(0, 0, leftPane, rightPane)

        expect(result).toBeNull()
    })

    it('returns null when elementFromPoint returns null', () => {
        mockElementFromPoint(null)

        const result = resolveDropTarget(0, 0, leftPane, rightPane)

        expect(result).toBeNull()
    })

    it('handles undefined left pane element', () => {
        const el = getElement('right-dir')
        mockElementFromPoint(el)

        const result = resolveDropTarget(500, 50, undefined, rightPane)

        expect(result).toEqual({
            type: 'folder',
            path: '/Users/test/Downloads',
            element: el,
            paneId: 'right',
        })
    })

    it('returns null when both pane elements are undefined', () => {
        mockElementFromPoint(getElement('left-dir'))

        const result = resolveDropTarget(100, 50, undefined, undefined)

        expect(result).toBeNull()
    })
})
