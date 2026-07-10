/**
 * E2E for the MCP agent-facing tool contracts: the flows an agent driving Cmdr
 * actually chains, end to end through the real server.
 *
 *   1. select-by-names → copy (autoConfirm) → await has_item on the destination
 *      → delete → await not_has_item. The canonical "act on specific files"
 *      loop, including the select round-trip's fresh-state guarantee (copy
 *      immediately after select must not read stale selection).
 *   2. Honest errors: select with a missing name, move_cursor to a missing
 *      filename / out-of-range index, copy with the cursor on `..` — each must
 *      fail with the real cause, never a false OK or a generic ack timeout.
 *   3. refresh as a round-trip that forces a backend re-read.
 *   4. The `operations:` section exists in `cmdr://state` (empty at idle).
 *
 * Unit coverage for the pieces lives in `mcp/executor/tests.rs` and
 * `pane-commands.test.ts`; this spec pins the cross-layer wiring (backend tool
 * → Tauri event → adapter → command bus → pane → state store → reply).
 */

import fs from 'fs'
import path from 'path'
import { test, expect } from './fixtures.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { ensureAppReady, expectAndDismissToast, getFixtureRoot } from './helpers.js'
import { ensureMcpClient, mcpCall, mcpReadResource, mcpNavToPath, mcpAwaitItem } from '../e2e-shared/mcp-client.js'

test.describe('MCP agent tools', () => {
  test.beforeEach(() => {
    recreateFixtures(getFixtureRoot())
  })

  test('select by names → copy → await → delete → await not_has_item', async ({ tauriPage }) => {
    // Chains five round-trips including two awaits (up to 15 s each); the suite
    // default of 15 s can't cover the worst case.
    test.setTimeout(60_000)
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))
    await mcpNavToPath('right', path.join(fixtureRoot, 'right'))

    // Select two specific files by name — no index bookkeeping.
    const selectResult = await mcpCall('select', { pane: 'left', names: ['file-a.txt', 'file-b.txt'] })
    expect(selectResult).toContain('OK')

    // Focus must follow the select (copy acts on the focused pane).
    // Copy IMMEDIATELY after select: the round-trip contract guarantees the
    // backend already holds the new selection, so this must not be rejected
    // as an empty operation.
    const copyResult = await mcpCall('copy', { autoConfirm: true })
    expect(copyResult).toContain('OK')

    await mcpAwaitItem('right', 'file-a.txt')
    await mcpAwaitItem('right', 'file-b.txt')
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'file-a.txt'))).toBe(true)
    expect(fs.existsSync(path.join(fixtureRoot, 'right', 'file-b.txt'))).toBe(true)
    await expectAndDismissToast(tauriPage, 'Copied 2 files')

    // Delete one of the copies and wait for ABSENCE — the await condition that
    // makes "did my delete finish?" a one-call check.
    await mcpCall('select', { pane: 'right', names: ['file-a.txt'] })
    const deleteResult = await mcpCall('delete', { autoConfirm: true })
    expect(deleteResult).toContain('OK')
    const gone = await mcpCall('await', {
      pane: 'right',
      condition: 'not_has_item',
      value: 'file-a.txt',
      timeoutSeconds: 15,
    })
    expect(gone).toContain('OK')
    await expectAndDismissToast(tauriPage, 'to trash')
  })

  test('honest errors: missing names, bad cursor targets, empty operations', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))

    // select: a missing name comes back named in the error
    await expect(mcpCall('select', { pane: 'left', names: ['file-a.txt', 'no-such-file.xyz'] })).rejects.toThrow(
      'no-such-file.xyz',
    )

    // move_cursor: missing filename is an error, not a false OK
    await expect(mcpCall('move_cursor', { pane: 'left', filename: 'no-such-file.xyz' })).rejects.toThrow('not found')

    // move_cursor: out-of-range index reports the range
    await expect(mcpCall('move_cursor', { pane: 'left', index: 99999 })).rejects.toThrow('out of range')

    // copy with nothing selected and the cursor on `..`: fast invalid_params
    // naming the real cause, not a "frontend may be stalled" ack timeout
    await mcpCall('select', { pane: 'left', count: 0 })
    await mcpCall('move_cursor', { pane: 'left', index: 0 })
    await expect(mcpCall('copy', { autoConfirm: true })).rejects.toThrow('Nothing to copy')
  })

  test('nav_to_path shifts focus so a follow-up create lands in the navigated pane', async ({ tauriPage }) => {
    // The focus-divergence regression: `select right` focuses the right pane in
    // the backend store; a same-volume `nav_to_path left` must shift FE focus to
    // the left pane too, so a `mkdir` with no explicit pane (which targets the
    // focused pane) creates in LEFT, not the previously-focused RIGHT. Before the
    // fix, FE focus stayed on right while `cmdr://state` reported left, and the
    // folder landed in the wrong pane.
    test.setTimeout(30_000)
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    // Two panes on DISTINCT dirs so the create is attributable: left starts at the
    // fixture root, right sits in the populated sub-dir (so the select can target a
    // real file there and focus the right pane).
    await mcpNavToPath('left', fixtureRoot)
    await mcpNavToPath('right', path.join(fixtureRoot, 'left', 'sub-dir'))

    // Focus the RIGHT pane via select, then navigate the LEFT pane (a same-volume,
    // in-place nav — the exact arm that used to skip the focus shift).
    await mcpCall('select', { pane: 'right', names: ['nested-file.txt'] })
    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))

    // No explicit pane → the focused pane, which must now be LEFT.
    const dirName = `mcp-focus-${String(Date.now())}`
    const mkdirResult = await mcpCall('mkdir', { name: dirName, autoConfirm: true })
    expect(mkdirResult).toContain('OK')

    expect(fs.existsSync(path.join(fixtureRoot, 'left', dirName))).toBe(true)
    // Before the focus fix, focus stayed on the right pane (sub-dir) and the folder
    // landed there instead.
    expect(fs.existsSync(path.join(fixtureRoot, 'left', 'sub-dir', dirName))).toBe(false)
  })

  test('mkdir with an explicit pane targets that pane regardless of focus', async ({ tauriPage }) => {
    // The pane param is the belt-and-suspenders guard: even with focus on LEFT,
    // `mkdir pane:right` creates in RIGHT, so creation never depends on focus timing.
    test.setTimeout(30_000)
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))
    await mcpNavToPath('right', path.join(fixtureRoot, 'right'))
    // Focus LEFT explicitly.
    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))

    const dirName = `mcp-pane-${String(Date.now())}`
    const mkdirResult = await mcpCall('mkdir', { name: dirName, autoConfirm: true, pane: 'right' })
    expect(mkdirResult).toContain('OK')

    expect(fs.existsSync(path.join(fixtureRoot, 'right', dirName))).toBe(true)
    expect(fs.existsSync(path.join(fixtureRoot, 'left', dirName))).toBe(false)
  })

  test('open_search_dialog then generic dialog close dismisses it', async ({ tauriPage }) => {
    // The generic soft-dialog close: `open_search_dialog` mounts the search dialog
    // (acks on SoftDialogAppeared 'search'), and `dialog close type:search` routes
    // through the generic path (validate id → mcp-close-dialog → the FE close registry
    // → the dialog's own close → SoftDialogDisappeared 'search'). Before the fix, only
    // a hardcoded subset of dialogs was closable and 'search' timed out.
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    const openResult = await mcpCall('open_search_dialog', { autoRun: false })
    expect(openResult).toContain('OK')
    await expect.poll(async () => tauriPage.isVisible('.search-overlay'), { timeout: 5000 }).toBeTruthy()

    const closeResult = await mcpCall('dialog', { action: 'close', type: 'search' })
    expect(closeResult).toContain('OK')
    await expect.poll(async () => !(await tauriPage.isVisible('.search-overlay')), { timeout: 5000 }).toBeTruthy()
  })

  test('refresh forces a backend re-read; transfers section exists', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    const fixtureRoot = getFixtureRoot()

    await mcpNavToPath('left', path.join(fixtureRoot, 'left'))

    // Create a file externally, then refresh: the round-trip re-read must
    // surface it even if the watcher hasn't fired yet.
    const fileName = `mcp-refresh-${String(Date.now())}.txt`
    fs.writeFileSync(path.join(fixtureRoot, 'left', fileName), 'refresh test')
    const refreshResult = await mcpCall('refresh', {})
    expect(refreshResult).toContain('re-read')
    await mcpAwaitItem('left', fileName, 5)

    // operations: present in the state resource, empty at idle
    const state = await mcpReadResource('cmdr://state?include=operations')
    expect(state).toContain('operations: []')
  })
})
