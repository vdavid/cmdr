/**
 * Tier 3 a11y tests for the sign-in sheet and the three renderers it composes.
 *
 * One file for the whole directory: `svelte-tests` charges per test FILE, not
 * per test (`docs/testing.md` § "What a test actually costs"), and mounting the
 * sheet in each of its states exercises `ServerFormFields`,
 * `SignInCredentialFields`, and `HostKeyStep` through the shipping component
 * rather than in isolation, which is the only way the labels, the `aria-invalid`
 * pairings, and the `role="alert"` refusals get checked where they really live.
 */

import { describe, it, vi, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import SignInSheet from './SignInSheet.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { SignInAttemptOutcome, SignInSheetRequest } from './sign-in-contract'

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  hasServerSecret: vi.fn(() => Promise.resolve(false)),
  getKnownSftpServers: vi.fn(() => Promise.resolve([])),
  getKnownWebdavServers: vi.fn(() => Promise.resolve([])),
  getSftpUnattendedReconnect: vi.fn(() => Promise.resolve('ready')),
  getWebdavUnattendedReconnect: vi.fn(() => Promise.resolve('possible')),
  forgetServerSecret: vi.fn(() => Promise.resolve(true)),
  updateSavedServer: vi.fn(() => Promise.resolve()),
  approveSftpHostKey: vi.fn(() => Promise.resolve({ outcome: 'recorded' })),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(() => Promise.resolve(null)) }))

// The sheet portals into `document.body`, and axe resolves ARIA ids
// document-wide, so a leftover dialog would poison the next block.
afterEach(() => {
  document.body.innerHTML = ''
})

const endpoint = {
  protocol: 'sftp' as const,
  displayName: 'Naspolya',
  address: 'ada@nas.local:22',
  host: 'nas.local',
  username: 'ada',
}

const refusing = (): Promise<SignInAttemptOutcome> =>
  Promise.resolve({ kind: 'refused', refusal: 'authentication_rejected' })

/** Mounts the sheet and lets its `onMount` seeding settle. */
async function renderSheet(request: SignInSheetRequest): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SignInSheet, { target, props: { request, onDone: () => {} } })
  for (let i = 0; i < 8; i++) {
    await Promise.resolve()
    await tick()
  }
  return target
}

describe('SignInSheet a11y', () => {
  it('add mode has no violations', async () => {
    await renderSheet({ mode: 'add', attempt: refusing })
    await expectNoA11yViolations()
  })

  it('sign-in mode with a read-only account has no violations', async () => {
    await renderSheet({
      mode: 'sign-in',
      volumeId: 'sftp-nas',
      endpoint,
      shape: { kind: 'password' },
      attempt: refusing,
    })
    await expectNoA11yViolations()
  })

  it('sign-in mode with an editable account and a guest choice has no violations', async () => {
    await renderSheet({
      mode: 'sign-in',
      volumeId: 'smb-nas',
      endpoint: { ...endpoint, protocol: 'smb' },
      shape: { kind: 'username_password', guestAllowed: true },
      attempt: refusing,
    })
    await expectNoA11yViolations()
  })

  it('the changed-key step has no violations', async () => {
    await renderSheet({
      mode: 'sign-in',
      volumeId: 'sftp-nas',
      endpoint,
      shape: { kind: 'password' },
      hostKey: {
        host: 'nas.local',
        port: 22,
        algorithm: 'ssh-ed25519',
        fingerprint: 'SHA256:9Xq4mE1cRt7vBn2sKw5jHy8dZpLu3aFgQoI6tNbVxYc',
        kind: 'changed',
      },
      attempt: refusing,
    })
    await expectNoA11yViolations()
  })

  it('edit mode has no violations', async () => {
    await renderSheet({
      mode: 'edit',
      server: {
        id: 'sftp-nas',
        protocol: 'sftp',
        displayName: 'Naspolya',
        address: 'nas.local:22',
        username: 'ada',
        pinned: true,
        lastConnectedAt: null,
        places: [],
      },
    })
    await expectNoA11yViolations()
  })
})
