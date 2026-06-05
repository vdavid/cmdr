import { describe, it, expect, vi } from 'vitest'
import { mount, flushSync } from 'svelte'
import FunctionKeyBar from './FunctionKeyBar.svelte'
import type { CommandId } from '$lib/commands'

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

  it('enables the source-op buttons by default (a real pane has canBeSource: true)', () => {
    const target = document.createElement('div')
    mount(FunctionKeyBar, { target, props: { visible: true } })

    const buttons = target.querySelectorAll('button')
    // F2 (0) … F8 (6). All enabled on a real folder (default props).
    for (const button of buttons) {
      expect(button.disabled).toBe(false)
    }
  })

  it('dispatches the matching file.* command for each default-state F-key', () => {
    const onCommand = vi.fn<(id: CommandId) => void>()
    const target = document.createElement('div')
    mount(FunctionKeyBar, { target, props: { visible: true, onCommand } })

    const buttons = target.querySelectorAll('button')
    // F2 Rename, F3 View, F4 Edit, F5 Copy, F6 Move, F7 New folder, F8 Delete.
    const expected: CommandId[] = [
      'file.rename',
      'file.view',
      'file.edit',
      'file.copy',
      'file.move',
      'file.newFolder',
      'file.delete',
    ]
    expected.forEach((commandId, index) => {
      buttons[index].click()
      expect(onCommand).toHaveBeenNthCalledWith(index + 1, commandId)
    })
  })

  it('dispatches the matching file.* command for each shift-state F-key', async () => {
    const onCommand = vi.fn<(id: CommandId) => void>()
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FunctionKeyBar, { target, props: { visible: true, onCommand } })

    await pressShift()

    const buttons = target.querySelectorAll('button')
    // Shift+F4 New file (2), Shift+F6 Rename (4), Shift+F8 Permanently (6).
    buttons[2].click()
    expect(onCommand).toHaveBeenLastCalledWith('file.newFile')
    buttons[4].click()
    expect(onCommand).toHaveBeenLastCalledWith('file.rename')
    buttons[6].click()
    expect(onCommand).toHaveBeenLastCalledWith('file.deletePermanently')

    await releaseShift()
    document.body.removeChild(target)
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
    expect(keys).toEqual(['F2', 'F3', '⇧F4', 'F5', '⇧F6', 'F7', '⇧F8'])

    // Shift+F4, Shift+F6, and Shift+F8 should have labels
    const buttons = target.querySelectorAll('button')
    const labels = Array.from(buttons).map((btn) => btn.querySelector('span')?.textContent ?? null)
    expect(labels).toEqual([null, null, 'New file', null, 'Rename', null, 'Permanently'])

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

  it('disables most buttons in shift state', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FunctionKeyBar, { target, props: { visible: true } })

    await pressShift()

    const buttons = target.querySelectorAll('button')
    // All disabled except Shift+F4 (index 2), Shift+F6 (index 4), and Shift+F8 (index 6)
    expect(buttons[0].disabled).toBe(true)
    expect(buttons[1].disabled).toBe(true)
    expect(buttons[2].disabled).toBe(false)
    expect(buttons[3].disabled).toBe(true)
    expect(buttons[4].disabled).toBe(false)
    expect(buttons[5].disabled).toBe(true)
    expect(buttons[6].disabled).toBe(false)

    await releaseShift()
    document.body.removeChild(target)
  })
})
