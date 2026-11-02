/**
 * E2E for the MCP operation-queue surface: the `queue` tool, the
 * `operations:` section of `cmdr://state`, and `await operation_complete`.
 *
 * Drives the whole agent loop end to end: start a throttled copy via the MCP
 * `copy` tool and `dialog_confirm` (capturing the returned `operationId`), pause it
 * with `queue`, assert it shows `status: paused` in `cmdr://state operations`,
 * resume it, then `await operation_complete` for the same id.
 *
 * The copy source is a dedicated multi-file directory plus the per-file
 * `set_test_throttle`, for the same two reasons as `operation-queue.spec.ts`: the
 * throttle sleeps once PER FILE (so many files keep the op in flight long enough
 * to observe), and pause gates BETWEEN files (so a one-file copy can't be paused
 * at all).
 *
 * Requires `--features playwright-e2e`.
 */

import { waitBudget } from './wait-budget.js'
import { execFileSync } from 'child_process'
import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import {
  dismissAllToasts,
  ensureAppReady,
  expectAndDismissToast,
  getFixtureRoot,
  waitForTransferUiToSettle,
} from './helpers.js'
import { confirmRollback } from './conflict-helpers.js'
import {
  ensureMcpClient,
  expectAwaitingPerson,
  mcpCall,
  mcpCallRaw,
  mcpConfirmedCopy,
  mcpConfirmedMove,
  mcpNavToPath,
  mcpReadResource,
} from '../e2e-shared/mcp-client.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** Per-file copy throttle; with `FILES_PER_SOURCE` files this keeps the copy in
 *  flight for ~`FILES_PER_SOURCE * THROTTLE_MS` ms, generous room to pause/resume. */
const THROTTLE_MS = 250
const FILES_PER_SOURCE = 24
const SOURCE = 'mcp-queue-src'

test.setTimeout(waitBudget(90_000))

/** Creates `left/<name>/` with `FILES_PER_SOURCE` tiny files (Node-side, real disk). */
function makeSourceDir(fixtureRoot: string, name: string): void {
  const dir = path.join(fixtureRoot, 'left', name)
  fs.mkdirSync(dir, { recursive: true })
  for (let i = 0; i < FILES_PER_SOURCE; i++) {
    fs.writeFileSync(path.join(dir, `file-${String(i).padStart(2, '0')}.txt`), 'x'.repeat(1024))
  }
}

/** Pulls the spawned operationId out of the `copy` tool's OK text
 *  ("... (operationId: op-xyz).") — the operationId correlation this spec proves. */
function parseOperationId(copyResult: string): string {
  const match = /operationId: ([^)]+)\)/.exec(copyResult)
  if (!match) throw new Error(`copy result carried no operationId: ${copyResult}`)
  return match[1]
}

test.beforeEach(async ({ tauriPage }) => {
  const fixtureRoot = getFixtureRoot()
  recreateFixtures(fixtureRoot)
  makeSourceDir(fixtureRoot, SOURCE)
  await ensureAppReady(tauriPage)
  await ensureMcpClient(tauriPage)
  // Slow each per-file copy step so the op stays in flight while we pause it.
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: ${String(THROTTLE_MS)} })`)
})

// ⚠️ ONE hook, drain THEN restore. The tree restore deletes this spec's own
// source dir, so a copy still reading it dies with `SourceNotFound` and RETAINS
// that failure: a leaked toast plus a queue row that outlives the test. Two
// hooks hide the order (Playwright runs same-suite `afterEach`s in declaration
// order). `../e2e-playwright/DETAILS.md` § "The fixture-tree leak guard".
test.afterEach(async ({ tauriPage }) => {
  // Clear the throttle FIRST so any in-flight op winds down fast, cancel
  // everything, then WAIT for the lane to empty (cancel returns on REQUEST, not
  // on wind-down; a still-cancelling op leaves the lane busy for the next test).
  // The dismiss sits inside the loop: an op can die while it's already spinning.
  await tauriPage.evaluate(`(async function() {
    try { await window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null }); } catch (e) {}
    try {
      var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
      var ids = ops.map(function(o) { return o.operationId; });
      if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
    } catch (e) {}
    for (var i = 0; i < 60; i++) {
      try { await window.__TAURI_INTERNALS__.invoke('dismiss_all_failed_operations'); } catch (e) {}
      var remaining = await window.__TAURI_INTERNALS__.invoke('list_operations');
      if (!remaining || remaining.length === 0) break;
      await new Promise(function(r) { setTimeout(r, 100); });
    }
  })()`)

  // Only now, with nothing reading `left/` any more: the post-test leak guard
  // fails whoever leaves the tree dirty, and the restore is surgical, so it only
  // rewrites what actually drifted.
  restoreFixtureTree(getFixtureRoot())
})

test.describe('MCP operation queue', () => {
  test('a confirmed copy returns an operationId; queue pauses/resumes it; await operation_complete settles', async ({
    tauriPage,
  }) => {
    const main = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()

    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))
    await mcpNavToPath('right', path.join(fixtureRoot, 'right'))

    // Select the multi-file source dir in the left pane and copy it to the right.
    await mcpCall('select', { pane: 'left', names: [SOURCE] })

    // The confirm returns the spawned operationId in its OK text.
    const copyResult = await mcpConfirmedCopy('left')
    const operationId = parseOperationId(copyResult)

    // The op is discoverable in `cmdr://state operations` while it runs.
    await expect
      .poll(async () => (await mcpReadResource('cmdr://state?include=operations')).includes(operationId), {
        timeout: waitBudget(15000),
      })
      .toBeTruthy()

    // Pause it via the `queue` tool → its status flips to paused.
    await mcpCall('queue', { action: 'pause', operationId })
    await expect
      .poll(
        async () => {
          const state = await mcpReadResource('cmdr://state?include=operations')
          return operationsStatusFor(state, operationId)
        },
        { timeout: waitBudget(15000) },
      )
      .toBe('paused')

    // Resume it → status flips back to running.
    await mcpCall('queue', { action: 'resume', operationId })
    await expect
      .poll(
        async () => {
          const state = await mcpReadResource('cmdr://state?include=operations')
          return operationsStatusFor(state, operationId)
        },
        { timeout: waitBudget(15000) },
      )
      .toBe('running')

    // Let it finish quickly, then await its completion by id.
    await main.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null })`)
    const settled = await mcpCall('await', {
      condition: 'operation_complete',
      value: operationId,
      timeoutSeconds: 30,
    })
    expect(settled).toContain('settled')
    expect(settled).toContain('completed')

    // The copy fires a completion toast; dismiss it so the global overlay-leak
    // guard stays clean (the wording is the contract for a folder copy).
    await expectAndDismissToast(main, 'Copied')
  })
})

const ROLLBACK_DIALOG = '[data-dialog-id="rollback-confirmation"]'

/** How many rollback questions are on screen. Two would mean a host stacked one on another. */
function rollbackDialogs(main: TauriPage): Promise<number> {
  return main.evaluate<number>(`document.querySelectorAll('${ROLLBACK_DIALOG}').length`)
}

function rollbackBody(main: TauriPage): Promise<string> {
  return main.evaluate<string>(`document.querySelector('#rollback-confirmation-body')?.textContent || ''`)
}

/** What's in the destination copy of `SOURCE`, as files on real disk. */
function copiedFiles(fixtureRoot: string): string[] {
  const dir = path.join(fixtureRoot, 'right', SOURCE)
  return fs.existsSync(dir) ? fs.readdirSync(dir) : []
}

/** Starts a copy of `SOURCE` from the left pane to the right, and returns its `operationId`. */
async function startCopy(fixtureRoot: string, confirm?: Record<string, unknown>): Promise<string> {
  await mcpNavToPath('left', path.join(fixtureRoot, 'left'))
  await mcpNavToPath('right', path.join(fixtureRoot, 'right'))
  await mcpCall('select', { pane: 'left', names: [SOURCE] })
  return parseOperationId(await mcpConfirmedCopy('left', confirm))
}

/** Waits until a copy has written something: before that it has nothing to put back. */
async function untilSomethingIsWritten(fixtureRoot: string): Promise<void> {
  await expect.poll(() => copiedFiles(fixtureRoot).length > 0, { timeout: waitBudget(15000) }).toBeTruthy()
}

/** Until the operation is over AND its progress dialog has left, which comes a moment later. */
async function untilSettled(main: TauriPage, operationId: string): Promise<void> {
  await main.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null })`)
  await mcpCall('await', { condition: 'operation_complete', value: operationId, timeoutSeconds: 30 })
  await waitForTransferUiToSettle(main)
}

async function rollbackStateOf(operationId: string): Promise<string> {
  const detail = JSON.parse(await mcpCall('operations_get', { operationId, limit: 1 })) as {
    operation: { rollbackState: string }
  }
  return detail.operation.rollbackState
}

/** The typed half of a refusal: what a caller branches on. */
async function refusalOf(tool: string, args: Record<string, unknown>): Promise<Record<string, unknown>> {
  const envelope = await mcpCallRaw(tool, args)
  if (envelope.error?.data === undefined) throw new Error(`Expected ${tool} to refuse, got ${JSON.stringify(envelope)}`)
  return envelope.error.data
}

test.describe('MCP rollback through the dialog', () => {
  test('a running copy on show asks through its progress dialog: no answer changes nothing, and a yes undoes it', async ({
    tauriPage,
  }) => {
    const main = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    const operationId = await startCopy(fixtureRoot)
    await untilSomethingIsWritten(fixtureRoot)

    expect(expectAwaitingPerson(await mcpCall('operations_rollback', { operationId }))).toBe('rollback-confirmation')
    expect(await rollbackDialogs(main)).toBe(1)
    expect(await rollbackBody(main)).toContain('written so far')

    // Asking again is the same question, ❌ never a second one on top.
    expectAwaitingPerson(await mcpCall('operations_rollback', { operationId }))
    expect(await rollbackDialogs(main)).toBe(1)

    await mcpCall('dialog', { action: 'close', type: 'rollback-confirmation' })
    expect(await rollbackDialogs(main)).toBe(0)
    expect(operationsStatusFor(await mcpReadResource('cmdr://state?include=operations'), operationId)).toBe('running')
    expect(copiedFiles(fixtureRoot).length).toBeGreaterThan(0)

    expectAwaitingPerson(await mcpCall('operations_rollback', { operationId }))
    const answer = JSON.parse(await mcpCall('dialog_confirm', { type: 'rollback-confirmation' })) as {
      status: string
      inverseOpId: string | null
    }
    expect(answer.status).toBe('dispatched')
    expect(answer.inverseOpId).toBeNull()

    await untilSettled(main, operationId)
    await expect.poll(() => copiedFiles(fixtureRoot).length, { timeout: waitBudget(15000) }).toBe(0)
    await dismissAllToasts(main)
  })

  test('a running copy in the background asks on its own, and a person pressing it undoes it', async ({
    tauriPage,
  }) => {
    const main = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    const operationId = await startCopy(fixtureRoot, { background: true })
    await untilSomethingIsWritten(fixtureRoot)

    expect(expectAwaitingPerson(await mcpCall('operations_rollback', { operationId }))).toBe('rollback-confirmation')
    expect(await rollbackDialogs(main)).toBe(1)
    expect(await rollbackBody(main)).toContain('written so far')

    await confirmRollback(main)
    await untilSettled(main, operationId)
    await expect.poll(() => copiedFiles(fixtureRoot).length, { timeout: waitBudget(15000) }).toBe(0)
    await dismissAllToasts(main)
  })

  test('a finished copy asks first: a no leaves it alone, a yes reverses it and says the client asked', async ({
    tauriPage,
  }) => {
    const main = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    await main.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null })`)
    const operationId = await startCopy(fixtureRoot)
    await untilSettled(main, operationId)
    await dismissAllToasts(main)
    await expect.poll(() => rollbackStateOf(operationId), { timeout: waitBudget(15000) }).toBe('rollbackable')

    expect(expectAwaitingPerson(await mcpCall('operations_rollback', { operationId }))).toBe('rollback-confirmation')
    expect(await rollbackBody(main)).toContain('the operation created')

    await mcpCall('dialog', { action: 'close', type: 'rollback-confirmation' })
    expect(await rollbackStateOf(operationId)).toBe('rollbackable')
    expect(copiedFiles(fixtureRoot).length).toBe(FILES_PER_SOURCE)

    expectAwaitingPerson(await mcpCall('operations_rollback', { operationId }))
    const answer = JSON.parse(await mcpCall('dialog_confirm', { type: 'rollback-confirmation' })) as {
      status: string
      inverseOpId: string
    }
    expect(answer.status).toBe('dispatched')
    expect(answer.inverseOpId).toBeTruthy()

    await expect.poll(() => rollbackStateOf(operationId), { timeout: waitBudget(15000) }).toBe('rolledBack')
    expect(copiedFiles(fixtureRoot).length).toBe(0)
    const inverse = JSON.parse(await mcpCall('operations_get', { operationId: answer.inverseOpId, limit: 1 })) as {
      operation: { initiator: string }
    }
    expect(inverse.operation.initiator).toBe('aiClient')

    // It's done, so a second go refuses, typed, and raises nothing.
    expect(await refusalOf('operations_rollback', { operationId })).toEqual({ refusal: 'alreadyRolledBack' })
    expect(await rollbackDialogs(main)).toBe(0)
    await dismissAllToasts(main)
  })

  test('what can not roll back refuses typed with no dialog, and another modal is named', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()

    expect(await refusalOf('operations_rollback', { operationId: 'no-such-operation' })).toEqual({
      refusal: 'unknownOperation',
    })
    expect(await rollbackDialogs(main)).toBe(0)

    // The old way past a person is refused by name, ❌ never ignored.
    const bypass = await mcpCallRaw('operations_rollback', { operationId: 'no-such-operation', autoConfirm: true })
    expect(bypass.error).toBeDefined()

    await main.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null })`)
    const operationId = await startCopy(fixtureRoot)
    await untilSettled(main, operationId)
    await dismissAllToasts(main)
    await expect.poll(() => rollbackStateOf(operationId), { timeout: waitBudget(15000) }).toBe('rollbackable')

    await mcpCall('dialog', { action: 'open', type: 'about' })
    expect(await refusalOf('operations_rollback', { operationId })).toEqual({ blockingDialog: 'about' })
    expect(await rollbackDialogs(main)).toBe(0)
    await mcpCall('dialog', { action: 'close', type: 'about' })

    expect(await rollbackStateOf(operationId)).toBe('rollbackable')
  })

  test('queue can no longer reverse anything or clear a lock', async () => {
    const running = await mcpCallRaw('queue', { action: 'cancel', operationId: 'op-any', rollback: true })
    expect(running.error).toBeDefined()
    const locked = await mcpCallRaw('queue', { action: 'move_locked', operationId: 'op-any' })
    expect(locked.error).toBeDefined()
  })
})

// `chflags` is the macOS Finder Locked flag; the Linux lane has no such item to move.
test.describe('MCP queue_move_locked', () => {
  test.skip(process.platform !== 'darwin', 'the Finder Locked flag is macOS-only')

  test('answers a move parked on a Locked item, which lands Locked', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    const name = 'locked-item.txt'
    const source = path.join(fixtureRoot, 'left', name)
    const destination = path.join(fixtureRoot, 'right', name)
    fs.writeFileSync(source, 'locked')
    execFileSync('chflags', ['uchg', source])
    try {
      await main.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null })`)
      await mcpNavToPath('left', path.join(fixtureRoot, 'left'))
      await mcpNavToPath('right', path.join(fixtureRoot, 'right'))
      await mcpCall('select', { pane: 'left', names: [name] })
      const operationId = parseOperationId(await mcpConfirmedMove('left'))

      await expect
        .poll(async () => (await mcpReadResource('cmdr://state?include=operations')).includes('pendingLockedSources'), {
          timeout: waitBudget(15000),
        })
        .toBeTruthy()

      expect(await mcpCall('queue_move_locked', { operationId })).toContain('moving its Locked items')
      await untilSettled(main, operationId)

      expect(fs.existsSync(source)).toBe(false)
      expect(fs.existsSync(destination)).toBe(true)
      expect(fs.statSync(destination).mode & 0o200).toBe(0o200)
      expect(execFileSync('ls', ['-lO', destination]).toString()).toContain('uchg')
      await dismissAllToasts(main)
    } finally {
      // Before the hooks: the restore can't remove a locked file.
      for (const file of [source, destination]) {
        if (fs.existsSync(file)) execFileSync('chflags', ['nouchg', file])
      }
      fs.rmSync(source, { force: true })
      fs.rmSync(destination, { force: true })
    }
  })
})

/** Reads the `status:` of a specific operationId out of the `operations:` YAML
 *  block. Returns `undefined` when the id isn't present. */
function operationsStatusFor(stateYaml: string, operationId: string): string | undefined {
  const lines = stateYaml.split('\n')
  const idIndex = lines.findIndex((l) => l.includes(`operationId: ${operationId}`))
  if (idIndex === -1) return undefined
  // status: rides within the same YAML list item, a couple of lines below the id.
  for (let i = idIndex + 1; i < Math.min(idIndex + 6, lines.length); i++) {
    const match = /^\s+status: (\w+)/.exec(lines[i])
    if (match) return match[1]
    if (/^\s+- operationId:/.test(lines[i])) break // next item; ours had no status
  }
  return undefined
}
