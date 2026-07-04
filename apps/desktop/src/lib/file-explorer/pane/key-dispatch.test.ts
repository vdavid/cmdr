import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { FilePaneAPI } from './types'
import { createKeyDispatch, isTypingInInput, type KeyDispatchDeps } from './key-dispatch'

function makePaneRef(overrides: Partial<FilePaneAPI> = {}): FilePaneAPI {
    return {
        isLoading: vi.fn(() => false),
        handleCancelLoading: vi.fn(),
        isVolumeChooserOpen: vi.fn(() => false),
        handleVolumeChooserKeyDown: vi.fn(() => false),
        isRenaming: vi.fn(() => false),
        isJumpActive: vi.fn(() => false),
        handleJumpKeystroke: vi.fn(),
        clearJumpState: vi.fn(),
        handleKeyDown: vi.fn(),
        handleKeyUp: vi.fn(),
        ...overrides,
    } as unknown as FilePaneAPI
}

function setup(refs: Partial<Record<'left' | 'right', FilePaneAPI>>, container?: HTMLElement) {
    const deps: KeyDispatchDeps = {
        getPaneRef: (p) => refs[p],
        getFocusedPane: () => 'left',
        getContainerElement: () => container,
    }
    return createKeyDispatch(deps)
}

/** A KeyboardEvent with spied preventDefault/stopPropagation. */
function keyEvent(key: string): KeyboardEvent {
    const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })
    vi.spyOn(e, 'preventDefault')
    vi.spyOn(e, 'stopPropagation')
    return e
}

describe('createKeyDispatch', () => {
    beforeEach(() => vi.clearAllMocks())

    it('Escape while loading cancels the load and swallows the key (no forward)', () => {
        const left = makePaneRef({ isLoading: vi.fn(() => true) })
        const kd = setup({ left })
        const e = keyEvent('Escape')

        kd.handleKeyDown(e)

        expect(left.handleCancelLoading).toHaveBeenCalled()
        expect(e.preventDefault).toHaveBeenCalled()
        expect(left.handleKeyDown).not.toHaveBeenCalled()
    })

    it('an open volume chooser that consumes the key swallows it from the pane behind', () => {
        const left = makePaneRef({
            isVolumeChooserOpen: vi.fn(() => true),
            handleVolumeChooserKeyDown: vi.fn(() => true),
        })
        const kd = setup({ left })
        const e = keyEvent('ArrowDown')

        kd.handleKeyDown(e)

        expect(left.handleKeyDown).not.toHaveBeenCalled()
    })

    it('an open volume chooser that ignores the key STILL swallows it (panes stay inert)', () => {
        const left = makePaneRef({
            isVolumeChooserOpen: vi.fn(() => true),
            handleVolumeChooserKeyDown: vi.fn(() => false),
        })
        const kd = setup({ left })
        kd.handleKeyDown(keyEvent('ArrowDown'))

        expect(left.handleKeyDown).not.toHaveBeenCalled()
    })

    it('a printable letter is captured into the type-to-jump buffer and not forwarded', () => {
        const left = makePaneRef()
        const kd = setup({ left })
        const e = keyEvent('a')

        kd.handleKeyDown(e)

        expect(left.handleJumpKeystroke).toHaveBeenCalledWith('a')
        expect(e.preventDefault).toHaveBeenCalled()
        expect(e.stopPropagation).toHaveBeenCalled()
        expect(left.handleKeyDown).not.toHaveBeenCalled()
    })

    it('once a jump is active, a printable non-letter (like "-") extends the buffer', () => {
        const left = makePaneRef({ isJumpActive: vi.fn(() => true) })
        const kd = setup({ left })

        kd.handleKeyDown(keyEvent('-'))

        expect(left.handleJumpKeystroke).toHaveBeenCalledWith('-')
        expect(left.handleKeyDown).not.toHaveBeenCalled()
    })

    it('a reset key clears the jump buffer and still forwards to the pane', () => {
        const left = makePaneRef()
        const kd = setup({ left })
        const e = keyEvent('ArrowDown')

        kd.handleKeyDown(e)

        expect(left.clearJumpState).toHaveBeenCalled()
        expect(left.handleKeyDown).toHaveBeenCalledWith(e)
    })

    it('does not intercept jump keys while renaming (forwards instead)', () => {
        const left = makePaneRef({ isRenaming: vi.fn(() => true) })
        const kd = setup({ left })
        const e = keyEvent('a')

        kd.handleKeyDown(e)

        expect(left.handleJumpKeystroke).not.toHaveBeenCalled()
        expect(left.handleKeyDown).toHaveBeenCalledWith(e)
    })

    it('handleKeyUp forwards to the focused pane', () => {
        const left = makePaneRef()
        const kd = setup({ left })
        const e = keyEvent('Shift')

        kd.handleKeyUp(e)

        expect(left.handleKeyUp).toHaveBeenCalledWith(e)
    })

    describe('handleFocusGuard', () => {
        it('refocuses the container when focus escapes to a non-exempt element', () => {
            const container = document.createElement('div')
            const focusSpy = vi.spyOn(container, 'focus')
            const kd = setup({}, container)
            const button = document.createElement('button')

            kd.handleFocusGuard({ target: button } as unknown as FocusEvent)

            expect(focusSpy).toHaveBeenCalled()
        })

        it('leaves focus alone on input elements', () => {
            const container = document.createElement('div')
            const focusSpy = vi.spyOn(container, 'focus')
            const kd = setup({}, container)
            const input = document.createElement('input')

            kd.handleFocusGuard({ target: input } as unknown as FocusEvent)

            expect(focusSpy).not.toHaveBeenCalled()
        })

        it('leaves focus alone inside dialog content (the trapFocus ping-pong guard)', () => {
            const container = document.createElement('div')
            const focusSpy = vi.spyOn(container, 'focus')
            const kd = setup({}, container)
            const dialog = document.createElement('div')
            dialog.setAttribute('role', 'dialog')
            const inner = document.createElement('button')
            dialog.appendChild(inner)

            kd.handleFocusGuard({ target: inner } as unknown as FocusEvent)

            expect(focusSpy).not.toHaveBeenCalled()
        })

        it('leaves focus alone when the container itself receives focus', () => {
            const container = document.createElement('div')
            const focusSpy = vi.spyOn(container, 'focus')
            const kd = setup({}, container)

            kd.handleFocusGuard({ target: container } as unknown as FocusEvent)

            expect(focusSpy).not.toHaveBeenCalled()
        })
    })

    it('isTypingInInput detects text-entry controls', () => {
        const input = document.createElement('input')
        expect(isTypingInInput({ target: input } as unknown as KeyboardEvent)).toBe(true)
        const div = document.createElement('div')
        expect(isTypingInInput({ target: div } as unknown as KeyboardEvent)).toBe(false)
    })
})
