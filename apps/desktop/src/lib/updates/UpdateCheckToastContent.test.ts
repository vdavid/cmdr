/**
 * Component tests for `UpdateCheckToastContent.svelte`.
 *
 * The toast reads the reactive `updateState` singleton and renders the corresponding status
 * string for each phase. The error case renders a "Send error report" link button.
 */

import { afterEach, beforeEach, describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'

const { openErrorReportDialogMock } = vi.hoisted(() => ({
  openErrorReportDialogMock: vi.fn(),
}))

vi.mock('$lib/error-reporter/error-report-flow.svelte', () => ({
  openErrorReportDialog: openErrorReportDialogMock,
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: vi.fn(),
  dismissToast: vi.fn(),
}))
vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => 60 * 60 * 1000),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(() => Promise.resolve('1.2.3')) }))
vi.mock('@tauri-apps/plugin-updater', () => ({ check: vi.fn() }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: () => {}, info: () => {}, warn: () => {}, error: () => {} }),
}))

import UpdateCheckToastContent from './UpdateCheckToastContent.svelte'
import { _resetUpdaterStateForTest, _setUpdateStatusForTest, updateState } from './updater.svelte'

function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(UpdateCheckToastContent, { target, props: {} })
  return target
}

describe('UpdateCheckToastContent', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
    openErrorReportDialogMock.mockClear()
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('renders the checking string while status is checking', async () => {
    updateState.previousVersion = '1.2.3'
    _setUpdateStatusForTest('checking')
    const target = render()
    await tick()
    expect(target.textContent).toContain('Checking…')
  })

  it('renders the no-updates message after a successful check', async () => {
    updateState.previousVersion = '1.2.3'
    _setUpdateStatusForTest('idle')
    const target = render()
    await tick()
    expect(target.textContent).toContain('No updates found. Current version: v1.2.3')
  })

  it('renders the downloading string with both versions', async () => {
    updateState.previousVersion = '1.2.3'
    updateState.nextVersion = '1.3.0'
    _setUpdateStatusForTest('downloading')
    const target = render()
    await tick()
    expect(target.textContent).toContain('Update found, downloading v1.3.0 (current: v1.2.3)…')
  })

  it('renders the installing string with both versions', async () => {
    updateState.previousVersion = '1.2.3'
    updateState.nextVersion = '1.3.0'
    _setUpdateStatusForTest('installing')
    const target = render()
    await tick()
    expect(target.textContent).toContain('Installing v1.3.0 (current: v1.2.3)…')
  })

  it('words a check the network didn’t carry from the catalog, with no "Error:" and no report link', async () => {
    updateState.failure = {
      phase: 'check',
      request: { type: 'unreachable', detail: 'error sending request for url (https://api.getcmdr.com/update-check)' },
    }
    _setUpdateStatusForTest('idle')
    const target = render()
    await tick()
    expect(target.textContent).toContain('Cmdr couldn’t check for updates.')
    expect(target.textContent).toContain('Check your internet connection')
    expect(target.textContent).not.toContain('Error:')
    expect(target.textContent).not.toContain('error sending request')
    const link = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Send error report')
    expect(link).toBeUndefined()
  })

  it('offers an error report for an install that didn’t finish, with the sentence it showed as the note', async () => {
    updateState.failure = { phase: 'install' }
    _setUpdateStatusForTest('idle')
    const target = render()
    await tick()
    const shown = target.querySelector('.message')?.textContent.trim() ?? ''
    expect(shown).toContain('Cmdr couldn’t install the update.')
    const link = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Send error report')
    expect(link).toBeTruthy()
    link?.click()
    expect(openErrorReportDialogMock).toHaveBeenCalledWith(shown)
  })
})
