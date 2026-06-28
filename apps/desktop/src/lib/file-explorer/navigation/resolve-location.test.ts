/**
 * The `resolveLocation` edge helper maps the `resolve_location` IPC result onto
 * a typed outcome: a resolved volume → `{ ok: true }`, no volume → `{ ok: false,
 * reason: 'no-volume' }`, and a timeout → `{ ok: false, reason: 'timed-out' }`.
 * A timeout takes precedence over a null location.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { resolveLocation as resolveLocationCommand } from '$lib/tauri-commands'
import { resolveLocation } from './resolve-location'

vi.mock('$lib/tauri-commands', () => ({
  resolveLocation: vi.fn(),
}))

const mockCommand = vi.mocked(resolveLocationCommand)

describe('resolveLocation', () => {
  beforeEach(() => {
    mockCommand.mockReset()
  })

  it('maps a resolved volume to ok with the location', async () => {
    mockCommand.mockResolvedValue({
      location: { volumeId: 'root', path: '/Users/dave/dir' },
      timedOut: false,
    })

    const outcome = await resolveLocation('/Users/dave/dir')

    expect(outcome).toEqual({ ok: true, location: { volumeId: 'root', path: '/Users/dave/dir' } })
    expect(mockCommand).toHaveBeenCalledWith('/Users/dave/dir')
  })

  it('maps a null location to not-ok with reason no-volume', async () => {
    mockCommand.mockResolvedValue({ location: null, timedOut: false })

    const outcome = await resolveLocation('/gone')

    expect(outcome).toEqual({ ok: false, reason: 'no-volume' })
  })

  it('maps a timeout to not-ok with reason timed-out, even with a null location', async () => {
    mockCommand.mockResolvedValue({ location: null, timedOut: true })

    const outcome = await resolveLocation('/slow/mount')

    expect(outcome).toEqual({ ok: false, reason: 'timed-out' })
  })
})
