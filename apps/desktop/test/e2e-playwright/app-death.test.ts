/**
 * Unit tests for the dead-app circuit breaker.
 *
 * The behaviour worth pinning is the cost, not the wording: once the shared Tauri instance
 * stops answering, every later test has to reach a verdict in about no time and without
 * waiting on anything, because the alternative is what it replaced — 223 tests each burning
 * the full 15 s test timeout twice over, two hours of CI for one wedge.
 *
 * These run browserless. The webview probe gets a stub page; the socket ping gets a REAL
 * Unix socket server, because the case that matters is a server that accepts the connection
 * and then never writes a line, and only a real socket reproduces that.
 */

import fs from 'fs'
import net from 'net'
import os from 'os'
import path from 'path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  assertAppAlive,
  checkAppSurvived,
  clearAppDeathMarker,
  failIfAppIsKnownDead,
  findAppDeath,
  pingAppSocket,
  probeAppAlive,
  readAppDeath,
} from './app-death.js'

let socketDir: string
let socketFile: string
let servers: net.Server[]
let accepted: net.Socket[]

beforeEach(() => {
  socketDir = fs.mkdtempSync(path.join(os.tmpdir(), 'cmdr-app-death-'))
  socketFile = path.join(socketDir, 'tauri-playwright.sock')
  process.env.CMDR_PLAYWRIGHT_SOCKET = socketFile
  servers = []
  accepted = []
})

afterEach(async () => {
  // Destroy the accepted sockets first: `close()` only stops NEW connections and then waits
  // out the open ones, and the whole point of the non-answering server is a connection
  // nobody ends. Without this the hook waits until vitest kills it.
  accepted.forEach((socket) => socket.destroy())
  await Promise.all(
    servers.map(
      (s) =>
        new Promise<void>((resolve) =>
          s.close(() => {
            resolve()
          }),
        ),
    ),
  )
  delete process.env.CMDR_PLAYWRIGHT_SOCKET
  fs.rmSync(socketDir, { recursive: true, force: true })
})

/**
 * Stand up a socket at the configured path. `answer` false models the wedge: the connection
 * is accepted, the ping arrives, and nothing is ever written back.
 */
function listen(answer: boolean): Promise<void> {
  const server = net.createServer((socket) => {
    accepted.push(socket)
    if (answer) socket.on('data', () => socket.write('{"ok":true}\n'))
  })
  servers.push(server)
  return new Promise((resolve) => server.listen(socketFile, resolve))
}

/** A page that answers every `evaluate`, counting the calls. */
function livePage(): { evaluate: (js: string) => Promise<unknown>; calls: () => number } {
  let calls = 0
  return {
    evaluate: (): Promise<unknown> => {
      calls += 1
      return Promise.resolve(true)
    },
    calls: () => calls,
  }
}

/** A page whose `evaluate` never settles, which is what a wedged WEBVIEW looks like. */
function hungPage(): { evaluate: (js: string) => Promise<unknown>; calls: () => number } {
  let calls = 0
  return {
    evaluate: (): Promise<unknown> => {
      calls += 1
      return new Promise(() => {})
    },
    calls: () => calls,
  }
}

/** A page whose `evaluate` rejects, which is what a CLOSED socket looks like. */
function deadSocketPage(): { evaluate: (js: string) => Promise<unknown>; calls: () => number } {
  let calls = 0
  return {
    evaluate: (): Promise<unknown> => {
      calls += 1
      return Promise.reject(new Error('socket hang up'))
    },
    calls: () => calls,
  }
}

describe('pingAppSocket', () => {
  it('returns null when the plugin answers', async () => {
    await listen(true)
    expect(await pingAppSocket(2000)).toBeNull()
  })

  it('gives up on a socket that accepts and never answers', async () => {
    // The exact wedge shape: `tauriPage`'s own setup does this ping with NO deadline and
    // hangs here for the whole test timeout, which is why the breaker pings for itself.
    await listen(false)
    const started = Date.now()
    expect(await pingAppSocket(60)).toContain('60')
    expect(Date.now() - started).toBeLessThan(2000)
  })

  it('reports a socket nobody is listening on', async () => {
    expect(await pingAppSocket(2000)).toContain('rejected')
  })
})

describe('probeAppAlive', () => {
  it('returns null while the webview answers', async () => {
    expect(await probeAppAlive(livePage(), 1000)).toBeNull()
  })

  it('gives up on a hung webview inside the deadline', async () => {
    const started = Date.now()
    expect(await probeAppAlive(hungPage(), 50)).toContain('50')
    expect(Date.now() - started).toBeLessThan(1000)
  })

  it('reports a rejected probe as death too', async () => {
    expect(await probeAppAlive(deadSocketPage(), 1000)).toContain('socket hang up')
  })
})

describe('findAppDeath', () => {
  it('lets a test through while the app answers, and leaves no marker', async () => {
    await listen(true)
    expect(await findAppDeath('Some suite › some test', 2000)).toBeNull()
    expect(readAppDeath()).toBeNull()
  })

  it('records where the app died the first time the ping goes unanswered', async () => {
    await listen(false)
    expect(await findAppDeath('Archive browsing › extracts it', 60)).toContain('STOPPED ANSWERING')
    expect(readAppDeath()?.where).toContain('Archive browsing › extracts it')
  })

  it('answers every later test WITHOUT waiting on anything, naming the test that met the wedge', async () => {
    await listen(false)
    expect(await findAppDeath('Archive browsing › extracts it', 60)).not.toBeNull()

    // The cascade path: no ping, no socket, no deadline. This is the whole point — 223 of
    // these used to cost 15 s each (twice, with the CI retry) and now cost nothing.
    const started = Date.now()
    expect(await findAppDeath('Type-to-jump › ESC clears the buffer', 60)).toContain('Archive browsing › extracts it')
    expect(Date.now() - started).toBeLessThan(50)
  })

  it('forgets a previous run, so a stale marker cannot poison a fresh one', async () => {
    await listen(false)
    expect(await findAppDeath('Archive browsing › extracts it', 60)).not.toBeNull()

    clearAppDeathMarker()
    expect(readAppDeath()).toBeNull()
    expect(() => {
      failIfAppIsKnownDead()
    }).not.toThrow()
  })
})

describe('the webview half', () => {
  it('lets a test through while the webview answers', async () => {
    await assertAppAlive(livePage(), 'Some suite › some test', 1000)
    expect(readAppDeath()).toBeNull()
  })

  it('catches a webview wedge the socket ping would miss, and fails later tests for free', async () => {
    // A live plugin with a dead webview: the ping would come back fine, so this probe is
    // the only thing standing between one wedge and a whole shard of 15 s timeouts.
    await expect(assertAppAlive(hungPage(), 'Archive browsing › extracts it', 50)).rejects.toThrow(/STOPPED ANSWERING/)

    const later = hungPage()
    await expect(assertAppAlive(later, 'Ask Cmdr › gates on consent', 50)).rejects.toThrow(
      /Archive browsing › extracts it/,
    )
    expect(later.calls()).toBe(0)
  })

  it('blames the killer on the way out, so the next test gets a verdict it did not have to earn', async () => {
    const killer = 'Archive browsing › opens the viewer'
    expect(await checkAppSurvived(hungPage(), killer, 50)).toContain('THIS is the one to debug')
    expect(() => {
      failIfAppIsKnownDead()
    }).toThrow(new RegExp(`during "${killer}"`))
  })

  it('stays quiet in teardown when the app is fine, and when the death is already recorded', async () => {
    expect(await checkAppSurvived(livePage(), 'Some suite › some test', 1000)).toBeNull()

    await expect(assertAppAlive(hungPage(), 'Archive browsing › extracts it', 50)).rejects.toThrow()
    // The gate already threw for this test; a second report of the same wedge in its
    // teardown would only bury the one that names where the app actually went.
    expect(await checkAppSurvived(hungPage(), 'Archive browsing › extracts it', 50)).toBeNull()
  })
})
