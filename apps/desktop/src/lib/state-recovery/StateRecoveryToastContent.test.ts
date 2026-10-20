/**
 * What each recovery notice says and what its buttons do (#461). An inert button is the one
 * thing a notice must not have, so every action is clicked here.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { StateFile, StateRecovery } from '#lib/ipc/bindings.js'

const { showInFinder, dismissToast, openSettingsWindow } = vi.hoisted(() => ({
  showInFinder: vi.fn(() => Promise.resolve()),
  dismissToast: vi.fn(),
  openSettingsWindow: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/tauri-commands/index.js', () => ({ showInFinder }))
vi.mock('#lib/ui/toast/index.js', () => ({ dismissToast }))
vi.mock('#lib/settings/settings-window.js', () => ({ openSettingsWindow }))

import StateRecoveryToastContent from './StateRecoveryToastContent.svelte'

const KEPT = '/data/settings.json.broken-2026-10-16T101500'

async function mountNotice(recovery: StateRecovery): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(StateRecoveryToastContent, { target, props: { toastId: 'toast-1', recovery } })
  await tick()
  return target
}

function labels(target: HTMLElement): string[] {
  return [...target.querySelectorAll('button')].map((b) => b.textContent.trim())
}

function click(target: HTMLElement, label: string): void {
  const button = [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === label)
  expect(button, `a "${label}" button`).toBeDefined()
  button?.click()
}

beforeEach(() => {
  document.body.innerHTML = ''
  vi.clearAllMocks()
})

describe('a recovered settings file', () => {
  it('says the consents are off, names the copy, and opens Updates & privacy', async () => {
    const target = await mountNotice({ file: 'settings', keptCopy: KEPT })
    expect(target.textContent).toContain('Cmdr couldn’t read your settings, so it started with the defaults')
    expect(target.textContent).toContain('Usage stats and crash reports stay off')
    expect(target.textContent).toContain(`Cmdr kept the old file at ${KEPT}.`)
    click(target, 'Open Settings')
    await vi.waitFor(() => {
      expect(openSettingsWindow).toHaveBeenCalledWith('state-recovery-toast', ['Updates & privacy'])
    })
    await vi.waitFor(() => {
      expect(dismissToast).toHaveBeenCalledWith('toast-1')
    })
  })

  it('shows the copy in Finder', async () => {
    const target = await mountNotice({ file: 'settings', keptCopy: KEPT })
    click(target, 'Show in Finder')
    expect(showInFinder).toHaveBeenCalledWith(KEPT)
  })
})

describe('a recovered license file', () => {
  it('says how to get the license back, and opens the License section', async () => {
    const target = await mountNotice({ file: 'license', keptCopy: KEPT })
    expect(target.textContent).toContain('enter your license key again')
    click(target, 'Open Settings')
    await vi.waitFor(() => {
      expect(openSettingsWindow).toHaveBeenCalledWith('state-recovery-toast', ['License'])
    })
  })
})

describe('a recovered credential store', () => {
  it('says why the passwords are gone and that Cmdr asks again, with no settings button', async () => {
    const target = await mountNotice({ file: 'credentials', keptCopy: KEPT })
    expect(target.textContent).toMatch(/^Cmdr couldn’t read your saved passwords/)
    expect(target.textContent).toContain('asks for each password again')
    expect(labels(target)).toEqual(['Dismiss', 'Show in Finder'])
  })
})

describe('every other file', () => {
  const others: StateFile[] = [
    'shortcuts',
    'app_status',
    'operation_log',
    'chat_history',
    'favorites',
    'go_to_path_history',
    'search_history',
    'selection_history',
    'multi_rename_presets',
    'multi_rename_last_settings',
    'multi_rename_field_history',
    'launch_days',
    's3_places',
    'sftp_servers',
    'webdav_servers',
    'smb_shares',
    'smb_servers',
    'sftp_host_keys',
    'webdav_certificates',
    'local_ai',
  ]

  for (const file of others) {
    it(`${file} has its own headline, and no settings button`, async () => {
      const target = await mountNotice({ file, keptCopy: KEPT })
      expect(target.textContent).toMatch(/^Cmdr couldn’t read /)
      expect(labels(target)).toEqual(['Dismiss', 'Show in Finder'])
    })
  }

  it('drops the copy line and Finder button when nothing could be kept, and dismisses', async () => {
    const target = await mountNotice({ file: 'favorites', keptCopy: null })
    expect(target.textContent).not.toContain('kept the old file')
    expect(labels(target)).toEqual(['Dismiss'])
    click(target, 'Dismiss')
    expect(dismissToast).toHaveBeenCalledWith('toast-1')
  })
})
