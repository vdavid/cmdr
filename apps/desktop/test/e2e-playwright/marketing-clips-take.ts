/**
 * One recorded take of a marketing clip: starts the ScreenCaptureKit recorder on a
 * window, collects the spec's markers, and writes the master plus its markers file for
 * the encoder (`apps/desktop/scripts/marketing-clips-encode.ts`).
 *
 * The recorder binary is `scripts/marketing-clips-recorder.swift`, compiled by the
 * orchestrator and handed over as `CMDR_CLIPS_RECORDER`; masters land in
 * `CMDR_CLIPS_MASTERS_DIR`. Its stdout protocol is one JSON object per line, documented
 * at the top of the Swift source.
 */

import { waitBudget } from './wait-budget.js'
import { spawn } from 'node:child_process'
import type { ChildProcess } from 'node:child_process'
import { renameSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { createInterface } from 'node:readline'

export type ClipTheme = 'dark' | 'light'

function requireEnv(name: string): string {
  const value = process.env[name]
  if (value === undefined || value === '') {
    throw new Error(`${name} is unset. Run this shard through \`pnpm marketing:clips\`, not bare Playwright.`)
  }
  return value
}

/** What the recorder says, one line at a time. */
type RecorderEvent =
  | { event: 'recording'; firstFrameEpochMs: number; width: number; height: number }
  | { event: 'done'; frames: number; durationS: number }
  | { event: 'error'; message: string }

/** A take in progress. Mark its moments, then `finish` it. */
export interface Take {
  /** Marks where the clip starts: call right before the first action a viewer should see. */
  markStart: () => void
  /** Marks the frame the poster is cut from: a moment that explains the clip on its own. */
  markPoster: () => void
  /** Marks where the usable footage ends, when slack follows it; `finish` marks it otherwise. */
  markEnd: () => void
  /** Notes a key moment by name (the long-form video beats list them for the editor). */
  note: (label: string) => void
  /**
   * Captions a key press (`⌘F`, `↓`, a typed word) at this moment, for the website's
   * KeyCastr-style overlay. Scenes reach it through `captionKey`, which finds the take
   * that is rolling.
   */
  key: (label: string) => void
  /** Marks where the clip ends, stops the recorder, and writes the markers file. */
  finish: () => Promise<void>
  /** Stops the recorder and throws the master away. For a take that failed partway. */
  abandon: () => Promise<void>
}

/** The take that is recording now, which `captionKey` writes to; `null` between takes. */
let rolling: Take | null = null

/**
 * Captions a key press on the take that is recording, and does nothing off camera, so a
 * helper can caption every press it makes without knowing whether a take is rolling.
 */
export function captionKey(label: string): void {
  rolling?.key(label)
}

/**
 * Starts recording `windowId` (plus `includeWindowIds`, composited where they overlap it)
 * and resolves once the first frame is in the master, so nothing the spec does next can
 * fall before the recording.
 */
export async function startTake(options: {
  slug: string
  theme: ClipTheme
  windowId: number
  includeWindowIds?: number[]
}): Promise<Take> {
  const { slug, theme, windowId, includeWindowIds = [] } = options
  const mastersDir = requireEnv('CMDR_CLIPS_MASTERS_DIR')
  const base = join(mastersDir, `${slug}-${theme}`)
  // Recorded under a temp name and renamed in by `finish`, so a failed take never
  // replaces the last good master with half a clip.
  const recording = `${base}.recording.mov`
  const args = ['--window-id', String(windowId), '--out', recording]
  for (const id of includeWindowIds) args.push('--include-window-id', String(id))

  const proc = spawn(requireEnv('CMDR_CLIPS_RECORDER'), args, { stdio: ['pipe', 'pipe', 'inherit'] })
  const events = recorderEvents(proc)

  const first = await events.next('recording', waitBudget(15000))
  if (first.event !== 'recording') throw new Error('unreachable: next() returns the event it waited for')
  const firstFrameEpochMs = first.firstFrameEpochMs

  let startEpochMs: number | null = null
  let posterEpochMs: number | null = null
  const notes: { epochMs: number; label: string }[] = []
  const keys: { epochMs: number; label: string }[] = []
  let usableEndEpochMs: number | null = null

  async function stop(): Promise<void> {
    proc.stdin.end('stop\n')
    await events.next('done', waitBudget(30000))
  }

  const take: Take = {
    markStart: () => {
      startEpochMs = Date.now()
    },
    markPoster: () => {
      posterEpochMs = Date.now()
    },
    markEnd: () => {
      usableEndEpochMs = Date.now()
    },
    note: (label) => {
      notes.push({ epochMs: Date.now(), label })
    },
    key: (label) => {
      keys.push({ epochMs: Date.now(), label })
    },
    finish: async () => {
      const endEpochMs = usableEndEpochMs ?? Date.now()
      if (rolling === take) rolling = null
      await stop()
      if (startEpochMs === null) {
        rmSync(recording, { force: true })
        throw new Error(`${slug}-${theme}: the take never marked its start`)
      }
      renameSync(recording, `${base}.mov`)
      const markers = {
        slug,
        theme,
        firstFrameEpochMs,
        startEpochMs,
        endEpochMs,
        // A take with no explicit poster moment uses its midpoint.
        posterEpochMs: posterEpochMs ?? Math.round((startEpochMs + endEpochMs) / 2),
        notes,
        keys,
      }
      writeFileSync(`${base}.markers.json`, `${JSON.stringify(markers, null, 2)}\n`)
    },
    abandon: async () => {
      if (rolling === take) rolling = null
      await stop().catch(() => {
        proc.kill('SIGKILL')
      })
      rmSync(recording, { force: true })
    },
  }
  rolling = take
  return take
}

/** Reads the recorder's JSON lines and hands them out by kind; an `error` line rejects any waiter. */
function recorderEvents(proc: ChildProcess): {
  next: (kind: RecorderEvent['event'], timeoutMs: number) => Promise<RecorderEvent>
} {
  const seen: RecorderEvent[] = []
  const waiters: { kind: string; resolve: (e: RecorderEvent) => void; reject: (e: Error) => void }[] = []
  let failure: Error | null = null

  const fail = (error: Error): void => {
    failure ??= error
    for (const waiter of waiters.splice(0)) waiter.reject(error)
  }

  if (proc.stdout !== null) {
    createInterface({ input: proc.stdout }).on('line', (line) => {
      let parsed: RecorderEvent
      try {
        parsed = JSON.parse(line) as RecorderEvent
      } catch {
        return
      }
      if (parsed.event === 'error') {
        fail(new Error(`the clip recorder: ${parsed.message}`))
        return
      }
      seen.push(parsed)
      for (const waiter of waiters.filter((w) => w.kind === parsed.event)) {
        waiters.splice(waiters.indexOf(waiter), 1)
        waiter.resolve(parsed)
      }
    })
  }
  // `close`, not `exit`: `exit` can fire before the last stdout line is read, which
  // would reject the wait for the `done` the recorder printed on its way out.
  proc.on('close', (code) => {
    fail(new Error(`the clip recorder exited ${String(code)}`))
  })
  proc.on('error', (error) => {
    fail(new Error(`the clip recorder could not start: ${error.message}`))
  })

  return {
    next: (kind, timeoutMs) => {
      const already = seen.find((e) => e.event === kind)
      if (already !== undefined) return Promise.resolve(already)
      if (failure !== null) return Promise.reject(failure)
      return new Promise<RecorderEvent>((resolve, reject) => {
        const timer = setTimeout(() => {
          reject(new Error(`the clip recorder never said '${kind}' within ${String(timeoutMs)} ms`))
        }, timeoutMs)
        waiters.push({
          kind,
          resolve: (e) => {
            clearTimeout(timer)
            resolve(e)
          },
          reject: (e) => {
            clearTimeout(timer)
            reject(e)
          },
        })
      })
    },
  }
}
