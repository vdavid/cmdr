import { describe, it, expect } from 'vitest'
import { createRenameState, type RenameTarget } from './rename-state.svelte'

const sampleTarget: RenameTarget = {
    path: '/Users/test/file.txt',
    originalName: 'file.txt',
    parentPath: '/Users/test',
    index: 3,
    isDirectory: false,
}

describe('createRenameState', () => {
    it('starts inactive', () => {
        const state = createRenameState()
        expect(state.active).toBe(false)
        expect(state.target).toBeNull()
        expect(state.currentName).toBe('')
        expect(state.severity).toBe('ok')
        expect(state.shaking).toBe(false)
    })

    it('activates with target', () => {
        const state = createRenameState()
        state.activate(sampleTarget)

        expect(state.active).toBe(true)
        expect(state.target).toEqual(sampleTarget)
        expect(state.currentName).toBe('file.txt')
        expect(state.severity).toBe('ok')
    })

    it('updates current name', () => {
        const state = createRenameState()
        state.activate(sampleTarget)
        state.setCurrentName('newname.txt')

        expect(state.currentName).toBe('newname.txt')
    })

    it('updates validation', () => {
        const state = createRenameState()
        state.activate(sampleTarget)
        state.setValidation({ severity: 'error', message: 'Bad name' })

        expect(state.severity).toBe('error')
        expect(state.validation.message).toBe('Bad name')
    })

    it('triggers and clears shake', () => {
        const state = createRenameState()
        state.activate(sampleTarget)
        state.triggerShake()
        expect(state.shaking).toBe(true)

        state.clearShake()
        expect(state.shaking).toBe(false)
    })

    it('clears shake on name change', () => {
        const state = createRenameState()
        state.activate(sampleTarget)
        state.triggerShake()
        expect(state.shaking).toBe(true)

        state.setCurrentName('x')
        expect(state.shaking).toBe(false)
    })

    it('cancels and resets to initial state', () => {
        const state = createRenameState()
        state.activate(sampleTarget)
        state.setCurrentName('changed')
        state.setValidation({ severity: 'warning', message: 'Conflict' })

        state.cancel()
        expect(state.active).toBe(false)
        expect(state.target).toBeNull()
        expect(state.currentName).toBe('')
        expect(state.severity).toBe('ok')
    })

    it('detects when name has changed', () => {
        const state = createRenameState()
        state.activate(sampleTarget)

        expect(state.hasChanged()).toBe(false)

        state.setCurrentName('different.txt')
        expect(state.hasChanged()).toBe(true)
    })

    it('detects change with trimmed whitespace', () => {
        const state = createRenameState()
        state.activate(sampleTarget)

        // Same name with spaces — no change after trim
        state.setCurrentName('  file.txt  ')
        expect(state.hasChanged()).toBe(false)

        state.setCurrentName('  different.txt  ')
        expect(state.hasChanged()).toBe(true)
    })

    it('returns trimmed name', () => {
        const state = createRenameState()
        state.activate(sampleTarget)
        state.setCurrentName('  spaced.txt  ')

        expect(state.getTrimmedName()).toBe('spaced.txt')
    })
})
