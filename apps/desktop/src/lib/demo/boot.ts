/**
 * The browser demo's stand-in for the Tauri backend: imported first by `hooks.client.ts`, and in
 * demo builds only (`__CMDR_DEMO_BUILD__`) it installs itself before any app module runs. Every
 * `invoke` the app makes lands in `answer()`; events the "backend" sends go through
 * `@tauri-apps/api/mocks`' event emulation. Everything here sits behind the guard, so a normal
 * build tree-shakes this module and all it imports.
 */
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import type { AnswerTable, Payload } from './answer-types.js'
import { appAnswers, rawAnswers, silentCommands } from './answers.js'
import { listingAnswers, listingSideAnswers, startListingEngine } from './listing.js'
import { pluginHandler } from './plugins.js'
import { exposeUnanswered, noteUnanswered } from './unanswered.js'
import { volumeAnswers } from './volumes.js'

type Handler = (payload: Payload) => unknown

function camelCase(snake: string): string {
  return snake.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase())
}

function installDemoBackend(): void {
  const silent = Object.fromEntries(silentCommands.map((name) => [name, () => undefined]))
  const answers: AnswerTable = {
    ...silent,
    ...appAnswers,
    ...volumeAnswers,
    ...listingSideAnswers,
    ...listingAnswers(),
  }
  const byName = answers as Record<string, Handler | undefined>

  function answer(cmd: string, payload: Payload): unknown {
    const handler = pluginHandler(cmd) ?? rawAnswers[cmd] ?? byName[camelCase(cmd)]
    return handler ? handler(payload) : noteUnanswered(cmd, payload)
  }

  exposeUnanswered()
  mockWindows('main')
  mockIPC((cmd, payload) => answer(cmd, (payload ?? {}) as Payload), { shouldMockEvents: true })
  startListingEngine()
}

if (__CMDR_DEMO_BUILD__) installDemoBackend()
