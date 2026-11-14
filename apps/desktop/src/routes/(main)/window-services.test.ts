/**
 * The window's subscriptions must not outlive it. `+page.svelte`'s `onMount` is async, so the
 * page can unmount while `startWindowServices` is still parked on an await (a route change mid-
 * startup); whatever it registered after that point used to stay subscribed forever, and a
 * remount then stacked a second copy of every listener, so one `execute-command` ran its command
 * twice.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { listen } from '@tauri-apps/api/event'
import { createMountScope } from '#lib/utils/mount-scope.js'
import { startEarlyWindowServices, startWindowServices, type WindowServicesContext } from './window-services'

/** Every Tauri listener currently subscribed, by registration order. */
const live = new Set<number>()

function trackListeners(): void {
  let next = 0
  vi.mocked(listen).mockImplementation(async () => {
    // A real `listen` is an IPC round trip; yielding here is what lets teardown land mid-start.
    await new Promise((resolve) => setTimeout(resolve, 0))
    const id = next++
    live.add(id)
    return () => {
      live.delete(id)
    }
  })
}

function context(): WindowServicesContext {
  const dispatch = vi.fn(() => Promise.resolve())
  return {
    getExplorer: () => undefined,
    dispatchers: {
      keyboard: dispatch,
      menu: dispatch,
      contextMenu: dispatch,
      mouse: dispatch,
      palette: dispatch,
      explorer: dispatch,
      mcp: dispatch,
    },
    dialogs: { setAboutWindow: vi.fn() },
    maybeRunWhatsNew: vi.fn(() => Promise.resolve()),
    getDialogsOnScreen: () => ({ dialogOpen: false, paletteOpen: false }),
  }
}

describe('startWindowServices', () => {
  beforeEach(() => {
    live.clear()
    trackListeners()
  })

  it('leaves no listener behind when torn down while still starting', async () => {
    const scope = createMountScope()
    startEarlyWindowServices(scope)
    const starting = startWindowServices(scope, context())
    // Let the first few registrations land, then unmount mid-startup.
    await new Promise((resolve) => setTimeout(resolve, 5))
    scope.dispose()
    await starting
    // The early phase's fire-and-forget starts settle on their own clock.
    await new Promise((resolve) => setTimeout(resolve, 20))

    expect(live.size).toBe(0)
  })

  it('tears down every listener it registered on a normal stop', async () => {
    const scope = createMountScope()
    startEarlyWindowServices(scope)
    await startWindowServices(scope, context())
    await new Promise((resolve) => setTimeout(resolve, 20))
    expect(live.size).toBeGreaterThan(0)

    scope.dispose()

    expect(live.size).toBe(0)
  })
})
