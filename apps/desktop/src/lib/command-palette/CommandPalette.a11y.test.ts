/**
 * Tier 3 a11y tests for `CommandPalette.svelte`.
 *
 * Own-overlay modal with fuzzy search. Tests cover populated (default)
 * and empty-query results states. Tauri and command registry are
 * mocked the same way `CommandPalette.test.ts` does it.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import CommandPalette from './CommandPalette.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/app-status-store', () => ({
  loadPaletteQuery: vi.fn(() => Promise.resolve('')),
  savePaletteQuery: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/commands', () => ({
  searchCommands: vi.fn((query: string) => {
    const all = [
      { command: { id: 'app.quit', name: 'Quit Cmdr', scope: 'App', shortcuts: ['\u2318Q'] }, matchedIndices: [] },
      { command: { id: 'app.about', name: 'About Cmdr', scope: 'App', shortcuts: [] }, matchedIndices: [] },
      {
        command: {
          id: 'file.copyPath',
          name: 'Copy path to clipboard',
          scope: 'Main window',
          shortcuts: [],
        },
        matchedIndices: [],
      },
    ]
    if (!query.trim()) return all
    return all.filter((c) => c.command.name.toLowerCase().includes(query.toLowerCase()))
  }),
}))

describe('CommandPalette a11y', () => {
  beforeEach(() => {
    Element.prototype.scrollIntoView = vi.fn()
  })

  it('default (populated results) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CommandPalette, {
      target,
      props: { onExecute: () => {}, onClose: () => {} },
    })
    await tick()
    await new Promise((r) => setTimeout(r, 0))
    await tick()
    await expectNoA11yViolations(target)
  })
})
