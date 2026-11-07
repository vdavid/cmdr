/**
 * Component tests for `UpdateToastContent.svelte`, the persistent restart prompt.
 *
 * The toast reads `previousVersion` / `nextVersion` off the reactive `updateState` singleton for
 * its version row, so the tests drive that module directly rather than the whole updater.
 */

import { afterEach, beforeEach, describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'

const { relaunchAppMock, dismissToastMock, openExternalUrlMock } = vi.hoisted(() => ({
  relaunchAppMock: vi.fn(() => Promise.resolve()),
  dismissToastMock: vi.fn(),
  openExternalUrlMock: vi.fn(() => Promise.resolve()),
}))

vi.mock('#lib/ui/toast/index.js', () => ({ dismissToast: dismissToastMock }))
vi.mock('#lib/tauri-commands/index.js', () => ({ openExternalUrl: openExternalUrlMock, relaunchApp: relaunchAppMock }))

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
  updateState.failure = null
  updateState.previousVersion = null
  updateState.nextVersion = null
  updateState.securityNotice = null
}

describe('UpdateToastContent', () => {
  beforeEach(() => {
    resetState()
    relaunchAppMock.mockClear()
    dismissToastMock.mockClear()
    openExternalUrlMock.mockClear()
  })

  afterEach(resetState)

  it('leads with the headline and the reassurance that doing nothing still lands the update', async () => {
    const target = render()
    await tick()
    expect(target.textContent).toContain('A new version of Cmdr is ready.')
    // The whole point of the second line: "Later" must not read as "skip this update".
    expect(target.textContent).toContain('Restart now, or you’ll get it the next time you open Cmdr.')
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
    expect(relaunchAppMock).not.toHaveBeenCalled()

    restart?.click()
    expect(relaunchAppMock).toHaveBeenCalledTimes(1)
  })

  /** An update with nothing to say about security must read exactly as it always has. */
  it('renders the plain toast unchanged when the update carries no security notice', async () => {
    updateState.previousVersion = '0.28.3'
    updateState.nextVersion = '0.29.0'
    const target = render()
    await tick()
    // Scoped-CSS hashes and Svelte's empty block anchors are framework bookkeeping that moves with any
    // edit to the component; the elements, attributes, and copy are what must not.
    const markup = target.innerHTML.replace(/svelte-[a-z0-9]+/g, 'svelte-HASH').replaceAll('<!---->', '')
    expect(markup).toMatchInlineSnapshot(
      `"<div><span class="update-headline svelte-HASH">A new version of Cmdr is ready.</span> <span class="update-detail svelte-HASH">Restart now, or you’ll get it the next time you open Cmdr.</span> <span class="update-versions svelte-HASH" role="img" aria-label="Updating from version 0.28.3 to version 0.29.0">v0.28.3 → v0.29.0</span> <div class="toast-actions svelte-HASH"><button type="button" class="btn btn-secondary btn-mini svelte-HASH">Later</button> <button type="button" class="btn btn-primary btn-mini svelte-HASH">Restart now</button></div></div>"`,
    )
  })

  describe('when the update fixes a security issue', () => {
    const advisoryUrl = 'https://github.com/vdavid/cmdr/security/advisories/GHSA-aaaa-bbbb-cccc'

    it('says so, says what to do, and links the advisory', async () => {
      updateState.securityNotice = { severity: 'high', count: 1, url: advisoryUrl }
      const target = render()
      await tick()
      expect(target.textContent).toContain('This update fixes a security issue.')
      expect(target.textContent).toContain('Restart Cmdr to finish installing it.')
      expect(target.textContent).not.toContain('A new version of Cmdr is ready.')

      const link = [...target.querySelectorAll('a')].find((a) => a.textContent.includes('Read the advisory'))
      expect(link?.getAttribute('href')).toBe(advisoryUrl)
      link?.click()
      expect(openExternalUrlMock).toHaveBeenCalledWith(advisoryUrl)
    })

    it('counts several fixes', async () => {
      updateState.securityNotice = { severity: 'critical', count: 3, url: advisoryUrl }
      const target = render()
      await tick()
      expect(target.textContent).toContain('This update fixes 3 security issues.')
    })

    it('drops the link when the notice has none, keeping the rest', async () => {
      updateState.securityNotice = { severity: 'low', count: 1, url: null }
      const target = render()
      await tick()
      expect(target.textContent).toContain('This update fixes a security issue.')
      expect(target.querySelector('a')).toBeNull()
    })

    it('keeps both buttons: the notice never forces a restart', async () => {
      updateState.securityNotice = { severity: 'critical', count: 1, url: advisoryUrl }
      const target = render()
      await tick()
      const labels = [...target.querySelectorAll('button')].map((b) => b.textContent.trim())
      expect(labels).toEqual(['Later', 'Restart now'])
    })
  })
})
