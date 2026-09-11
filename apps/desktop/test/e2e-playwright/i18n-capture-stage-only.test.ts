/**
 * Unit tests for the capture's STAGE-ONLY mode (`isStageOnly` in
 * `i18n-capture-config.ts`), which the routine E2E lane runs through
 * `i18n-capture-staging.spec.ts`.
 *
 * What's worth pinning is what the mode must NEVER do, because each one is a real
 * cost on a lane that shares a machine with David's work and a fixture tree with
 * every other spec: turn the key sink on, grab the front position, or write a
 * screenshot. And the half that makes the mode worth running: a surface whose
 * staging no longer reaches its ready state still fails, under its own name.
 *
 * Browserless: a stub page records every script and wait the engines send it.
 */

import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { setStageOnly } from './i18n-capture-config.js'
import {
  type SurfaceEntry,
  captureCall,
  captureSurface,
  captureToastSurface,
  focusWindow,
  shoot,
} from './i18n-capture-helpers.js'
import { captureErrorPaneExample } from './i18n-capture-surfaces-main.js'

interface PageCalls {
  evaluated: string[]
  waited: string[]
  screenshots: string[]
}

/** A page that renders exactly `present`, and records everything asked of it. */
function stubPage(present: string[]): { page: TauriPage; calls: PageCalls } {
  const calls: PageCalls = { evaluated: [], waited: [], screenshots: [] }
  const page = {
    evaluate: (script: string) => {
      calls.evaluated.push(script)
      return Promise.resolve(null)
    },
    waitForSelector: (selector: string) => {
      calls.waited.push(selector)
      return present.includes(selector) ? Promise.resolve() : Promise.reject(new Error(`timed out on ${selector}`))
    },
    screenshot: ({ path }: { path: string }) => {
      calls.screenshots.push(path)
      return Promise.resolve()
    },
  }
  return { page: page as unknown as TauriPage, calls }
}

/** Every script that would turn the sink on or take the front position. */
function forbiddenScripts(calls: PageCalls): string[] {
  return calls.evaluated.filter((s) => s.includes('api.enable(') || s.includes('plugin:window|set_focus'))
}

describe('stage-only capture mode', () => {
  let report: Record<string, SurfaceEntry>
  let failed: string[]

  beforeEach(() => {
    setStageOnly(true)
    report = {}
    failed = []
  })

  afterEach(() => {
    setStageOnly(false)
  })

  it('stages a surface and proves it ready, without recording keys, grabbing focus, or shooting', async () => {
    const { page, calls } = stubPage(['.settings-window'])
    let staged = false

    await captureSurface('settings-appearance', report, failed, async () => {
      // What a real staging closure does: sink calls and a focus request alongside
      // the actual staging. The mode has to neutralize the first two on its own.
      await captureCall(page, 'reset')
      await captureCall<boolean>(page, 'enable')
      await focusWindow(page, 'settings')
      staged = true
      return { page, focusLabel: 'settings', readySelector: '.settings-window', fitSelector: '.settings-window' }
    })

    expect(staged).toBe(true)
    expect(failed).toEqual([])
    expect('settings-appearance' in report).toBe(true)
    expect(calls.waited).toContain('.settings-window')
    expect(calls.screenshots).toEqual([])
    expect(forbiddenScripts(calls)).toEqual([])
    // No `setSurface` / `rerender`: those are the photograph's steps, run after staging.
    expect(calls.evaluated.filter((s) => s.includes('api.setSurface(') || s.includes('api.rerender('))).toEqual([])
  })

  it('fails a surface whose ready selector never appears, under its own name', async () => {
    const { page } = stubPage([])

    await captureSurface('servers-hub', report, failed, async () => {
      await Promise.resolve()
      return { page, readySelector: '.servers-hub .add-row' }
    })

    expect(failed).toEqual(['servers-hub'])
    expect('servers-hub' in report).toBe(false)
  })

  it('stages a toast by waiting for it, and never shoots it', async () => {
    const { page, calls } = stubPage(['.toast'])
    let triggered = false

    await captureToastSurface('toast-favorite', report, failed, page, async () => {
      await Promise.resolve()
      triggered = true
    })

    expect(triggered).toBe(true)
    expect(failed).toEqual([])
    expect('toast-favorite' in report).toBe(true)
    expect(calls.screenshots).toEqual([])
    expect(forbiddenScripts(calls)).toEqual([])
  })

  it('stages the error pane without shooting it', async () => {
    const previousRoot = process.env.CMDR_E2E_START_PATH
    process.env.CMDR_E2E_START_PATH = '/tmp/cmdr-e2e-fixtures-stage-only-test'
    try {
      const { page, calls } = stubPage(['.error-pane', '.file-entry'])

      await captureErrorPaneExample('error-message-example', report, failed, page)

      expect(failed).toEqual([])
      expect('error-message-example' in report).toBe(true)
      expect(calls.screenshots).toEqual([])
      expect(forbiddenScripts(calls)).toEqual([])
    } finally {
      if (previousRoot === undefined) delete process.env.CMDR_E2E_START_PATH
      else process.env.CMDR_E2E_START_PATH = previousRoot
    }
  })

  it('refuses the shutter outright, so no caller can write an image by going around the engines', async () => {
    const { page, calls } = stubPage([])

    await expect(shoot(page, 'main', 'about.png')).rejects.toThrow(/stage-only/)
    expect(calls.screenshots).toEqual([])
  })

  it('still sends every sink call in a photographing run', async () => {
    setStageOnly(false)
    const { page, calls } = stubPage([])

    await captureCall<boolean>(page, 'enable')
    await focusWindow(page, 'main')

    expect(forbiddenScripts(calls)).toHaveLength(2)
  })
})
