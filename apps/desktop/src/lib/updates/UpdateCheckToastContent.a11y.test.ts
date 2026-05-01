/**
 * Tier 3 a11y tests for `UpdateCheckToastContent.svelte`.
 *
 * The toast reads `updateState` and renders one of five strings (Checking, No updates, Downloading,
 * Installing, Error). The error case adds a "Send error report" link button. Cover the default and
 * error paths.
 */

import { afterEach, beforeEach, describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/error-reporter/error-report-flow.svelte', () => ({
  openErrorReportDialog: vi.fn(),
}))
vi.mock('$lib/ui/toast', () => ({
  addToast: vi.fn(),
  dismissToast: vi.fn(),
}))
vi.mock('$lib/settings-store', () => ({
  loadSettings: vi.fn(async () => ({ isOnboarded: false })),
  saveSettings: vi.fn(async () => {}),
}))
vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => 60 * 60 * 1000),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(async () => '1.2.3') }))
vi.mock('@tauri-apps/plugin-updater', () => ({ check: vi.fn() }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: () => {}, info: () => {}, warn: () => {}, error: () => {} }),
}))

import UpdateCheckToastContent from './UpdateCheckToastContent.svelte'
import { _resetUpdaterStateForTest, _setUpdateStatusForTest, updateState } from './updater.svelte'

describe('UpdateCheckToastContent a11y', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('checking state has no a11y violations', async () => {
    updateState.previousVersion = '1.2.3'
    _setUpdateStatusForTest('checking')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(UpdateCheckToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('error state with Send error report link has no a11y violations', async () => {
    updateState.error = 'kaboom'
    _setUpdateStatusForTest('idle')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(UpdateCheckToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })
})
