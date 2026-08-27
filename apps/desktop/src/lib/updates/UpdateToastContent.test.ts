/**
 * Component tests for `UpdateToastContent.svelte`, the persistent restart prompt.
 *
 * The toast reads `previousVersion` / `nextVersion` off the reactive `updateState` singleton for
 * its version row, so the tests drive that module directly rather than the whole updater.
 */

import { afterEach, beforeEach, describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'

const { relaunchMock, dismissToastMock } = vi.hoisted(() => ({
  relaunchMock: vi.fn(() => Promise.resolve()),
  dismissToastMock: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: relaunchMock }))
vi.mock('$lib/ui/toast', () => ({ dismissToast: dismissToastMock }))

import UpdateToastContent from './UpdateToastContent.svelte'
import { updateState } from './update-state.svelte'

function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(UpdateToastContent, { target, props: {} })
  return target
}

function resetState() {
  updateState.status = 'idle'
  updateState.update = null
  updateState.error = null
  updateState.previousVersion = null
  updateState.nextVersion = null
}

describe('UpdateToastContent', () => {
  beforeEach(() => {
    resetState()
    relaunchMock.mockClear()
    dismissToastMock.mockClear()
  })

  afterEach(resetState)

  it('leads with the headline and the reassurance that doing nothing still lands the update', async () => {
    const target = render()
    await tick()
    expect(target.textContent).toContain('A new version of Cmdr is ready.')
    // The whole point of the second line: "Later" must not read as "skip this update".
    expect(target.textContent).toContain("Restart now, or you'll get it the next time you open Cmdr.")
  })

  it('shows both versions once the state knows them', async () => {
    updateState.previousVersion = '0.28.3'
    updateState.nextVersion = '0.29.0'
    const target = render()
    await tick()
    expect(target.textContent).toContain('v0.28.3 → v0.29.0')
  })

  it('gives the version row an accessible name, so the arrow is never read as a bare symbol', async () => {
    updateState.previousVersion = '0.28.3'
    updateState.nextVersion = '0.29.0'
    const target = render()
    await tick()
    const row = target.querySelector('[role="img"]')
    expect(row?.getAttribute('aria-label')).toBe('Updating from version 0.28.3 to version 0.29.0')
  })

  it('drops the version row when only one end is known, rather than rendering a half arrow', async () => {
    updateState.previousVersion = '0.28.3'
    updateState.nextVersion = null
    const target = render()
    await tick()
    expect(target.querySelector('[role="img"]')).toBeNull()
    expect(target.textContent).not.toContain('→')
  })

  it('restarts on the primary button and only dismisses on the secondary', async () => {
    const target = render()
    await tick()
    const buttons = [...target.querySelectorAll('button')]
    const later = buttons.find((b) => b.textContent.includes('Later'))
    const restart = buttons.find((b) => b.textContent.includes('Restart now'))

    later?.click()
    expect(dismissToastMock).toHaveBeenCalledWith('update')
    expect(relaunchMock).not.toHaveBeenCalled()

    restart?.click()
    expect(relaunchMock).toHaveBeenCalledTimes(1)
  })
})
