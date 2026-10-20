/**
 * The sheet's trust step for a WebDAV server whose certificate macOS refused (a
 * self-signed NAS): it pins exactly the fingerprint on screen, dials again, and
 * meets a changed certificate with the alarm's face, never first contact's.
 */

import { describe, expect, it, vi, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import SignInSheet from './SignInSheet.svelte'
import type { SignInAttemptOutcome, SignInSheetRequest } from './sign-in-contract'

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  hasServerSecret: vi.fn(() => Promise.resolve(false)),
  getKnownWebdavServers: vi.fn(() => Promise.resolve([])),
  getWebdavUnattendedReconnect: vi.fn(() => Promise.resolve('possible')),
  approveWebdavCertificate: vi.fn(() => Promise.resolve()),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(() => Promise.resolve(null)) }))

afterEach(() => {
  document.body.innerHTML = ''
})

const endpoint = {
  protocol: 'webdav' as const,
  displayName: 'Naspolya',
  address: 'https://nas.local:5006/',
  host: 'nas.local',
  username: 'ada',
}

/** Drains the sheet's `onMount` seeding and any attempt in flight. */
async function flush(times = 8) {
  for (let i = 0; i < times; i++) {
    await Promise.resolve()
    await tick()
  }
}

async function renderSheet(request: SignInSheetRequest) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SignInSheet, { target, props: { request, onDone: () => {} } })
  await flush()
}

/** The sheet's own buttons live in the portalled dialog. */
function buttonSaying(text: string): HTMLButtonElement {
  const match = [...document.body.querySelectorAll('button')].find((b) => b.textContent.includes(text))
  if (!match) throw new Error(`no button saying ${text}`)
  return match
}

/** A self-signed NAS's certificate, as a WebDAV dial hands it over. */
const certificate = (fingerprint: string, kind: 'unknown' | 'changed') =>
  ({
    subject: 'certificate',
    prompt: { host: 'nas.local', port: 5006, fingerprint, kind },
  }) as const

describe('SignInSheet: the certificate step', () => {
  it('pins exactly the certificate on screen, then dials again', async () => {
    const { approveWebdavCertificate } = await import('#lib/tauri-commands/index.js')
    const approve = vi.mocked(approveWebdavCertificate)
    approve.mockClear()
    const attempt = vi.fn((): Promise<SignInAttemptOutcome> =>
      Promise.resolve({ kind: 'connected', volumeId: 'webdav-nas' }),
    )

    await renderSheet({
      mode: 'sign-in',
      remembered: true,
      endpoint,
      shape: { kind: 'password' },
      trust: certificate('AB:CD:EF', 'unknown'),
      attempt,
    })
    expect(document.body.textContent).toContain('macOS doesn’t know the certificate for nas.local')
    expect(document.body.textContent).toContain('AB:CD:EF')

    buttonSaying('Trust and connect').click()
    await flush()

    expect(approve).toHaveBeenCalledWith({ host: 'nas.local', port: 5006, fingerprint: 'AB:CD:EF', kind: 'unknown' })
    expect(attempt).toHaveBeenCalledTimes(1)
  })

  it('opens on the trust step again when the dial after the pin meets a different certificate', async () => {
    const attempt = (): Promise<SignInAttemptOutcome> =>
      Promise.resolve({
        kind: 'needs_certificate',
        prompt: { host: 'nas.local', port: 5006, fingerprint: '12:34:56', kind: 'changed' },
      })

    await renderSheet({
      mode: 'sign-in',
      remembered: true,
      endpoint,
      shape: { kind: 'password' },
      trust: certificate('AB:CD:EF', 'unknown'),
      attempt,
    })
    buttonSaying('Trust and connect').click()
    await flush()

    // ❗ A changed certificate never wears first contact's plain primary button:
    // its trust button sits behind the disclosure.
    expect(document.body.textContent).toContain('The certificate for nas.local changed')
    expect(document.body.textContent).toContain('12:34:56')
    expect(document.body.textContent).not.toContain('Trust and connect')
    expect(document.body.textContent).toContain('I’ve checked it')
  })

  it('never calls a certificate a key', async () => {
    await renderSheet({
      mode: 'sign-in',
      remembered: false,
      endpoint,
      shape: { kind: 'password' },
      trust: certificate('AB:CD:EF', 'changed'),
      attempt: () => Promise.resolve({ kind: 'cancelled' }),
    })
    expect(document.body.textContent).toContain('Certificate fingerprint (SHA-256)')
    expect(document.body.textContent).toContain('Trust the new certificate')
    expect(document.body.textContent).not.toMatch(/\bkey\b/i)
  })
})
