/**
 * The sign-in sheet's three modes and its two host-key faces.
 *
 * ❗ The `attempt` here is a REAL prop, not a preview branch: the sheet really
 * calls it and really renders what it answers, which is exactly how the refusal
 * and the key step get design-reviewed. What it does not do is open a socket.
 */

import type { SignInAttemptOutcome, SignInSheetRequest } from '#lib/servers/sign-in-contract.js'

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

/** The same NAS over WebDAV, with the self-signed certificate it ships with. */
const webdavEndpoint = {
  protocol: 'webdav' as const,
  displayName: 'Naspolya',
  address: 'https://nas.local:5006/',
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
  'add-s3': {
    request: {
      mode: 'add',
      prefill: 's3://AKIAIOSFODNN7EXAMPLE@s3.eu-west-1.amazonaws.com:443/photos',
      attempt: answering({ kind: 'refused', refusal: 'region_mismatch', region: 'us-east-2' }),
    },
  },
  'sign-in-s3': {
    request: {
      mode: 'sign-in',
      remembered: false,
      endpoint: {
        protocol: 's3',
        displayName: 'photos',
        address: 's3.eu-west-1.amazonaws.com/photos',
        host: 's3.eu-west-1.amazonaws.com',
        username: 'AKIAIOSFODNN7EXAMPLE',
      },
      shape: { kind: 'access_keys' },
      attempt: answering({ kind: 'refused', refusal: 'access_denied' }),
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
      trust: {
        subject: 'host_key',
        prompt: {
          host: 'nas.local',
          port: 22,
          algorithm: 'ssh-ed25519',
          fingerprint: 'SHA256:2Bp0aJ8h5rXKk1vN7qTt3fYw9cLmQzE4sVuG6dRhPxA',
          kind: 'unknown',
        },
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
      trust: {
        subject: 'host_key',
        prompt: {
          host: 'nas.local',
          port: 22,
          algorithm: 'ssh-ed25519',
          fingerprint: 'SHA256:9Xq4mE1cRt7vBn2sKw5jHy8dZpLu3aFgQoI6tNbVxYc',
          kind: 'changed',
        },
      },
      attempt: answering({ kind: 'connected', volumeId: 'sftp-gallery-fixture' }),
    },
  },
  'certificate-first-contact': {
    request: {
      mode: 'sign-in',
      remembered: false,
      endpoint: webdavEndpoint,
      shape: { kind: 'password' },
      trust: {
        subject: 'certificate',
        prompt: {
          host: 'nas.local',
          port: 5006,
          fingerprint:
            'CD:6F:DC:7C:4B:21:C9:B5:28:58:69:B3:4E:5C:68:B8:55:26:2C:0A:5C:06:7A:8A:2D:BF:BD:FA:AE:C2:9B:E7',
          kind: 'unknown',
        },
      },
      attempt: answering({ kind: 'connected', volumeId: 'webdav-gallery-fixture' }),
    },
  },
  'certificate-changed': {
    request: {
      mode: 'sign-in',
      remembered: false,
      endpoint: webdavEndpoint,
      shape: { kind: 'password' },
      trust: {
        subject: 'certificate',
        prompt: {
          host: 'nas.local',
          port: 5006,
          fingerprint:
            '3A:91:0E:F7:42:B8:6C:D5:19:A4:7E:23:C0:58:9F:B6:E1:04:7D:2A:93:C8:5B:F0:16:4E:A7:D9:38:62:BC:05',
          kind: 'changed',
        },
      },
      attempt: answering({ kind: 'connected', volumeId: 'webdav-gallery-fixture' }),
    },
  },
  edit: {
    request: {
      mode: 'edit',
      server: {
        id: 'sftp-gallery-fixture',
        protocol: 'sftp',
        displayName: 'Naspolya',
        nameSource: 'user',
        address: 'nas.local:22',
        username: 'ada',
        pinned: true,
        lastConnectedAt: '2026-09-06T09:12:00Z',
        autoReconnect: true,
        places: [],
      },
    },
  },
}
