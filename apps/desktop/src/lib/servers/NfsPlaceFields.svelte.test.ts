/**
 * The NFS block of the add form, as a person meets it: type a NAS address, see its
 * exports offered, pick one, and find a sensible uid already filled in.
 *
 * ❗ The two asks wait for the typing to settle, so these drive a fake clock, and
 * the asks themselves are the injected seam (`discoveryDeps`), ❌ never the IPC.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick } from 'svelte'
import NfsPlaceFields from './NfsPlaceFields.svelte'
import { emptyNfsFields, withTypedIdentity, type NfsFormFields } from './nfs-form'
import type { GuessedNfsIdentity, NfsExportList } from '#lib/ipc/bindings.js'

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
  document.body.innerHTML = ''
})

interface Rendered {
  onChange: ReturnType<typeof vi.fn>
  listNfsExports: ReturnType<typeof vi.fn>
  guessNfsIdentity: ReturnType<typeof vi.fn>
}

async function render(
  fields: Partial<NfsFormFields>,
  props: Record<string, unknown> = {},
  answers: { list?: NfsExportList; guess?: GuessedNfsIdentity } = {},
): Promise<Rendered> {
  const onChange = vi.fn()
  const listed: NfsExportList = { outcome: 'listed', exports: ['/Public', '/volume1/data'] }
  const guessed: GuessedNfsIdentity = { uid: 1000, gid: 100, otherGids: [], source: 'most_common_owner' }
  const listNfsExports = vi.fn(() => Promise.resolve(answers.list ?? listed))
  const guessNfsIdentity = vi.fn(() => Promise.resolve(answers.guess ?? guessed))
  mount(NfsPlaceFields, {
    target: document.body.appendChild(document.createElement('div')),
    props: {
      fields: { ...emptyNfsFields(), ...fields },
      address: 'testnas.local',
      disabled: false,
      exportEditable: true,
      onChange,
      discoveryDeps: { listNfsExports, guessNfsIdentity },
      ...props,
    },
  })
  await tick()
  return { onChange, listNfsExports, guessNfsIdentity }
}

/** Lets the settle timers fire and their answers land. */
async function settle() {
  await vi.advanceTimersByTimeAsync(1000)
  flushSync()
  await tick()
}

const text = (selector: string) => document.body.querySelector(selector)?.textContent.trim()

describe('NfsPlaceFields', () => {
  it('asks the server for its exports once the address settles, with the port it named', async () => {
    const { listNfsExports } = await render({}, { address: 'nfs://testnas.local:15481' })
    expect(listNfsExports).not.toHaveBeenCalled()
    await settle()
    expect(listNfsExports).toHaveBeenCalledWith({ host: 'testnas.local', port: 15481 })
    expect(text('#server-nfs-export-help')).toBe(
      'Pick one from the list, or type its path. A folder inside an export works too.',
    )
  })

  it('says when the server keeps its exports to itself, and keeps the field typeable', async () => {
    await render({}, {}, { list: { outcome: 'listing_refused' } })
    await settle()
    expect(text('#server-nfs-export-help')).toBe('testnas.local doesn’t list its exports. Type the export’s path.')
    expect(document.body.querySelector<HTMLInputElement>('input[aria-label="Export"]')?.disabled).toBe(false)
  })

  it('says when nothing answered', async () => {
    await render({}, {}, { list: { outcome: 'timed_out' } })
    await settle()
    expect(text('#server-nfs-export-help')).toContain('couldn’t reach testnas.local')
  })

  it('fills the guessed identity once an export is picked, and says where it came from', async () => {
    const { onChange, guessNfsIdentity } = await render({ exportPath: '/Public' })
    await settle()
    expect(guessNfsIdentity).toHaveBeenCalledWith({ host: 'testnas.local', port: null, exportPath: '/Public' })
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ uid: '1000', gid: '100', identitySource: 'most_common_owner' }),
    )
  })

  it('never guesses over ids the person typed', async () => {
    const { guessNfsIdentity } = await render(
      withTypedIdentity({ ...emptyNfsFields(), exportPath: '/Public' }, { uid: '1026' }),
    )
    await settle()
    expect(guessNfsIdentity).not.toHaveBeenCalled()
  })

  it('marks typing into an id as the person’s own', async () => {
    const { onChange } = await render({ uid: '1000', gid: '100', identitySource: 'this_mac' })
    const uid = document.body.querySelector<HTMLInputElement>('#server-nfs-uid')
    if (!uid) throw new Error('no uid field')
    uid.value = '1026'
    uid.dispatchEvent(new Event('input', { bubbles: true }))
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ uid: '1026', identityFromGuess: false, identitySource: null }),
    )
  })

  it('fills the guessed other groups with the rest of the identity', async () => {
    const { onChange } = await render(
      { exportPath: '/Public' },
      {},
      { guess: { uid: 1000, gid: 100, otherGids: [2001, 2002], source: 'most_common_owner' } },
    )
    await settle()
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ otherGids: '2001, 2002' }))
  })

  it('marks typing into the other groups as the person’s own', async () => {
    const { onChange } = await render({ uid: '1000', gid: '100', identitySource: 'this_mac' })
    const others = document.body.querySelector<HTMLInputElement>('#server-nfs-other-gids')
    if (!others) throw new Error('no other groups field')
    others.value = '2001'
    others.dispatchEvent(new Event('input', { bubbles: true }))
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ otherGids: '2001', identityFromGuess: false, identitySource: null }),
    )
  })

  it('says nothing under an empty or readable other groups field', async () => {
    await render({ otherGids: '2001, 2002', identityFromGuess: false })
    expect(document.body.querySelector('#server-nfs-other-gids-problem')).toBeNull()
    expect(document.body.querySelector('#server-nfs-other-gids')?.getAttribute('aria-invalid')).not.toBe('true')
  })

  it('says what’s wrong with other groups that don’t read', async () => {
    await render({ otherGids: '2001, staff', identityFromGuess: false })
    expect(text('#server-nfs-other-gids-problem')).toContain('separated by commas')
    expect(document.body.querySelector('#server-nfs-other-gids')?.getAttribute('aria-invalid')).toBe('true')
  })

  it('says how many other groups NFS carries when there are too many', async () => {
    await render({
      otherGids: Array.from({ length: 17 }, (_, index) => index + 1).join(', '),
      identityFromGuess: false,
    })
    expect(text('#server-nfs-other-gids-problem')).toContain('16')
    expect(text('#server-nfs-other-gids-problem')).toContain('17')
  })

  it('warns about user ID 0 without blocking anything', async () => {
    await render({ uid: '0', gid: '0', identityFromGuess: false })
    expect(document.body.textContent).toContain('Most servers give user ID 0 no rights.')
  })

  it('puts a refusal under the export, and asks nothing in edit mode', async () => {
    const { listNfsExports } = await render(
      { exportPath: '/Public', identityFromGuess: false },
      { exportEditable: false, exportRefusal: 'testnas.local has no export at that path.' },
    )
    await settle()
    expect(text('#server-nfs-export-refusal')).toBe('testnas.local has no export at that path.')
    expect(listNfsExports).not.toHaveBeenCalled()
  })
})
