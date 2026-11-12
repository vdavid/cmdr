/**
 * The teaching animation draws each OS's own keyboard and presses the binding it's given, so a
 * Windows or Linux reader sees Ctrl, Win or Super, and Alt light up where a Mac reader sees
 * control, option, and command.
 */
import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import type { Platform } from '#lib/platform/index.js'
import GlobalShortcutAnimation from './GlobalShortcutAnimation.svelte'

async function render(binding: string, platform: Platform): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(GlobalShortcutAnimation, { target, props: { binding, platform } })
  await tick()
  return target
}

/** The first label of each lit key cap, modifiers first, then the pressed key. */
function litLabels(target: HTMLElement): { modifiers: string[]; key: string[] } {
  const labelsOf = (selector: string) =>
    Array.from(target.querySelectorAll(selector), (group) => group.querySelector('text')?.textContent ?? '')
  return { modifiers: labelsOf('.mod-group'), key: labelsOf('.target-group') }
}

describe('GlobalShortcutAnimation', () => {
  it('presses ⌃⌥⌘J on Apple’s key caps on macOS', async () => {
    const target = await render('⌃⌥⌘J', 'macos')
    expect(litLabels(target)).toEqual({ modifiers: ['control', 'option', 'command'], key: ['J'] })
  })

  it('presses Ctrl+Alt+Win+J on a PC keyboard on Windows, with no Mac glyph anywhere', async () => {
    const target = await render('Ctrl+Alt+Super+J', 'windows')
    expect(litLabels(target)).toEqual({ modifiers: ['Ctrl', 'Win', 'Alt'], key: ['J'] })
    expect(target.textContent).not.toMatch(/[⌃⌥⌘]|command|option/)
  })

  it('presses Ctrl+Alt+Super+J on Linux', async () => {
    const target = await render('Ctrl+Alt+Super+J', 'linux')
    expect(litLabels(target)).toEqual({ modifiers: ['Ctrl', 'Super', 'Alt'], key: ['J'] })
  })

  it('follows a rebind: the bound key lights, and J stays dark', async () => {
    const target = await render('Ctrl+Shift+K', 'linux')
    expect(litLabels(target)).toEqual({ modifiers: ['Shift', 'Ctrl'], key: ['K'] })
    // Only the pressed key's homing bar animates; J's stays a plain bar.
    expect(target.querySelectorAll('.target-bar')).toHaveLength(0)
  })

  it('animates the homing bar of a pressed F or J', async () => {
    const target = await render('Ctrl+Alt+Super+F', 'windows')
    expect(target.querySelectorAll('.target-bar')).toHaveLength(1)
  })
})
