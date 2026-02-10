import { describe, it, expect, vi } from 'vitest'
import { mount } from 'svelte'
import FunctionKeyBar from './FunctionKeyBar.svelte'

describe('FunctionKeyBar', () => {
    it('renders 6 buttons when visible', () => {
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true } })

        const buttons = target.querySelectorAll('button')
        expect(buttons).toHaveLength(6)
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
        // F8 (index 5) is still disabled
        expect(buttons[5].disabled).toBe(true)
    })

    it('enables F3, F4, F5, F6, and F7 buttons', () => {
        const target = document.createElement('div')
        mount(FunctionKeyBar, {
            target,
            props: {
                visible: true,
                onView: () => {},
                onEdit: () => {},
                onCopy: () => {},
                onMove: () => {},
                onNewFolder: () => {},
            },
        })

        const buttons = target.querySelectorAll('button')
        // F3 (0), F4 (1), F5 (2), F6 (3), F7 (4)
        expect(buttons[0].disabled).toBe(false)
        expect(buttons[1].disabled).toBe(false)
        expect(buttons[2].disabled).toBe(false)
        expect(buttons[3].disabled).toBe(false)
        expect(buttons[4].disabled).toBe(false)
    })

    it('calls onView when F3 button is clicked', () => {
        const onView = vi.fn()
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true, onView } })

        const buttons = target.querySelectorAll('button')
        buttons[0].click()
        expect(onView).toHaveBeenCalledOnce()
    })

    it('calls onEdit when F4 button is clicked', () => {
        const onEdit = vi.fn()
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true, onEdit } })

        const buttons = target.querySelectorAll('button')
        buttons[1].click()
        expect(onEdit).toHaveBeenCalledOnce()
    })

    it('calls onCopy when F5 button is clicked', () => {
        const onCopy = vi.fn()
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true, onCopy } })

        const buttons = target.querySelectorAll('button')
        buttons[2].click()
        expect(onCopy).toHaveBeenCalledOnce()
    })

    it('calls onMove when F6 button is clicked', () => {
        const onMove = vi.fn()
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true, onMove } })

        const buttons = target.querySelectorAll('button')
        buttons[3].click()
        expect(onMove).toHaveBeenCalledOnce()
    })

    it('calls onNewFolder when F7 button is clicked', () => {
        const onNewFolder = vi.fn()
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true, onNewFolder } })

        const buttons = target.querySelectorAll('button')
        buttons[4].click()
        expect(onNewFolder).toHaveBeenCalledOnce()
    })

    it('shows correct key labels', () => {
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true } })

        const kbds = target.querySelectorAll('kbd')
        const keys = Array.from(kbds).map((kbd) => kbd.textContent)
        expect(keys).toEqual(['F3', 'F4', 'F5', 'F6', 'F7', 'F8'])
    })

    it('shows correct action labels', () => {
        const target = document.createElement('div')
        mount(FunctionKeyBar, { target, props: { visible: true } })

        const buttons = target.querySelectorAll('button')
        const labels = Array.from(buttons).map((btn) => btn.querySelector('span')?.textContent)
        expect(labels).toEqual(['View', 'Edit', 'Copy', 'Move', 'New folder', 'Delete'])
    })
})
