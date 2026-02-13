import { describe, it, expect, vi } from 'vitest'
import { mount, flushSync } from 'svelte'
import FunctionKeyBar from './FunctionKeyBar.svelte'

describe('FunctionKeyBar', () => {
    /** Simulates pressing the Shift key, waits for effect and flushes Svelte reactivity. */
    async function pressShift() {
        document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Shift' }))
        // Effects run after microtask; flush to apply $state change to DOM
        await new Promise((r) => setTimeout(r, 0))
        flushSync()
    }

    /** Simulates releasing the Shift key, waits for effect and flushes Svelte reactivity. */
    async function releaseShift() {
        document.dispatchEvent(new KeyboardEvent('keyup', { key: 'Shift' }))
        await new Promise((r) => setTimeout(r, 0))
        flushSync()
    }

    it('renders 7 buttons when visible', () => {
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true } })

        const buttons = target.querySelectorAll('button')
        expect(buttons).toHaveLength(7)
    })

    it('renders nothing when visible is false', () => {
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: false } })

        expect(target.querySelector('.function-key-bar')).toBeNull()
    })

    it('disables only F8 button', () => {
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true } })

        const buttons = target.querySelectorAll('button')
        // F8 (index 6) is still disabled
        expect(buttons[6].disabled).toBe(true)
    })

    it('enables F2, F3, F4, F5, F6, and F7 buttons', () => {
        const target = document.createElement('div')
        mount(FunctionKeyBar, {
            target,
            props: {
                visible: true,
                onRename: () => {},
                onView: () => {},
                onEdit: () => {},
                onCopy: () => {},
                onMove: () => {},
                onNewFolder: () => {},
            },
        })

        const buttons = target.querySelectorAll('button')
        // F2 (0), F3 (1), F4 (2), F5 (3), F6 (4), F7 (5)
        expect(buttons[0].disabled).toBe(false)
        expect(buttons[1].disabled).toBe(false)
        expect(buttons[2].disabled).toBe(false)
        expect(buttons[3].disabled).toBe(false)
        expect(buttons[4].disabled).toBe(false)
        expect(buttons[5].disabled).toBe(false)
    })

    it('calls onRename when F2 button is clicked', () => {
        const onRename = vi.fn()
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true, onRename } })

        const buttons = target.querySelectorAll('button')
        buttons[0].click()
        expect(onRename).toHaveBeenCalledOnce()
    })

    it('calls onView when F3 button is clicked', () => {
        const onView = vi.fn()
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true, onView } })

        const buttons = target.querySelectorAll('button')
        buttons[1].click()
        expect(onView).toHaveBeenCalledOnce()
    })

    it('calls onEdit when F4 button is clicked', () => {
        const onEdit = vi.fn()
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true, onEdit } })

        const buttons = target.querySelectorAll('button')
        buttons[2].click()
        expect(onEdit).toHaveBeenCalledOnce()
    })

    it('calls onCopy when F5 button is clicked', () => {
        const onCopy = vi.fn()
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true, onCopy } })

        const buttons = target.querySelectorAll('button')
        buttons[3].click()
        expect(onCopy).toHaveBeenCalledOnce()
    })

    it('calls onMove when F6 button is clicked', () => {
        const onMove = vi.fn()
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true, onMove } })

        const buttons = target.querySelectorAll('button')
        buttons[4].click()
        expect(onMove).toHaveBeenCalledOnce()
    })

    it('calls onNewFolder when F7 button is clicked', () => {
        const onNewFolder = vi.fn()
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true, onNewFolder } })

        const buttons = target.querySelectorAll('button')
        buttons[5].click()
        expect(onNewFolder).toHaveBeenCalledOnce()
    })

    it('shows correct key labels', () => {
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true } })

        const kbds = target.querySelectorAll('kbd')
        const keys = Array.from(kbds).map((kbd) => kbd.textContent)
        expect(keys).toEqual(['F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8'])
    })

    it('shows correct action labels', () => {
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true } })

        const buttons = target.querySelectorAll('button')
        const labels = Array.from(buttons).map((btn) => btn.querySelector('span')?.textContent)
        expect(labels).toEqual(['Rename', 'View', 'Edit', 'Copy', 'Move', 'New folder', 'Delete'])
    })

    it('shows shift-state buttons when Shift is held', async () => {
        const target = document.createElement('div')
        document.body.appendChild(target)
        mount(FunctionKeyBar, { target, props: { visible: true } })

        await pressShift()

        const kbds = target.querySelectorAll('kbd')
        const keys = Array.from(kbds).map((kbd) => kbd.textContent)
        expect(keys).toEqual(['F2', 'F3', 'F4', 'F5', '\u21E7F6', 'F7', 'F8'])

        // Only Shift+F6 should have a label
        const buttons = target.querySelectorAll('button')
        const labels = Array.from(buttons).map((btn) => btn.querySelector('span')?.textContent ?? null)
        expect(labels).toEqual([null, null, null, null, 'Rename', null, null])

        await releaseShift()
        document.body.removeChild(target)
    })

    it('restores normal buttons when Shift is released', async () => {
        const target = document.createElement('div')
        document.body.appendChild(target)
        mount(FunctionKeyBar, { target, props: { visible: true } })

        await pressShift()
        await releaseShift()

        const kbds = target.querySelectorAll('kbd')
        const keys = Array.from(kbds).map((kbd) => kbd.textContent)
        expect(keys).toEqual(['F2', 'F3', 'F4', 'F5', 'F6', 'F7', 'F8'])

        document.body.removeChild(target)
    })

    it('calls onRename when Shift+F6 button is clicked in shift state', async () => {
        const onRename = vi.fn()
        const target = document.createElement('div')
        document.body.appendChild(target)
        mount(FunctionKeyBar, { target, props: { visible: true, onRename } })

        await pressShift()

        const buttons = target.querySelectorAll('button')
        // Shift+F6 is at index 4
        buttons[4].click()
        expect(onRename).toHaveBeenCalledOnce()

        await releaseShift()
        document.body.removeChild(target)
    })

    it('disables most buttons in shift state', async () => {
        const target = document.createElement('div')
        document.body.appendChild(target)
        mount(FunctionKeyBar, { target, props: { visible: true, onRename: () => {} } })

        await pressShift()

        const buttons = target.querySelectorAll('button')
        // All disabled except Shift+F6 (index 4)
        expect(buttons[0].disabled).toBe(true)
        expect(buttons[1].disabled).toBe(true)
        expect(buttons[2].disabled).toBe(true)
        expect(buttons[3].disabled).toBe(true)
        expect(buttons[4].disabled).toBe(false)
        expect(buttons[5].disabled).toBe(true)
        expect(buttons[6].disabled).toBe(true)

        await releaseShift()
        document.body.removeChild(target)
    })
})
