/**
 * The sheet a tool raised: `select_volume` onto a pinned server, under someone typing in the rail.
 * Their text mustn't land in the password field, and their Enter mustn't send it as an attempt.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import SignInSheet from './SignInSheet.svelte'

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  hasServerSecret: vi.fn(() => Promise.resolve(false)),
  getKnownSftpServers: vi.fn(() => Promise.resolve([])),
  getKnownWebdavServers: vi.fn(() => Promise.resolve([])),
}))

describe('raised while someone types in the rail', () => {
  it('parks focus away from the password field', async () => {
    const page = document.createElement('div')
    page.innerHTML = '<aside class="ask-cmdr-rail"><textarea></textarea></aside><div class="host"></div>'
    document.body.appendChild(page)
    page.querySelector('textarea')?.focus()
    const host = page.querySelector('.host') as HTMLElement

    const sheet = mount(SignInSheet, {
      target: host,
      props: {
        request: {
          mode: 'sign-in',
          endpoint: {
            protocol: 'sftp',
            displayName: 'Testnas',
            address: 'test_user_1@testnas.local:22',
            host: 'testnas.local',
            username: 'test_user_1',
          },
          shape: { kind: 'username_password', guestAllowed: false },
          remembered: false,
          attempt: () => Promise.resolve({ kind: 'connected', volumeId: 'v' }),
        },
        onDone: () => {},
      },
    })
    for (let turn = 0; turn < 8; turn++) await tick()

    expect(host.querySelector('[data-armed]')).not.toBeNull()
    expect(document.activeElement?.tagName).not.toBe('INPUT')
    await unmount(sheet)
    page.remove()
  })

  it('leaves the add form alone: only a person opens it, by clicking Add', async () => {
    const page = document.createElement('div')
    page.innerHTML = '<aside class="ask-cmdr-rail"><textarea></textarea></aside><div class="host"></div>'
    document.body.appendChild(page)
    page.querySelector('textarea')?.focus()
    const host = page.querySelector('.host') as HTMLElement

    const sheet = mount(SignInSheet, {
      target: host,
      props: {
        request: { mode: 'add', attempt: () => Promise.resolve({ kind: 'connected', volumeId: 'v' }) },
        onDone: () => {},
      },
    })
    for (let turn = 0; turn < 8; turn++) await tick()

    expect(host.querySelector('[data-armed]')).toBeNull()
    await unmount(sheet)
    page.remove()
  })
})
