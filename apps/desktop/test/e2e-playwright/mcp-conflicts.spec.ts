/**
 * E2E for answering a name clash ONE FILE AT A TIME over MCP.
 *
 * This is the state that used to be unreachable from automation. `dialog
 * confirm` only offered the three bulk policies, so every agent-driven transfer
 * decided all its clashes before it started, and nothing automated could put an
 * operation into Stop mode and answer a single file. A wedging bug lived in that
 * blind spot for months: answering one clash swallowed the NEXT one and parked
 * the transfer forever. This spec drives exactly that path — and step 4 is the
 * regression anchor for the wedge, because it proves the second clash survives
 * the first one's answer and is separately answerable.
 *
 * The whole loop is MCP, nothing else: copy → confirm with `onConflict: stop` →
 * read the clash out of `cmdr://state` → `resolve_conflict` → read the NEXT
 * clash → answer that one too → `await operation_complete`.
 *
 * Requires `--features playwright-e2e`.
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureAppReady, expectAndDismissToast, getFixtureRoot } from './helpers.js'
import { ensureMcpClient, mcpCall, mcpCallRaw, mcpReadResource, mcpNavToPath } from '../e2e-shared/mcp-client.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'

/** The folder copied left → right. It exists on BOTH sides, so the copy merges
 *  and every file inside it clashes: several clashes, one operation. */
const SOURCE = 'mcp-conflict-src'
const FILE_COUNT = 3
/** Per-file throttle, so the operation can't race through every clash between
 *  two polls of `cmdr://state`. */
const THROTTLE_MS = 150

test.setTimeout(90_000)

/** One clash's worth of `pendingConflict:` YAML, parsed out of `cmdr://state`. */
interface PendingClash {
  operationId: string
  conflictId: number
  destination: string
}

test.beforeEach(async ({ tauriPage }) => {
  const fixtureRoot = getFixtureRoot()
  recreateFixtures(fixtureRoot)
  // Same names on both sides: the folders merge, each file inside clashes.
  makeClashingPair(fixtureRoot)
  await ensureAppReady(tauriPage)
  await ensureMcpClient(tauriPage)
  await tauriPage.evaluate(`window.__TAURI_INTERNALS__.invoke('set_test_throttle', { ms: ${String(THROTTLE_MS)} })`)
})

// ⚠️ ONE hook, drain THEN restore — the restore deletes this spec's own source
// dir, and a copy still reading it dies with a retained `SourceNotFound`. Same
// contract as `mcp-queue.spec.ts`; `DETAILS.md` § "The fixture-tree leak guard".
test.afterEach(async ({ tauriPage }) => {
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
  restoreFixtureTree(getFixtureRoot())
})

test.describe('MCP per-file conflicts', () => {
  test('a transfer can be left on stop, its clash read, and one file answered', async ({ tauriPage }) => {
    const main = tauriPage as TauriPage
    const fixtureRoot = getFixtureRoot()
    const destDir = path.join(fixtureRoot, 'right', SOURCE)

    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))
    await mcpNavToPath('right', path.join(fixtureRoot, 'right'))
    await mcpCall('select', { pane: 'left', names: [SOURCE] })

    // 1. Start the copy and leave the policy on stop: nothing is decided upfront.
    await mcpCall('copy', {})
    await mcpCall('dialog', { action: 'confirm', type: 'transfer-confirmation', onConflict: 'stop' })

    // 2. The parked clash is READABLE. Without this an agent sees a `running`
    //    row whose counters have stopped and no way to learn why.
    const first = await waitForPendingClash()
    expect(first.destination).toContain(SOURCE)

    // 3. Answer that ONE file, and learn what the answer did. `resolved` means
    //    this answer is the one the operation carried on with.
    const answered = await resolveConflict(first, 'skip')
    expect(answered.outcome).toBe('resolved')
    expect(answered.conflictId).toBe(first.conflictId)

    // 4. The regression anchor. The operation raises its NEXT clash, under its
    //    own id: answering the first one didn't swallow it, and the transfer
    //    isn't parked on a question nobody can answer.
    const second = await waitForPendingClash(first.conflictId)
    expect(second.operationId).toBe(first.operationId)
    expect(second.conflictId).toBeGreaterThan(first.conflictId)

    // 5. An answer for the RETIRED clash is refused as stale, not applied to
    //    the live one. This is the exact confusion the conflictId exists to
    //    remove, and the agent is told which one it is in typed data.
    const stale = await resolveConflictRaw(
      { operationId: first.operationId, conflictId: first.conflictId },
      'overwrite',
    )
    expect(stale.error?.data?.outcome).toBe('stale_answer')

    // 6. Answer the rest in one go and let the operation finish.
    const rest = await resolveConflict(second, 'skip', true)
    expect(rest.outcome).toBe('resolved')

    const settled = await mcpCall('await', {
      condition: 'operation_complete',
      value: first.operationId,
      timeoutSeconds: 30,
    })
    expect(settled).toContain('completed')

    // 7. Skip means skip: every destination file still holds what it held.
    for (let i = 0; i < FILE_COUNT; i++) {
      expect(fs.readFileSync(path.join(destDir, `file-${String(i)}.txt`), 'utf8')).toBe('destination')
    }

    await expectAndDismissToast(main, 'Copied')
  })
})

/** Creates `left/<SOURCE>/` and `right/<SOURCE>/` holding the same filenames
 *  with different content, so a copy merges and clashes on every file. */
function makeClashingPair(fixtureRoot: string): void {
  for (const [side, content] of [
    ['left', 'source'],
    ['right', 'destination'],
  ] as const) {
    const dir = path.join(fixtureRoot, side, SOURCE)
    fs.mkdirSync(dir, { recursive: true })
    for (let i = 0; i < FILE_COUNT; i++) {
      fs.writeFileSync(path.join(dir, `file-${String(i)}.txt`), content)
    }
  }
}

/** Polls `cmdr://state` until an operation is parked on a clash, optionally one
 *  whose id differs from `after` (used to wait for the NEXT clash). */
async function waitForPendingClash(after?: number): Promise<PendingClash> {
  const seen: PendingClash[] = []
  await expect
    .poll(
      async () => {
        const clash = parsePendingClash(await mcpReadResource('cmdr://state?include=operations'))
        if (!clash) return false
        if (after !== undefined && clash.conflictId === after) return false
        seen.push(clash)
        return true
      },
      { timeout: 20_000 },
    )
    .toBeTruthy()
  const captured = seen.at(-1)
  if (!captured) throw new Error('no pending clash was captured')
  return captured
}

/** Reads the one `pendingConflict:` block out of the `operations:` YAML. */
function parsePendingClash(stateYaml: string): PendingClash | null {
  const lines = stateYaml.split('\n')
  const blockAt = lines.findIndex((l) => l.includes('pendingConflict:'))
  if (blockAt === -1) return null
  // The operation this block belongs to is the nearest `- operationId:` above it.
  let operationId: string | undefined
  for (let i = blockAt; i >= 0; i--) {
    const match = /^\s+- operationId: (\S+)/.exec(lines[i])
    if (match) {
      operationId = match[1]
      break
    }
  }
  const conflictId = findWithin(lines, blockAt, /^\s+conflictId: (\d+)/)
  const destination = findWithin(lines, blockAt, /^\s+destination: "(.*)"/)
  if (operationId === undefined || conflictId === undefined || destination === undefined) return null
  return { operationId, conflictId: Number(conflictId), destination }
}

/** First capture of `pattern` in the handful of lines after `from`. */
function findWithin(lines: string[], from: number, pattern: RegExp): string | undefined {
  for (let i = from + 1; i < Math.min(from + 9, lines.length); i++) {
    const match = pattern.exec(lines[i])
    if (match) return match[1]
  }
  return undefined
}

interface ResolveConflictResult {
  outcome: string
  conflictId: number
}

/** Answers one clash and parses the tool's typed result. */
async function resolveConflict(
  clash: Pick<PendingClash, 'operationId' | 'conflictId'>,
  resolution: string,
  applyToAll = false,
): Promise<ResolveConflictResult> {
  const text = await mcpCall('resolve_conflict', {
    operationId: clash.operationId,
    conflictId: clash.conflictId,
    resolution,
    applyToAll,
  })
  return JSON.parse(text) as ResolveConflictResult
}

/** The same call, kept raw so the refusal's typed `data.outcome` survives. */
async function resolveConflictRaw(
  clash: Pick<PendingClash, 'operationId' | 'conflictId'>,
  resolution: string,
): Promise<{ error?: { data?: { outcome?: string } } }> {
  return mcpCallRaw('resolve_conflict', {
    operationId: clash.operationId,
    conflictId: clash.conflictId,
    resolution,
  })
}
