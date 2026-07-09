/**
 * E2E for the MCP operation-queue surface: the `queue` tool, the
 * `operations:` section of `cmdr://state`, and `await operation_complete`.
 *
 * Drives the whole agent loop end to end: start a throttled copy via the MCP
 * `copy` tool with `autoConfirm` (capturing the returned `operationId`), pause it
 * with `queue`, assert it shows `status: paused` in `cmdr://state operations`,
 * resume it, then `await operation_complete` for the same id.
 *
 * The copy source is a dedicated multi-file directory plus the per-file
 * `set_test_throttle`, for the same two reasons as `transfer-queue.spec.ts`: the
 * throttle sleeps once PER FILE (so many files keep the op in flight long enough
 * to observe), and pause gates BETWEEN files (so a one-file copy can't be paused
 * at all).
 *
 * Requires `--features playwright-e2e`.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureAppReady, expectAndDismissToast, getFixtureRoot } from './helpers.js'
import { ensureMcpClient, mcpCall, mcpReadResource, mcpNavToPath } from '../e2e-shared/mcp-client.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** Per-file copy throttle; with `FILES_PER_SOURCE` files this keeps the copy in
 *  flight for ~`FILES_PER_SOURCE * THROTTLE_MS` ms, generous room to pause/resume. */
const THROTTLE_MS = 250
const FILES_PER_SOURCE = 24
const SOURCE = 'mcp-queue-src'

test.setTimeout(90_000)

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

test.afterEach(async ({ tauriPage }) => {
  // Clear the throttle FIRST so any in-flight op winds down fast, cancel
  // everything, then WAIT for the lane to empty (cancel returns on REQUEST, not
  // on wind-down; a still-cancelling op leaves the lane busy for the next test).
  await tauriPage.evaluate(`(async function() {
    try { await window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: null }); } catch (e) {}
    try {
      var ops = await window.__TAURI_INTERNALS__.invoke('list_operations');
      var ids = ops.map(function(o) { return o.operationId; });
      if (ids.length) await window.__TAURI_INTERNALS__.invoke('cancel_operations', { operationIds: ids });
    } catch (e) {}
    for (var i = 0; i < 60; i++) {
      var remaining = await window.__TAURI_INTERNALS__.invoke('list_operations');
      if (!remaining || remaining.length === 0) break;
      await new Promise(function(r) { setTimeout(r, 100); });
    }
  })()`)
})

test.describe('MCP operation queue', () => {
  test('copy (autoConfirm) returns an operationId; queue pauses/resumes it; await operation_complete settles', async ({
    tauriPage,
  }) => {
    const main = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()

    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))
    await mcpNavToPath('right', path.join(fixtureRoot, 'right'))

    // Select the multi-file source dir in the left pane and copy it to the right.
    // `select` focuses the target pane, so the subsequent `copy` acts on it.
    await mcpCall('select', { pane: 'left', names: [SOURCE] })

    // Auto-confirmed copy returns the spawned operationId in its OK text.
    const copyResult = await mcpCall('copy', { autoConfirm: true })
    const operationId = parseOperationId(copyResult)

    // The op is discoverable in `cmdr://state operations` while it runs.
    await expect
      .poll(async () => (await mcpReadResource('cmdr://state?include=operations')).includes(operationId), {
        timeout: 15000,
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
        { timeout: 15000 },
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
        { timeout: 15000 },
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
