/**
 * The sheet with S3 selected: no address, a provider preset that makes the endpoint,
 * the access key pair in the account fields, and refusals under the field that fixes
 * them. Plus the sign-in sheet's `access_keys` renderer.
 */

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import SignInSheet from './SignInSheet.svelte'
import type { SignInAttemptOutcome, SignInSheetRequest, SignInSubmission } from './sign-in-contract'

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  listSavedServers: vi.fn(() => Promise.resolve([])),
  knownS3PlaceOf: (id: string) => knownS3PlaceOf(id),
  hasServerSecret: vi.fn(() => Promise.resolve(true)),
  getS3UnattendedReconnect: vi.fn(() => Promise.resolve('possible')),
  updateSavedServer: (target: unknown) => updateSavedServer(target),
  updateSavedS3Account: (id: string, name: string) => updateSavedS3Account(id, name),
  saveS3Credentials: (...args: unknown[]) => saveS3Credentials(...args),
}))

const { knownS3PlaceOf, updateSavedServer, updateSavedS3Account, saveS3Credentials } = vi.hoisted(() => ({
  knownS3PlaceOf: vi.fn((_id: string) =>
    Promise.resolve({
      provider: { kind: 'wasabi', region: 'eu-central-1' },
      accessKeyId: 'AKIAEXAMPLE',
      bucket: 'photos',
      displayName: '',
      autoReconnect: false,
      pinned: true,
      volumeId: 's3-photos',
    }),
  ),
  updateSavedServer: vi.fn((_target: unknown) => Promise.resolve({ outcome: 'saved' })),
  updateSavedS3Account: vi.fn((_id: string, _name: string) => Promise.resolve(true)),
  saveS3Credentials: vi.fn((..._args: unknown[]) => Promise.resolve()),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(() => Promise.resolve(null)) }))

let submissions: SignInSubmission[] = []
/** What each round answers, in order; the last one repeats. */
let answers: SignInAttemptOutcome[] = []

beforeEach(() => {
  submissions = []
  answers = [{ kind: 'refused', refusal: 'unreachable' }]
})

afterEach(() => {
  document.body.innerHTML = ''
})

const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
  submissions.push(submission)
  return Promise.resolve(answers.length > 1 ? (answers.shift() as SignInAttemptOutcome) : answers[0])
}

async function flush(times = 8) {
  for (let i = 0; i < times; i++) {
    await Promise.resolve()
    await tick()
  }
}

async function open(request: SignInSheetRequest) {
  mount(SignInSheet, {
    target: document.body.appendChild(document.createElement('div')),
    props: { request, onDone: () => {} },
  })
  await flush()
}

const field = (id: string) => document.body.querySelector<HTMLInputElement>(`#${id}`)

function type(id: string, value: string) {
  const input = field(id)
  if (!input) throw new Error(`no field #${id}`)
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

function press(label: string) {
  const button = [...document.body.querySelectorAll('button')].find((b) => b.textContent.trim() === label)
  if (!button) throw new Error(`no button saying ${label}`)
  button.click()
}

/** An S3 add on AWS, with the key and the region typed. */
async function s3OnAws(region = 'eu-west-1') {
  await open({ mode: 'add', attempt })
  press('S3')
  await tick()
  type('server-s3-region', region)
  type('server-username', 'AKIAEXAMPLE')
  type('server-secret', 's3cr3t')
  await tick()
}

function onlyTarget() {
  const last = submissions.at(-1)
  if (last?.mode !== 'add') throw new Error('expected an add submission')
  return last
}

describe('SignInSheet: the S3 form', () => {
  it('asks for a provider, its region, a bucket, and the key pair, and no address', async () => {
    await s3OnAws()
    expect(field('server-address')).toBeNull()
    expect(document.body.querySelector('label[for="server-username"]')?.textContent).toBe('Access key ID')
    expect(document.body.querySelector('label[for="server-secret"]')?.textContent).toBe('Secret access key')
    expect(field('server-secret')?.getAttribute('autocomplete')).toBe('off')
    // An empty bucket opens the whole account, and the field says so.
    expect(document.body.querySelector('#server-s3-bucket-help')?.textContent).toContain('every bucket')
  })

  it('sends the AWS preset, the key, the account root for an empty bucket, and the secret', async () => {
    await s3OnAws()
    press('Add and open')
    await flush()
    expect(onlyTarget()).toEqual({
      mode: 'add',
      target: {
        protocol: 's3',
        displayName: '',
        provider: { kind: 'aws', region: 'eu-west-1' },
        accessKeyId: 'AKIAEXAMPLE',
        bucket: null,
        autoReconnect: true,
      },
      secret: { secret: 's3cr3t', remember: true },
      intent: 'open',
    })
  })

  it('refuses a region no host name can carry under the region, before dialing', async () => {
    await s3OnAws('EU West 1')
    press('Add and open')
    await flush()
    expect(submissions).toEqual([])
    expect(document.body.querySelector('#server-s3-zone-refusal')?.textContent).toContain('lowercase letters')
    expect(document.activeElement).toBe(field('server-s3-region'))
  })

  it('puts a missing bucket under the bucket field', async () => {
    answers = [{ kind: 'refused', refusal: 'bucket_not_found' }]
    await s3OnAws()
    type('server-s3-bucket', 'nope')
    await tick()
    press('Add and open')
    await flush()
    expect(document.body.querySelector('#server-s3-bucket-refusal')?.textContent).toContain(
      's3.eu-west-1.amazonaws.com',
    )
    expect(document.activeElement).toBe(field('server-s3-bucket'))
  })

  it('names the region a bucket lives in, and one press switches to it and tries again', async () => {
    answers = [{ kind: 'refused', refusal: 'region_mismatch', region: 'us-east-2' }, { kind: 'cancelled' }]
    await s3OnAws()
    press('Add and open')
    await flush()
    expect(document.body.querySelector('#server-s3-zone-refusal')?.textContent).toContain('us-east-2')

    press('Use us-east-2')
    await flush()
    expect(submissions).toHaveLength(2)
    expect(onlyTarget().target).toMatchObject({ provider: { kind: 'aws', region: 'us-east-2' } })
  })
})

const account = {
  id: 's3-account-root',
  protocol: 's3' as const,
  displayName: 'AKIAEXAMPLE@s3.eu-central-1.wasabisys.com',
  nameSource: 'fallback' as const,
  address: 's3.eu-central-1.wasabisys.com',
  username: 'AKIAEXAMPLE',
  pinned: false,
  lastConnectedAt: null,
  autoReconnect: true,
  places: [
    {
      volumeId: 's3-photos',
      name: 'photos',
      pinned: true,
      connected: false,
      appRoot: 's3://AKIAEXAMPLE@s3.eu-central-1.wasabisys.com:443/photos',
      username: 'AKIAEXAMPLE',
      autoReconnect: false,
    },
  ],
}

describe('SignInSheet: editing an S3 place', () => {
  beforeEach(() => {
    updateSavedServer.mockClear()
    saveS3Credentials.mockClear()
  })

  it('opens on the PLACE it was raised on, with its identity locked, its own switch, and no name', async () => {
    await open({ mode: 'edit', server: account, placeVolumeId: 's3-photos' })
    expect(knownS3PlaceOf).toHaveBeenCalledWith('s3-photos')
    expect(document.body.textContent).toContain('Edit photos')
    expect(field('server-s3-region')?.value).toBe('eu-central-1')
    expect(field('server-s3-region')?.disabled).toBe(true)
    expect(field('server-s3-bucket')?.value).toBe('photos')
    expect(field('server-username')?.disabled).toBe(true)
    // ❗ A bucket reads as its own name; the account carries the name, renamed on its own row.
    expect(field('server-name')).toBeNull()
    expect(document.body.textContent).toContain('Reconnect automatically')
  })

  it('saves the same place back, sending no name, and writes a typed secret as the account’s', async () => {
    await open({ mode: 'edit', server: account, placeVolumeId: 's3-photos' })
    type('server-secret', 'n3w')
    await tick()
    press('Save')
    await flush()
    // A blank name leaves the account's name alone (`s3_known_places::adopt_typed_name`).
    expect(updateSavedServer).toHaveBeenCalledWith({
      protocol: 's3',
      displayName: '',
      provider: { kind: 'wasabi', region: 'eu-central-1' },
      accessKeyId: 'AKIAEXAMPLE',
      bucket: 'photos',
      autoReconnect: false,
    })
    expect(saveS3Credentials).toHaveBeenCalledWith({ kind: 'wasabi', region: 'eu-central-1' }, 'AKIAEXAMPLE', 'n3w')
  })
})

describe('SignInSheet: editing an S3 account', () => {
  beforeEach(() => {
    updateSavedServer.mockClear()
    updateSavedS3Account.mockClear()
    saveS3Credentials.mockClear()
  })

  it('opens on the account: its name, its key and provider locked, no bucket, no per-place switch', async () => {
    knownS3PlaceOf.mockResolvedValueOnce({
      provider: { kind: 'wasabi', region: 'eu-central-1' },
      accessKeyId: 'AKIAEXAMPLE',
      bucket: 'photos',
      displayName: 'Cloudflare R2 test3',
      autoReconnect: false,
      pinned: true,
      volumeId: 's3-photos',
    })
    await open({ mode: 'edit', server: { ...account, displayName: 'Cloudflare R2 test3', nameSource: 'user' } })

    expect(document.body.textContent).toContain('Edit Cloudflare R2 test3')
    expect(field('server-name')?.value).toBe('Cloudflare R2 test3')
    expect(field('server-name')?.disabled).toBe(false)
    expect(field('server-s3-region')?.disabled).toBe(true)
    expect(field('server-username')?.disabled).toBe(true)
    expect(field('server-s3-bucket')).toBeNull()
    expect(document.body.textContent).not.toContain('Reconnect automatically')
    expect(document.body.textContent).toContain('The provider and the access key ID')
  })

  it('renames the account, writes a typed secret as the account’s, and saves no place', async () => {
    await open({ mode: 'edit', server: account })
    expect(field('server-name')?.placeholder).toBe('Leave empty to use AKIAEXAMPLE@s3.eu-central-1.wasabisys.com')
    type('server-name', 'Studio')
    type('server-secret', 'n3w')
    await tick()
    press('Save')
    await flush()

    expect(updateSavedS3Account).toHaveBeenCalledExactlyOnceWith('s3-account-root', 'Studio')
    expect(updateSavedServer).not.toHaveBeenCalled()
    expect(saveS3Credentials).toHaveBeenCalledWith({ kind: 'wasabi', region: 'eu-central-1' }, 'AKIAEXAMPLE', 'n3w')
  })

  it('stays open and says nothing was saved when the account went away meanwhile', async () => {
    updateSavedS3Account.mockResolvedValueOnce(false)
    await open({ mode: 'edit', server: account })
    press('Save')
    await flush()

    expect(document.body.querySelector('.form-refusal')?.textContent).toBeTruthy()
    expect(saveS3Credentials).not.toHaveBeenCalled()
  })
})

describe('SignInSheet: the access_keys renderer', () => {
  it('shows the access key ID as the read-only account, over one secret access key field', async () => {
    await open({
      mode: 'sign-in',
      remembered: false,
      endpoint: {
        protocol: 's3',
        displayName: 'photos',
        address: 's3.eu-west-1.amazonaws.com/photos',
        host: 's3.eu-west-1.amazonaws.com',
        username: 'AKIAEXAMPLE',
      },
      shape: { kind: 'access_keys' },
      attempt,
    })
    expect(document.body.querySelector('#sign-in-username-label')?.textContent).toBe('Access key ID')
    expect(document.body.querySelector('#sign-in-username')?.textContent).toBe('AKIAEXAMPLE')
    expect(document.body.querySelector('label[for="sign-in-secret"]')?.textContent).toBe('Secret access key')
    expect(field('sign-in-secret')?.getAttribute('autocomplete')).toBe('off')

    answers = [{ kind: 'refused', refusal: 'authentication_rejected' }]
    type('sign-in-secret', 'wrong')
    await tick()
    press('Sign in')
    await flush()
    // ❗ The key ID is the volume's identity, so the round sends no username.
    expect(submissions).toEqual([{ mode: 'sign-in', secret: { secret: 'wrong', remember: false }, username: null }])
    expect(document.body.querySelector('#sign-in-secret-refusal')?.textContent).toBe(
      'That secret access key didn’t work.',
    )
  })
})
