/**
 * Component tests for `UpdatesSection.svelte`.
 *
 * Verifies the "Check for updates" button + status text behave correctly across update phases,
 * and that the error case renders a "Send error report" link wired to `openErrorReportDialog`.
 */

import { afterEach, beforeEach, describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'

const { openErrorReportDialogMock, checkForUpdatesMock } = vi.hoisted(() => ({
  openErrorReportDialogMock: vi.fn(),
  checkForUpdatesMock: vi.fn(async () => {}),
}))

vi.mock('$lib/error-reporter/error-report-flow.svelte', () => ({
  openErrorReportDialog: openErrorReportDialogMock,
}))

// Stubs the updater module so this test isolates the section's UI logic. We use the real
// updater's reactive `updateState` so the section's `$derived`s react correctly to mutations.
import { updateState as realUpdateState, _resetUpdaterStateForTest } from '$lib/updates/updater.svelte'

vi.mock('$lib/updates/updater.svelte', async () => {
  const real = await vi.importActual<typeof import('$lib/updates/updater.svelte')>('$lib/updates/updater.svelte')
  return {
    ...real,
    checkForUpdates: checkForUpdatesMock,
  }
})

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => true),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(async () => '1.2.3') }))
vi.mock('@tauri-apps/plugin-updater', () => ({ check: vi.fn() }))
vi.mock('$lib/settings-store', () => ({
  loadSettings: vi.fn(async () => ({ isOnboarded: false })),
  saveSettings: vi.fn(async () => {}),
}))
vi.mock('$lib/ui/toast', () => ({
  addToast: vi.fn(),
  dismissToast: vi.fn(),
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: () => {}, info: () => {}, warn: () => {}, error: () => {} }),
}))

import UpdatesSection from './UpdatesSection.svelte'

function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(UpdatesSection, { target, props: { searchQuery: '' } })
  return target
}

function getCheckButton(target: HTMLElement): HTMLButtonElement {
  const btn = Array.from(target.querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Check for updates')
  if (!btn) throw new Error('Check for updates button missing')
  return btn
}

describe('UpdatesSection', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
    openErrorReportDialogMock.mockClear()
    checkForUpdatesMock.mockClear()
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('renders the Check for updates button enabled when status is idle', async () => {
    const target = render()
    await tick()
    const btn = getCheckButton(target)
    expect(btn.disabled).toBe(false)
  })

  it('disables the button while status is not idle', async () => {
    realUpdateState.status = 'checking'
    const target = render()
    await tick()
    expect(getCheckButton(target).disabled).toBe(true)
  })

  it('clicking the button calls checkForUpdates', async () => {
    const target = render()
    await tick()
    getCheckButton(target).click()
    expect(checkForUpdatesMock).toHaveBeenCalledTimes(1)
  })

  it('shows the no-updates message after an idle check', async () => {
    realUpdateState.previousVersion = '1.2.3'
    const target = render()
    await tick()
    expect(target.textContent).toContain('No updates found. Current version: v1.2.3')
  })

  it('shows the downloading status with both versions', async () => {
    realUpdateState.previousVersion = '1.2.3'
    realUpdateState.nextVersion = '1.3.0'
    realUpdateState.status = 'downloading'
    const target = render()
    await tick()
    expect(target.textContent).toContain('Update found, downloading v1.3.0 (current: v1.2.3)…')
  })

  it('renders an error and a Send error report link when error is set, calling openErrorReportDialog with the formatted note', async () => {
    realUpdateState.error = 'something exploded'
    const target = render()
    await tick()
    expect(target.textContent).toContain('Error: something exploded')
    const link = Array.from(target.querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Send error report',
    )
    expect(link).toBeTruthy()
    link?.click()
    expect(openErrorReportDialogMock).toHaveBeenCalledWith('Update check failed: something exploded')
  })
})
