/**
 * What each security notice says and what its buttons do (#457). An inert button is the one
 * thing a notice must not have, so every action is clicked here.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { AccessNotice } from '#lib/ipc/bindings.js'
import type { ReportIncident } from '#lib/error-reporter/report-incident.js'

const { ipc, dismissToast, openSettingsWindow, dismissSecurityNotice, openErrorReportFor } = vi.hoisted(() => ({
  ipc: {
    reconnectVolume: vi.fn(() => Promise.resolve()),
    disconnectSmbVolume: vi.fn(() => Promise.resolve()),
    openExternalUrl: vi.fn(() => Promise.resolve()),
  },
  dismissToast: vi.fn(),
  openSettingsWindow: vi.fn(() => Promise.resolve()),
  dismissSecurityNotice: vi.fn(),
  openErrorReportFor: vi.fn(),
}))
vi.mock('#lib/tauri-commands/index.js', () => ipc)
vi.mock('#lib/ui/toast/index.js', () => ({ dismissToast }))
vi.mock('#lib/settings/settings-window.js', () => ({ openSettingsWindow }))
vi.mock('#lib/stores/volume-store.svelte.js', () => ({
  getVolumes: () => [{ id: 'smb-nas-photos', name: 'photos' }],
}))
vi.mock('./dismissed-notices', () => ({ dismissSecurityNotice }))
vi.mock('#lib/error-reporter/error-report-flow.svelte', () => ({ openErrorReportFor }))

import SecurityNoticeToastContent from './SecurityNoticeToastContent.svelte'

async function mountNotice(notice: AccessNotice, count = 1, report?: ReportIncident): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SecurityNoticeToastContent, { target, props: { toastId: 'toast-1', notice, count, report } })
  await tick()
  return target
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

describe('a web page at the MCP server', () => {
  it('says so without naming the page, and opens MCP settings or dismisses', async () => {
    const notice: AccessNotice = { kind: 'mcpForeignOrigin' }
    const target = await mountNotice(notice)
    expect(target.textContent).toContain('Cmdr blocked a web page from reaching the MCP server')
    click(target, 'MCP settings')
    await vi.waitFor(() => {
      expect(openSettingsWindow).toHaveBeenCalledWith('security-notice-toast', ['AI', 'MCP server'])
    })
    click(target, 'Dismiss')
    expect(dismissSecurityNotice).toHaveBeenCalledWith(notice)
  })

  it('counts the repeats once there are some', async () => {
    const once = await mountNotice({ kind: 'mcpForeignOrigin' }, 1)
    expect(once.textContent).not.toContain('since you opened Cmdr')
    const many = await mountNotice({ kind: 'mcpForeignOrigin' }, 1_234)
    expect(many.textContent).toContain('1,234 times since you opened Cmdr')
  })
})

describe('a refused server reply', () => {
  it('names the share, and reconnects or disconnects it', async () => {
    const target = await mountNotice({ kind: 'smbSignatureRefused', volumeId: 'smb-nas-photos' })
    expect(target.textContent).toContain('Cmdr refused a reply from photos')
    click(target, 'Reconnect')
    expect(ipc.reconnectVolume).toHaveBeenCalledWith('smb-nas-photos')
    click(target, 'Disconnect')
    expect(ipc.disconnectSmbVolume).toHaveBeenCalledWith('smb-nas-photos')
  })

  it('still reads once the share is gone from the volume list', async () => {
    const target = await mountNotice({ kind: 'smbSignatureRefused', volumeId: 'smb-gone' })
    expect(target.textContent).toContain('Cmdr refused a reply from a server')
  })
})

describe('an update that isn’t genuine', () => {
  it('says nothing was installed, and opens the website or the report', async () => {
    const report: ReportIncident = { kind: 'updateNotGenuine', version: '0.53.0', check: 'minisign' }
    const target = await mountNotice({ kind: 'updateNotGenuine', version: '0.53.0', check: 'minisign' }, 1, report)
    expect(target.textContent).toContain('it isn’t a genuine Cmdr release')
    click(target, 'Open getcmdr.com')
    expect(ipc.openExternalUrl).toHaveBeenCalledWith('https://getcmdr.com')
    click(target, 'Send error report…')
    expect(openErrorReportFor).toHaveBeenCalledWith(report)
  })
})
