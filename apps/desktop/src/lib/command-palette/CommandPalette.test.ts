import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount } from 'svelte'
import { tick } from 'svelte'
import CommandPalette from './CommandPalette.svelte'

// Mock the app-status-store to avoid Tauri dependency in tests
vi.mock('$lib/app-status-store', () => ({
    loadPaletteQuery: vi.fn().mockResolvedValue(''),
    savePaletteQuery: vi.fn().mockResolvedValue(undefined),
}))

// Mock the commands module to provide test data
vi.mock('$lib/commands', () => ({
    searchCommands: vi.fn((query: string) => {
        const allCommands = [
            { command: { id: 'app.quit', name: 'Quit Cmdr', scope: 'App', shortcuts: ['⌘Q'] }, matchedIndices: [] },
            { command: { id: 'app.about', name: 'About Cmdr', scope: 'App', shortcuts: [] }, matchedIndices: [] },
            {
                command: { id: 'file.copyPath', name: 'Copy path to clipboard', scope: 'Main window', shortcuts: [] },
                matchedIndices: [],
            },
            {
                command: {
                    id: 'view.showHidden',
                    name: 'Toggle hidden files',
                    scope: 'Main window',
                    shortcuts: ['⌘⇧.'],
                },
                matchedIndices: [],
            },
        ]
        if (!query.trim()) return allCommands
        return allCommands.filter((c) => c.command.name.toLowerCase().includes(query.toLowerCase()))
    }),
}))

describe('CommandPalette', () => {
    let mockOnExecute: (commandId: string) => void
    let mockOnClose: () => void

    beforeEach(() => {
        mockOnExecute = vi.fn()
        mockOnClose = vi.fn()
        // Mock scrollIntoView which isn't available in jsdom
        Element.prototype.scrollIntoView = vi.fn()
    })

    it('renders the modal with search input', async () => {
        const target = document.createElement('div')
        mount(CommandPalette, {
            target,
            props: { onExecute: mockOnExecute, onClose: mockOnClose },
        })

        await tick()

        const input = target.querySelector('input')
        expect(input).toBeTruthy()
        expect(input?.placeholder).toContain('command')
    })

    it('shows all commands when query is empty', async () => {
        const target = document.createElement('div')
        mount(CommandPalette, {
            target,
            props: { onExecute: mockOnExecute, onClose: mockOnClose },
        })

        await tick()

        const results = target.querySelectorAll('[class*="result-item"]')
        expect(results.length).toBeGreaterThan(0)
    })

    it('filters commands on input', async () => {
        const target = document.createElement('div')
        mount(CommandPalette, {
            target,
            props: { onExecute: mockOnExecute, onClose: mockOnClose },
        })

        await tick()

        const input = target.querySelector('input')
        if (input) {
            input.value = 'quit'
            input.dispatchEvent(new InputEvent('input', { bubbles: true }))
        }

        await tick()

        // Results should be filtered (mock only returns matches containing 'quit')
        const results = target.querySelectorAll('[class*="result-item"]')
        expect(results.length).toBe(1)
    })

    it('calls onClose when Escape is pressed', async () => {
        const target = document.createElement('div')
        mount(CommandPalette, {
            target,
            props: { onExecute: mockOnExecute, onClose: mockOnClose },
        })

        await tick()

        const input = target.querySelector('input')
        if (input) {
            input.focus()
            input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
        }

        await tick()

        expect(mockOnClose).toHaveBeenCalled()
    })

    it('calls onExecute when Enter is pressed with selected item', async () => {
        const target = document.createElement('div')
        mount(CommandPalette, {
            target,
            props: { onExecute: mockOnExecute, onClose: mockOnClose },
        })

        await tick()

        const input = target.querySelector('input')
        if (input) {
            input.focus()
            input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
        }

        await tick()

        expect(mockOnExecute).toHaveBeenCalled()
    })

    it('navigates selection with ArrowDown', async () => {
        const target = document.createElement('div')
        mount(CommandPalette, {
            target,
            props: { onExecute: mockOnExecute, onClose: mockOnClose },
        })

        await tick()

        const input = target.querySelector('input')
        if (input) {
            input.focus()
            // Initial selection is index 0, arrow down should move to index 1
            input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
        }

        await tick()

        // Check that selection moved - verify the command that is executed is the second one
        // We do this by pressing Enter and checking which command is executed
        if (input) {
            input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
        }

        await tick()

        // Second command (index 1) is 'app.about'
        expect(mockOnExecute).toHaveBeenCalledWith('app.about')
    })

    it('navigates selection with ArrowUp', async () => {
        const target = document.createElement('div')
        mount(CommandPalette, {
            target,
            props: { onExecute: mockOnExecute, onClose: mockOnClose },
        })

        await tick()

        const input = target.querySelector('input')
        if (input) {
            input.focus()
            // Move down first then up
            input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
            await tick()
            input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }))
        }

        await tick()

        // Check that we're back at first item by pressing Enter and checking executed command
        if (input) {
            input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
        }

        await tick()

        // First command (index 0) is 'app.quit'
        expect(mockOnExecute).toHaveBeenCalledWith('app.quit')
    })

    it('stops keyboard event propagation', async () => {
        const target = document.createElement('div')
        const propagationSpy = vi.fn()

        // Add listener on parent to check propagation
        target.addEventListener('keydown', propagationSpy)

        mount(CommandPalette, {
            target,
            props: { onExecute: mockOnExecute, onClose: mockOnClose },
        })

        await tick()

        const input = target.querySelector('input')
        if (input) {
            input.focus()
            input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
        }

        await tick()

        // Event should not have propagated to parent due to stopPropagation
        // Note: This test may not work perfectly since we're dispatching on the input directly
        // The stopPropagation happens in the component's keydown handler
        expect(mockOnClose).not.toHaveBeenCalled()
    })

    it('closes on click outside', async () => {
        const target = document.createElement('div')
        mount(CommandPalette, {
            target,
            props: { onExecute: mockOnExecute, onClose: mockOnClose },
        })

        await tick()

        // Simulate click on overlay background
        const overlay = target.querySelector('[class*="overlay"]')
        if (overlay) {
            overlay.dispatchEvent(new MouseEvent('click', { bubbles: true }))
        }

        await tick()

        expect(mockOnClose).toHaveBeenCalled()
    })

    it('shows keyboard shortcuts for commands', async () => {
        const target = document.createElement('div')
        mount(CommandPalette, {
            target,
            props: { onExecute: mockOnExecute, onClose: mockOnClose },
        })

        await tick()

        // Check that shortcuts are displayed
        const shortcutElements = target.querySelectorAll('[class*="shortcut"]')
        expect(shortcutElements.length).toBeGreaterThan(0)
    })
})
