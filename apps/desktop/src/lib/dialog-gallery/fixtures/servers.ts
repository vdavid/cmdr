/**
 * The sign-in sheet's three modes and its two host-key faces.
 *
 * ❗ The `attempt` here is a REAL prop, not a preview branch: the sheet really
 * calls it and really renders what it answers, which is exactly how the refusal
 * and the key step get design-reviewed. What it does not do is open a socket.
 */

import type { SignInAttemptOutcome, SignInSheetRequest } from '$lib/servers/sign-in-contract'

export interface SignInSheetFixture {
  request: SignInSheetRequest
}

/** An attempt that answers `outcome` after a beat, so the spinner is visible. */
function answering(outcome: SignInAttemptOutcome) {
  return async (): Promise<SignInAttemptOutcome> => {
    await new Promise((resolve) => setTimeout(resolve, 400))
    return outcome
  }
}

const endpoint = {
  protocol: 'sftp' as const,
  displayName: 'Naspolya',
  address: 'ada@nas.local:22',
  host: 'nas.local',
  username: 'ada',
}

export const serverSignInFixtures: Record<string, SignInSheetFixture | undefined> = {
  add: {
    request: { mode: 'add', attempt: answering({ kind: 'refused', refusal: 'unreachable' }) },
  },
  'add-prefilled': {
    request: {
      mode: 'add',
      prefill: 'https://cloud.example.com',
      attempt: answering({ kind: 'refused', refusal: 'not_a_webdav_server' }),
    },
  },
  'sign-in': {
    request: {
      mode: 'sign-in',
      remembered: false,
      endpoint,
      shape: { kind: 'password' },
      attempt: answering({ kind: 'refused', refusal: 'authentication_rejected' }),
    },
  },
  'sign-in-guest': {
    request: {
      mode: 'sign-in',
      remembered: true,
      endpoint: { ...endpoint, protocol: 'smb', displayName: 'media', address: 'naspolya/media' },
      shape: { kind: 'username_password', guestAllowed: true },
      attempt: answering({ kind: 'refused', refusal: 'authentication_rejected' }),
    },
  },
  'host-key-first-contact': {
    request: {
      mode: 'sign-in',
      remembered: false,
      endpoint,
      shape: { kind: 'password' },
      hostKey: {
        host: 'nas.local',
        port: 22,
        algorithm: 'ssh-ed25519',
        fingerprint: 'SHA256:2Bp0aJ8h5rXKk1vN7qTt3fYw9cLmQzE4sVuG6dRhPxA',
        kind: 'unknown',
      },
      attempt: answering({ kind: 'connected', volumeId: 'sftp-gallery-fixture' }),
    },
  },
  'host-key-changed': {
    request: {
      mode: 'sign-in',
      remembered: false,
      endpoint,
      shape: { kind: 'password' },
      hostKey: {
        host: 'nas.local',
        port: 22,
        algorithm: 'ssh-ed25519',
        fingerprint: 'SHA256:9Xq4mE1cRt7vBn2sKw5jHy8dZpLu3aFgQoI6tNbVxYc',
        kind: 'changed',
      },
      attempt: answering({ kind: 'connected', volumeId: 'sftp-gallery-fixture' }),
    },
  },
  edit: {
    request: {
      mode: 'edit',
      server: {
        id: 'sftp-gallery-fixture',
        protocol: 'sftp',
        displayName: 'Naspolya',
        address: 'nas.local:22',
        username: 'ada',
        pinned: true,
        lastConnectedAt: '2026-09-06T09:12:00Z',
        places: [],
      },
    },
  },
}
