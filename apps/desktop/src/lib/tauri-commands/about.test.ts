/**
 * The card's stream wrapper: the id it mints rides the start and the cancel, events reach the caller as they land, and
 * neither a torn-down start nor a cancel for a run that's already gone throws.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AboutEvent } from '#lib/ipc/bindings.js'
import { startAbout } from './about'

const { invoked, aboutCancel, channels } = vi.hoisted(() => ({
  invoked: [] as { command: string; args: Record<string, unknown> }[],
  aboutCancel: vi.fn<(id: string) => Promise<null>>(),
  channels: [] as { onmessage: ((event: unknown) => void) | null }[],
}))

let invokeResult: Promise<unknown> = Promise.resolve()

vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {
    onmessage: ((event: unknown) => void) | null = null
    constructor() {
      channels.push(this)
    }
  },
  invoke: (command: string, args: Record<string, unknown>) => {
    invoked.push({ command, args })
    return invokeResult
  },
}))
vi.mock('#lib/ipc/bindings.js', () => ({ commands: { aboutCancel } }))

const item = { path: '/Users/test_user_1/Downloads/Tool.dmg', volumeId: 'root', isDirectory: false }

describe('startAbout', () => {
  beforeEach(() => {
    invoked.length = 0
    channels.length = 0
    aboutCancel.mockReset()
    invokeResult = Promise.resolve()
  })

  it('starts the card with an id it minted, which the cancel carries too', async () => {
    const handle = startAbout(item, () => {})
    expect(handle.requestId).toMatch(/^[0-9a-f-]{36}$/)
    expect(invoked).toHaveLength(1)
    expect(invoked[0].command).toBe('about_start')
    expect(invoked[0].args.request).toEqual({ requestId: handle.requestId, ...item })
    expect(invoked[0].args.onEvent).toBe(channels[0])

    aboutCancel.mockResolvedValue(null)
    await handle.cancel()
    expect(aboutCancel).toHaveBeenCalledWith(handle.requestId)
  })

  it('hands each event to the caller as it lands', () => {
    const seen: AboutEvent[] = []
    const handle = startAbout(item, (event) => seen.push(event))
    const settled: AboutEvent = { requestId: handle.requestId, update: { type: 'factsSettled' } }
    channels[0].onmessage?.(settled)
    expect(seen).toEqual([settled])
  })

  it('mints a new id per card, so a reopen never reads the first card’s events', () => {
    const first = startAbout(item, () => {})
    const second = startAbout(item, () => {})
    expect(first.requestId).not.toBe(second.requestId)
  })

  it('shrugs off a start the webview tore down and a cancel for a run already gone', async () => {
    invokeResult = Promise.reject(new Error('webview gone'))
    const handle = startAbout(item, () => {})
    aboutCancel.mockRejectedValue(new Error('no such request'))
    await expect(handle.cancel()).resolves.toBeUndefined()
    // The rejected start was caught: let it settle without an unhandled rejection.
    await Promise.resolve()
  })
})
