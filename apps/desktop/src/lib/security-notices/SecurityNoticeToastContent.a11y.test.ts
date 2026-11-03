/**
 * Tier 3 a11y tests for `SecurityNoticeToastContent.svelte`: every notice kind, with and without
 * its count line.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { AccessNotice } from '#lib/ipc/bindings.js'
import { expectNoA11yViolations } from '#lib/test-a11y.js'

vi.mock('#lib/tauri-commands/index.js', () => ({
  reconnectVolume: () => Promise.resolve(),
  disconnectSmbVolume: () => Promise.resolve(),
  openExternalUrl: () => Promise.resolve(),
}))
vi.mock('#lib/ui/toast/index.js', () => ({ dismissToast: (): void => {} }))
vi.mock('#lib/settings/settings-window.js', () => ({ openSettingsWindow: () => Promise.resolve() }))
vi.mock('#lib/stores/volume-store.svelte.js', () => ({ getVolumes: () => [{ id: 'smb-nas-photos', name: 'photos' }] }))

import SecurityNoticeToastContent from './SecurityNoticeToastContent.svelte'

const notices: AccessNotice[] = [
  { kind: 'mcpForeignOrigin' },
  { kind: 'smbSignatureRefused', volumeId: 'smb-nas-photos' },
  { kind: 'updateNotGenuine', version: '0.53.0', check: 'code_signature' },
]

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('SecurityNoticeToastContent a11y', () => {
  for (const notice of notices) {
    it(`${notice.kind} has no a11y violations, counted or not`, async () => {
      for (const count of [1, 12]) {
        const target = document.createElement('div')
        document.body.appendChild(target)
        mount(SecurityNoticeToastContent, {
          target,
          props: {
            toastId: 'toast-1',
            notice,
            count,
            report:
              notice.kind === 'updateNotGenuine'
                ? { kind: 'updateNotGenuine', version: notice.version, check: notice.check }
                : undefined,
          },
        })
        await tick()
        await expectNoA11yViolations(target)
      }
    })
  }
})
